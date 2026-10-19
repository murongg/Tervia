mod process;
pub mod tools;
mod transport;

use lsp_types::{GotoDefinitionResponse, LocationLink, Position};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tools::{Launch, Progress, Provision, Service, Stage, Status, Tools};
use transport::Connection;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Server {
    pub program: String,
    pub args: Vec<String>,
}

impl Server {
    pub fn for_language(language: &str) -> Option<Self> {
        let (program, args): (_, &[&str]) = match language {
            "rust" => ("rust-analyzer", &[]),
            "javascript" | "typescript" | "tsx" => ("typescript-language-server", &["--stdio"]),
            "python" => ("pyright-langserver", &["--stdio"]),
            "go" => ("gopls", &[]),
            _ => return None,
        };
        Some(Self {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).into()).collect(),
        })
    }
}

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    pub language: String,
    pub text: String,
}

#[derive(Clone)]
pub struct Query {
    pub root: PathBuf,
    pub path: PathBuf,
    pub documents: Vec<Document>,
    /// UTF-8 byte offset in the source document; the wire protocol uses UTF-16.
    pub offset: usize,
}

type Response = Result<Vec<LocationLink>, String>;
enum Outcome {
    Complete(Response),
    StartFailed(String),
}
impl Outcome {
    fn response(self) -> Response {
        match self {
            Self::Complete(result) => result,
            Self::StartFailed(error) => Err(error),
        }
    }
    fn failed(&self) -> bool {
        matches!(self, Self::StartFailed(_) | Self::Complete(Err(_)))
    }
}
struct Job {
    server: Server,
    launch: Launch,
    progress: Progress,
    query: Arc<Query>,
    deadline: Instant,
    reply: mpsc::SyncSender<Outcome>,
}
struct Worker {
    jobs: mpsc::SyncSender<Job>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
struct Workers {
    stop: Arc<AtomicBool>,
    worker: Option<Worker>,
    launches: HashMap<Server, Launch>,
    progress: Progress,
}
impl Default for Workers {
    fn default() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            launches: HashMap::new(),
            progress: Arc::default(),
        }
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Calls block their caller and belong on a background executor. Only the first
/// definition request starts the worker; each project owns its independent client.
#[derive(Clone)]
pub struct Client {
    worker: Arc<Mutex<Workers>>,
    timeout: Duration,
    idle_timeout: Duration,
    tools: Arc<Tools>,
}

pub struct DefinitionRequest {
    client: Client,
    server: Server,
    query: Arc<Query>,
    stop: Arc<AtomicBool>,
    progress: Progress,
    cached: Option<Launch>,
}
impl DefinitionRequest {
    pub fn run(self) -> Response {
        self.client.resolve(&self)
    }
}
impl Default for Client {
    fn default() -> Self {
        Self::with_timeout(Duration::from_secs(30))
    }
}
impl Client {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            worker: Arc::default(),
            timeout,
            idle_timeout: Duration::from_secs(300),
            tools: Arc::new(Tools::default()),
        }
    }

    pub fn with_idle_timeout(timeout: Duration) -> Self {
        Self {
            idle_timeout: timeout,
            ..Self::default()
        }
    }

    pub fn with_tools(tools: Arc<Tools>) -> Self {
        Self {
            tools,
            ..Self::default()
        }
    }

    pub fn status(&self) -> Option<Status> {
        let progress = self.worker.lock().ok()?.progress.clone();
        progress.lock().ok().and_then(|status| status.clone())
    }

    pub fn definitions(&self, server: &Server, query: Query) -> Response {
        self.definition_request(server, query)?.run()
    }

    /// Capture ownership before queuing background work. No discovery, process
    /// launch or network access happens until the returned request is run.
    pub fn definition_request(
        &self,
        server: &Server,
        query: Query,
    ) -> Result<DefinitionRequest, String> {
        let (stop, cached, progress) = {
            let mut slot = self.worker.lock().map_err(|error| error.to_string())?;
            // Only the most recent lookup owns the visible preparation state.
            slot.progress = Arc::default();
            (
                slot.stop.clone(),
                slot.launches.get(server).cloned(),
                slot.progress.clone(),
            )
        };
        Ok(DefinitionRequest {
            client: self.clone(),
            server: server.clone(),
            query: Arc::new(query),
            stop,
            progress,
            cached,
        })
    }

    fn resolve(&self, request: &DefinitionRequest) -> Response {
        let server = &request.server;
        let query = request.query.clone();
        let stop = &request.stop;
        let progress = &request.progress;
        if stop.load(Ordering::Relaxed) {
            return Err("Language service preparation canceled".into());
        }
        // Preparation has its own budget and never runs under the worker lock.
        // Closing a project can cancel a download without waiting for its socket.
        let launch = match request.cached.clone() {
            Some(launch) => launch,
            None => self.tools.prepare(
                server,
                stop,
                Instant::now() + Duration::from_secs(900),
                progress,
            )?,
        };
        let outcome = self.execute(server, query.clone(), launch.clone(), stop, progress)?;
        if matches!(outcome, Outcome::StartFailed(_)) && launch.provision == Provision::System {
            // CLI presence/version alone cannot detect missing peer packages
            // or a broken launcher. Retry once with an isolated known version.
            let managed = self.tools.prepare_managed(
                server,
                stop,
                Instant::now() + Duration::from_secs(900),
                progress,
            )?;
            return self
                .execute(server, query, managed, stop, progress)?
                .response();
        }
        outcome.response()
    }

    fn execute(
        &self,
        server: &Server,
        query: Arc<Query>,
        launch: Launch,
        stop: &Arc<AtomicBool>,
        progress: &Progress,
    ) -> Result<Outcome, String> {
        let (reply, receive) = mpsc::sync_channel(1);
        {
            let mut slot = self.worker.lock().map_err(|error| error.to_string())?;
            if stop.load(Ordering::Relaxed) || !Arc::ptr_eq(stop, &slot.stop) {
                return Err("Language service preparation canceled".into());
            }
            slot.launches.insert(server.clone(), launch.clone());
            if slot.worker.is_none() {
                let (jobs, receiver) = mpsc::sync_channel::<Job>(8);
                let canceled = stop.clone();
                let idle_timeout = self.idle_timeout;
                let thread = thread::Builder::new()
                    .name("tervia-lsp".into())
                    .spawn(move || {
                        let mut sessions: HashMap<(PathBuf, Server), Session> = HashMap::new();
                        while !canceled.load(Ordering::Relaxed) {
                            sessions
                                .retain(|_, session| session.last_used.elapsed() < idle_timeout);
                            let job = match receiver.recv_timeout(Duration::from_millis(50)) {
                                Ok(job) => job,
                                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                                Err(_) => break,
                            };
                            let key = (job.query.root.clone(), job.server.clone());
                            if !sessions.contains_key(&key) {
                                match Session::start(
                                    &job.launch,
                                    &job.query.root,
                                    job.deadline,
                                    &canceled,
                                ) {
                                    Ok(session) => {
                                        sessions.insert(key.clone(), session);
                                    }
                                    Err(error) => {
                                        let _ = job.reply.send(Outcome::StartFailed(error));
                                        continue;
                                    }
                                }
                            }
                            let session = sessions.get_mut(&key).unwrap();
                            let result = session.definitions(
                                &job.query,
                                job.deadline,
                                &canceled,
                                &job.progress,
                            );
                            session.last_used = Instant::now();
                            if result.is_err() {
                                // Failed/expired transports are discarded so the next action can retry.
                                sessions.remove(&key);
                            }
                            let _ = job.reply.send(Outcome::Complete(result));
                        }
                        // Session destruction kills and reaps the owned servers on this worker.
                    })
                    .map_err(|error| error.to_string())?;
                slot.worker = Some(Worker {
                    jobs,
                    stop: stop.clone(),
                    thread: Some(thread),
                });
            }
            slot.worker
                .as_ref()
                .unwrap()
                .jobs
                .try_send(Job {
                    server: server.clone(),
                    launch,
                    progress: progress.clone(),
                    query,
                    deadline: Instant::now() + self.timeout,
                    reply,
                })
                .map_err(|_| "Language server is busy; try again".to_owned())?;
        }
        let result = receive
            .recv_timeout(self.timeout + Duration::from_secs(1))
            .map_err(|_| "Language server request timed out".to_owned())?;
        if result.failed()
            && let Ok(mut slot) = self.worker.lock()
            && Arc::ptr_eq(stop, &slot.stop)
        {
            slot.launches.remove(server);
        }
        Ok(result)
    }

    pub fn shutdown(&self) {
        let worker = self.worker.lock().ok().and_then(|mut slot| {
            slot.stop.store(true, Ordering::Relaxed);
            slot.stop = Arc::new(AtomicBool::new(false));
            slot.launches.clear();
            // Old download callbacks retain the old progress handle; they
            // cannot overwrite a reopened project's new preparation state.
            slot.progress = Arc::default();
            slot.worker.take()
        });
        drop(worker);
    }
}

struct Buffer {
    version: i32,
    text: String,
}
struct Session {
    connection: Connection,
    root: PathBuf,
    root_uri: String,
    documents: HashMap<String, Buffer>,
    next_id: u64,
    last_used: Instant,
    rust_ready: Option<bool>,
}
impl Session {
    fn start(
        launch: &Launch,
        root: &Path,
        deadline: Instant,
        stop: &Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let server = &launch.server;
        let root_uri = uri(&root)?;
        let mut session = Self {
            connection: Connection::start(server, &root, deadline, stop.clone(), &launch.paths)?,
            root,
            root_uri,
            documents: HashMap::new(),
            next_id: 0,
            last_used: Instant::now(),
            rust_ready: None,
        };
        let initialized = session.request("initialize", json!({
            "processId": std::process::id(),
            "clientInfo": {"name": "Tervia", "version": env!("CARGO_PKG_VERSION")},
            "rootUri": session.root_uri,
            "workspaceFolders": session.folders(),
            "capabilities": {
                "general": {"positionEncodings": ["utf-16"]},
                "experimental": {"serverStatusNotification": true},
                "workspace": {"configuration": true, "workspaceFolders": true, "applyEdit": false},
                "textDocument": {"definition": {"linkSupport": true}, "synchronization": {"dynamicRegistration": false}}
            }
        }), deadline, stop)?;
        session.rust_ready = (initialized["serverInfo"]["name"] == "rust-analyzer")
            .then_some(session.rust_ready.unwrap_or(false));
        let capabilities = &initialized["capabilities"];
        if capabilities["definitionProvider"].is_null()
            || capabilities["definitionProvider"] == false
        {
            return Err(format!(
                "{} does not support Go to Definition",
                server.program
            ));
        }
        if capabilities
            .get("positionEncoding")
            .and_then(Value::as_str)
            .is_some_and(|encoding| encoding != "utf-16")
        {
            return Err("Language server did not negotiate UTF-16 positions".into());
        }
        session.notify("initialized", json!({}))?;
        Ok(session)
    }

    fn folders(&self) -> Value {
        json!([{"uri": self.root_uri, "name": self.root.file_name().unwrap_or_default().to_string_lossy()}])
    }

    fn definitions(
        &mut self,
        query: &Query,
        deadline: Instant,
        stop: &AtomicBool,
        progress: &Progress,
    ) -> Response {
        self.connection.set_deadline(deadline);
        let source = query
            .documents
            .iter()
            .find(|document| document.path == query.path)
            .ok_or("Source document is not open")?;
        let position = position(&source.text, query.offset)?;
        let source_uri = self.document_uri(&query.path)?;
        let mut documents = HashMap::new();
        for document in &query.documents {
            documents.insert(self.document_uri(&document.path)?, document);
        }
        for closed in self
            .documents
            .keys()
            .filter(|key| !documents.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>()
        {
            self.notify(
                "textDocument/didClose",
                json!({"textDocument": {"uri": closed}}),
            )?;
            self.documents.remove(&closed);
        }
        for (uri, document) in documents {
            let version =
                match self.documents.get(&uri) {
                    Some(previous) if previous.text == document.text => continue,
                    Some(previous) => {
                        let version = previous
                            .version
                            .checked_add(1)
                            .ok_or("Document version exhausted")?;
                        // A range-less content change replaces the whole document, including
                        // for incremental servers. Its text always comes from the captured draft.
                        self.notify(
                            "textDocument/didChange",
                            json!({
                                "textDocument": {"uri": uri, "version": version},
                                "contentChanges": [{"text": document.text}]
                            }),
                        )?;
                        version
                    }
                    None => {
                        let jsx = document
                            .path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsx"));
                        let language = match document.language.as_str() {
                            "tsx" => "typescriptreact",
                            "javascript" if jsx => "javascriptreact",
                            other => other,
                        };
                        self.notify("textDocument/didOpen", json!({"textDocument": {
                            "uri": uri, "languageId": language, "version": 1, "text": document.text
                        }}))?;
                        1
                    }
                };
            self.documents.insert(
                uri,
                Buffer {
                    version,
                    text: document.text.clone(),
                },
            );
        }
        if self.rust_ready == Some(false) {
            // rust-analyzer acknowledges initialization before loading the
            // workspace. Its quiescence notification separates indexing from
            // a real "no definition" result on the first lookup.
            if let Ok(mut status) = progress.lock() {
                *status = Some(Status {
                    service: Service::Rust,
                    stage: Stage::Checking,
                });
            }
            let ready = (|| {
                while self.rust_ready == Some(false) {
                    let message = self.connection.receive(deadline, stop)?;
                    if message.get("method").is_some() {
                        self.server_request(&message)?;
                    }
                }
                Ok::<_, String>(())
            })();
            if let Ok(mut status) = progress.lock()
                && status.as_ref().is_some_and(|status| {
                    status.service == Service::Rust && status.stage == Stage::Checking
                })
            {
                *status = None;
            }
            ready?;
        }
        let result = self.request("textDocument/definition", json!({
            "textDocument": {"uri": source_uri}, "position": {"line": position.line, "character": position.character}
        }), deadline, stop)?;
        if result.is_null() {
            return Ok(Vec::new());
        }
        let response: GotoDefinitionResponse = serde_json::from_value(result)
            .map_err(|error| format!("Invalid definition response: {error}"))?;
        let locations = match response {
            GotoDefinitionResponse::Link(links) => return Ok(links),
            GotoDefinitionResponse::Scalar(location) => vec![location],
            GotoDefinitionResponse::Array(locations) => locations,
        };
        Ok(locations
            .into_iter()
            .map(|location| LocationLink {
                origin_selection_range: None,
                target_uri: location.uri,
                target_range: location.range,
                target_selection_range: location.range,
            })
            .collect())
    }

    fn document_uri(&self, path: &Path) -> Result<String, String> {
        if path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("Source is outside the project".into());
        }
        uri(&self.root.join(path))
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.connection
            .send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        deadline: Instant,
        stop: &AtomicBool,
    ) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.connection
            .send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let message = self.connection.receive(deadline, stop)?;
            if message.get("method").is_some() {
                self.server_request(&message)?;
            } else if message["id"] == id {
                if let Some(error) = message.get("error") {
                    return Err(format!(
                        "Language server: {}",
                        error["message"].as_str().unwrap_or("request failed")
                    ));
                }
                return message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "Invalid language server response".into());
            }
        }
    }

    fn server_request(&mut self, message: &Value) -> Result<(), String> {
        if message["method"] == "experimental/serverStatus" {
            if let Some(ready) = message["params"]["quiescent"].as_bool() {
                self.rust_ready = Some(ready);
            }
            return Ok(());
        }
        let Some(id) = message.get("id") else {
            return Ok(());
        };
        let result = match message["method"].as_str().unwrap_or_default() {
            "workspace/configuration" => json!(message["params"]["items"].as_array().unwrap_or(&vec![]).iter().map(|_| Value::Null).collect::<Vec<_>>()),
            "workspace/workspaceFolders" => self.folders(),
            "workspace/applyEdit" => json!({"applied": false, "failureReason": "Definition navigation does not apply workspace edits"}),
            "client/registerCapability" | "client/unregisterCapability" | "window/workDoneProgress/create" | "window/showMessageRequest" => Value::Null,
            _ => return self.connection.send(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not supported"}})),
        };
        self.connection
            .send(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }
}

pub fn position(text: &str, offset: usize) -> Result<Position, String> {
    let before = text.get(..offset).ok_or("Invalid source position")?;
    let line = before.bytes().filter(|byte| *byte == b'\n').count();
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .encode_utf16()
        .count();
    Ok(Position::new(
        u32::try_from(line).map_err(|_| "Source is too large")?,
        u32::try_from(column).map_err(|_| "Source line is too large")?,
    ))
}

fn uri(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(String::from)
        .map_err(|_| "Invalid file path".into())
}

pub fn target_path(root: &Path, uri: &str) -> Result<PathBuf, String> {
    let project = root.canonicalize().map_err(|error| error.to_string())?;
    let path = url::Url::parse(uri)
        .map_err(|error| error.to_string())?
        .to_file_path()
        .map_err(|_| "Only local file definitions can be opened")?;
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    let resolved = canonical
        .strip_prefix(&project)
        .map_err(|_| "Definition is outside the current project")?;
    // Validate the resolved boundary without losing the URI spelling used by
    // an already-open symlink buffer and its unsaved source positions.
    let relative = path
        .strip_prefix(root)
        .or_else(|_| path.strip_prefix(&project))
        .ok()
        .filter(|path| {
            !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        })
        .unwrap_or(resolved);
    Ok(relative.into())
}
