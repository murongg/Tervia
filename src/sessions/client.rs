use super::*;
use crate::{agent::Agent, pty::PtyEvent};
use std::{
    fs::File,
    io::BufReader,
    net::TcpStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub struct Connection {
    sender: Option<mpsc::SyncSender<Command>>,
    stop: Arc<AtomicBool>,
    identity: Arc<Mutex<Option<Agent>>>,
    pid: Option<u32>,
    target: PathBuf,
    supports_colors: bool,
    shell: String,
}

struct Command {
    value: Value,
    completion: Option<mpsc::SyncSender<Result<(), String>>>,
}

impl Connection {
    pub fn shell(&self) -> &str {
        &self.shell
    }
    pub fn flush(&self) -> Result<(), String> {
        let Some(sender) = &self.sender else {
            return Ok(());
        };
        if self.target.join("finished.json").exists() {
            return Ok(());
        }
        let (done, reply) = mpsc::sync_channel(1);
        match sender.try_send(Command {
            value: json!({"op":"status"}),
            completion: Some(done),
        }) {
            Ok(()) => reply
                .recv_timeout(Duration::from_secs(3))
                .map_err(|error| format!("Waiting for terminal input to be preserved: {error}"))?,
            Err(mpsc::TrySendError::Disconnected(_)) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => {
                Err("Terminal input is still being sent; wait before quitting".into())
            }
        }
    }
    pub fn foreground_agent(&self) -> Option<Agent> {
        *self.identity.lock().unwrap()
    }
    pub fn process_id(&self) -> Option<u32> {
        self.pid
    }
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        // Packet bounds apply per chunk, while large bracketed pastes retain byte order.
        for bytes in bytes.chunks(65536) {
            self.send(json!({"op":"input","bytes":bytes}))?;
        }
        Ok(())
    }
    pub fn resize(
        &self,
        columns: usize,
        rows: usize,
        width: u16,
        height: u16,
    ) -> Result<(), String> {
        self.send(json!({"op":"resize","cols":columns,"rows":rows,"width":width,"height":height}))
    }
    pub fn terminate(&self) -> Result<(), String> {
        terminate(&self.target).map_err(|error| error.to_string())
    }
    pub fn clear(&self) -> Result<(), String> {
        self.send(json!({"op":"clear"}))
    }

    pub fn colors(&self, colors: Vec<[u8; 3]>) -> Result<(), String> {
        // New interfaces can reconnect to hosts from an earlier app version without requiring optional operations.
        if !self.supports_colors {
            return Ok(());
        }
        self.send(json!({"op":"colors","colors":colors}))
    }
    fn send(&self, value: Value) -> Result<(), String> {
        self.sender
            .as_ref()
            .ok_or("Terminal has ended")?
            .try_send(Command {
                value,
                completion: None,
            })
            .map_err(|error| error.to_string())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.sender.take();
    }
}

fn metadata(directory: &Path) -> io::Result<Value> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(io::Error::other("Invalid terminal directory"));
    }
    let bytes = fs::read(directory.join("endpoint.json"))?;
    if bytes.len() > MAX_PACKET as usize {
        return Err(io::Error::other("Oversized terminal endpoint"));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    if value["version"].as_u64() != Some(VERSION) {
        return Err(io::Error::other("Incompatible terminal protocol"));
    }
    Ok(value)
}

pub(super) fn has_exited(directory: &Path, process_exited: impl FnOnce(u32) -> bool) -> bool {
    if directory.join("finished.json").is_file() || matches!(directory.try_exists(), Ok(false)) {
        return true;
    }
    // A reconnect/lease failure is not proof of exit. Unknown metadata or process permissions keep the tab.
    metadata(directory)
        .ok()
        .and_then(|value| value["pid"].as_u64())
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .is_some_and(process_exited)
}

fn socket(metadata: &Value) -> io::Result<BufReader<TcpStream>> {
    let port = metadata["port"]
        .as_u64()
        .filter(|port| *port > 0 && *port <= 65535)
        .ok_or_else(|| io::Error::other("Invalid terminal port"))? as u16;
    let stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(2),
    )?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(BufReader::new(stream))
}

fn request(
    stream: &mut BufReader<TcpStream>,
    metadata: &Value,
    mut value: Value,
) -> io::Result<Value> {
    value["version"] = json!(VERSION);
    value["token"] = metadata["token"].clone();
    host::reply(stream.get_mut(), &value)?;
    let response = host::packet(stream)?;
    if response["ok"] != true {
        return Err(io::Error::other(
            response["error"]
                .as_str()
                .unwrap_or("Terminal request failed"),
        ));
    }
    Ok(response)
}

pub(super) fn terminate(directory: &Path) -> io::Result<()> {
    if directory.join("finished.json").is_file() {
        fs::remove_dir_all(directory)?;
    } else {
        let meta = metadata(directory)?;
        request(&mut socket(&meta)?, &meta, json!({"op":"close"}))?;
    }
    #[cfg(target_os = "macos")]
    super::macos::cleanup(directory);
    Ok(())
}

fn attach(
    directory: &Path,
    meta: &Value,
    colors: &[[u8; 3]],
) -> io::Result<(Option<BufReader<TcpStream>>, Value)> {
    let path = directory.join("finished.json");
    let completed = || -> io::Result<_> { Ok((None, serde_json::from_slice(&fs::read(&path)?)?)) };
    if path.is_file() {
        return completed();
    }
    let live = (|| {
        let mut stream = socket(meta)?;
        // A just-dropped client may still be releasing its lease on the host thread.
        let deadline = Instant::now() + Duration::from_secs(2);
        let response = loop {
            match request(&mut stream, meta, json!({"op":"attach","colors":colors})) {
                Ok(value) => break value,
                Err(error)
                    if error.to_string().contains("already attached")
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error),
            }
        };
        Ok((Some(stream), response))
    })();
    match live {
        // The final journal event can reach a client just before the finished marker.
        // A failed attach during shutdown must replay that marker, never start a replacement shell.
        Err(_) if path.is_file() => completed(),
        result => result,
    }
}

pub(super) fn connect(directory: &Path, colors: &[[u8; 3]]) -> io::Result<Opened> {
    let meta = metadata(directory)?;
    let (mut stream, response) = attach(directory, &meta, colors)?;
    let finished = stream.is_none();
    let replay_length = response["length"]
        .as_u64()
        .ok_or_else(|| io::Error::other("Invalid terminal replay boundary"))?;
    let pid = meta["pid"].as_u64().map(|pid| pid as u32);
    let tracker = meta["integration"]
        .as_str()
        .map(|token| Tracker::new(token.to_owned(), meta["shell"].as_str().unwrap_or("")));
    let stop = Arc::new(AtomicBool::new(false));
    let identity = Arc::new(Mutex::new(
        response["agent"].as_str().and_then(Agent::from_executable),
    ));
    let (events, receiver) = async_channel::bounded(64);
    let (sender, commands) = mpsc::sync_channel::<Command>(1024);
    if let Some(mut stream) = stream.take() {
        let identity = identity.clone();
        let stop = stop.clone();
        let events = events.clone();
        let meta = meta.clone();
        std::thread::spawn(move || {
            // Drain accepted input/resize requests before releasing the lease, even if the GUI has detached.
            loop {
                let command = match commands.recv_timeout(Duration::from_millis(400)) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Timeout) => Command {
                        value: json!({"op":"status"}),
                        completion: None,
                    },
                    Err(_) => break,
                };
                let result = request(&mut stream, &meta, command.value);
                if let Some(completion) = command.completion {
                    let _ =
                        completion.send(result.as_ref().map(|_| ()).map_err(ToString::to_string));
                }
                match result {
                    Ok(response) => {
                        *identity.lock().unwrap() =
                            response["agent"].as_str().and_then(Agent::from_executable)
                    }
                    Err(error) => {
                        if !stop.load(Ordering::Acquire) {
                            deliver(&events, PtyEvent::Error(error.to_string()), &stop);
                        }
                        break;
                    }
                }
            }
        });
    }
    let mut file = File::open(directory.join("output"))?;
    let reader_stop = stop.clone();
    std::thread::spawn(move || {
        let mut offset = 0;
        let mut replaying = true;
        while !reader_stop.load(Ordering::Acquire) {
            if replaying && offset >= replay_length {
                if !deliver(&events, PtyEvent::ReplayComplete, &reader_stop) {
                    break;
                }
                replaying = false;
            }
            match journal::read(&mut file, &mut offset) {
                Ok(Some(event)) => {
                    let ended = matches!(event, PtyEvent::Exited(_));
                    if !deliver(&events, event, &reader_stop) || ended {
                        break;
                    }
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(8)),
                Err(error) => {
                    deliver(&events, PtyEvent::Error(error.to_string()), &reader_stop);
                    break;
                }
            }
        }
    });
    Ok((
        Connection {
            sender: (!finished).then_some(sender),
            stop,
            identity,
            pid,
            target: directory.to_owned(),
            shell: meta["shell"].as_str().unwrap_or("").to_owned(),
            supports_colors: response["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "colors")),
        },
        receiver,
        tracker,
    ))
}

fn deliver(
    sender: &async_channel::Sender<PtyEvent>,
    mut event: PtyEvent,
    stop: &AtomicBool,
) -> bool {
    while !stop.load(Ordering::Acquire) {
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(async_channel::TrySendError::Full(value)) => {
                event = value;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_errors_without_a_finished_marker_remain_errors() {
        use std::net::TcpListener;
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let meta = json!({"version":VERSION,"port":listener.local_addr().unwrap().port(),"token":"mock-token"});
        runtime::write_private(&root.path().join("endpoint.json"), &meta).unwrap();
        let server = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            host::packet(&mut reader).unwrap();
            host::reply(
                reader.get_mut(),
                &json!({"ok":false,"error":"mock attach rejected"}),
            )
            .unwrap();
        });
        let error = connect(root.path(), &[]).err().unwrap();
        server.join().unwrap();
        assert!(error.to_string().contains("mock attach rejected"));
        assert!(!root.path().join("finished.json").exists());
    }

    #[test]
    fn host_finishing_during_attach_falls_back_to_its_completed_journal() {
        use std::net::TcpListener;
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let meta = json!({"version":VERSION,"port":listener.local_addr().unwrap().port(),"token":"mock-token","shell":"/bin/bash","pid":0});
        runtime::write_private(&root.path().join("endpoint.json"), &meta).unwrap();
        let mut journal = journal::Journal::create(&root.path().join("output")).unwrap();
        journal.append(0, b"mock-completed-output").unwrap();
        journal.append(2, b"Success").unwrap();
        let mut finished = meta.clone();
        finished["length"] = json!(journal.length);
        let directory = root.path().to_owned();
        let server = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            assert_eq!(host::packet(&mut reader).unwrap()["op"], "attach");
            // Reproduce the real transition: the endpoint closes after the final marker is published.
            runtime::write_private(&directory.join("finished.json"), &finished).unwrap();
        });
        let connected = connect(root.path(), &[]);
        server.join().unwrap();
        let (session, events, _) = connected.unwrap();
        assert!(session.sender.is_none());
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut output = false;
        let mut exited = false;
        while Instant::now() < deadline && !exited {
            match events.try_recv() {
                Ok(PtyEvent::Data(bytes)) => output |= bytes == b"mock-completed-output",
                Ok(PtyEvent::Exited(status)) => {
                    assert_eq!(status, "Success");
                    exited = true;
                }
                Ok(PtyEvent::Error(error)) => panic!("{error}"),
                _ => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        assert!(output && exited);
    }

    #[test]
    fn recovery_discards_only_destroyed_or_confirmed_exited_processes() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("mock-session");
        assert!(has_exited(&directory, |_| panic!("No process to inspect")));
        fs::create_dir(&directory).unwrap();
        assert!(!has_exited(&directory, |_| false));
        fs::write(directory.join("endpoint.json"), "invalid mock metadata").unwrap();
        assert!(!has_exited(&directory, |_| false));
        fs::write(
            directory.join("endpoint.json"),
            json!({"version":VERSION,"pid":12345}).to_string(),
        )
        .unwrap();
        assert!(!has_exited(&directory, |pid| {
            assert_eq!(pid, 12345);
            false
        }));
        assert!(has_exited(&directory, |pid| {
            assert_eq!(pid, 12345);
            true
        }));
        fs::write(directory.join("finished.json"), "{}").unwrap();
        assert!(has_exited(&directory, |_| false));
    }
}
