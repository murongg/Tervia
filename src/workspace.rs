use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::state::Workspace;
mod catalog;
mod recovery;
mod startup;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

/// Shared persistence state serializes background writes and explicit durable saves.
#[derive(Clone)]
pub struct Store {
    storage: Arc<Mutex<Storage>>,
    revision: Arc<AtomicU64>,
    metadata: Arc<Mutex<Arc<Metadata>>>,
}

pub struct PreparedSave {
    owner: Store,
    snapshot: Result<Value, String>,
    revision: u64,
}

#[derive(Default)]
struct Metadata {
    fallbacks: BTreeMap<PathBuf, PathBuf>,
    orphans: Vec<Value>,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            storage: Arc::new(Mutex::new(Storage::new(path))),
            revision: Arc::default(),
            metadata: Arc::default(),
        }
    }

    pub fn load(&mut self) -> Result<Workspace, String> {
        self.revision.fetch_add(1, Ordering::AcqRel);
        let mut storage = self.storage.lock().unwrap();
        let result = storage.load();
        *self.metadata.lock().unwrap() = storage.metadata.clone();
        self.revision.fetch_add(1, Ordering::AcqRel);
        result
    }

    pub fn save(&mut self, workspace: &Workspace) -> Result<(), String> {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.storage.lock().unwrap().save(workspace)
    }

    /// Capture on the UI thread; JSON text encoding and disk IO run when the request is saved.
    pub fn prepare(&self, workspace: &Workspace) -> PreparedSave {
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        // Metadata has its own short-lived lock: capture must never wait for disk IO.
        let metadata = self.metadata.lock().unwrap().clone();
        PreparedSave {
            owner: self.clone(),
            snapshot: snapshot(workspace, &metadata.fallbacks, &metadata.orphans)
                .map_err(|error| error.to_string()),
            revision,
        }
    }
}

impl PreparedSave {
    pub fn save(self) -> Result<(), String> {
        if self.owner.revision.load(Ordering::Acquire) != self.revision {
            return Ok(());
        }
        let mut storage = self.owner.storage.lock().unwrap();
        // Check after acquiring the writer too: an explicit close/save may have
        // superseded this request while it waited, and must remain durable.
        if self.owner.revision.load(Ordering::Acquire) != self.revision {
            return Ok(());
        }
        storage.write(self.snapshot)
    }
}

/// App-owned project history, stored separately from the user-editable configuration.
struct Storage {
    path: PathBuf,
    source: Option<String>,
    saved: Option<Value>,
    metadata: Arc<Metadata>,
}

impl Storage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            source: None,
            saved: None,
            metadata: Arc::default(),
        }
    }

    pub fn load(&mut self) -> Result<Workspace, String> {
        self.saved = None;
        self.source = None;
        self.metadata = Arc::default();
        let mut orphans = Vec::new();
        if fs::metadata(&self.path).is_ok_and(|metadata| metadata.len() > 256 * 1024 * 1024) {
            return Err(self.error("Recovery file exceeds 256 MiB"));
        }
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => Some(source),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(self.error(error)),
        };
        let (mut workspace, fallbacks) = match &source {
            Some(source) => restore(source).map_err(|error| self.error(error))?,
            None => (Workspace::empty(), BTreeMap::new()),
        };
        if let Some(source) = &source {
            let value = serde_json::from_str(source).map_err(|error| self.error(error))?;
            orphans =
                recovery::restore(&value, &mut workspace).map_err(|error| self.error(error))?;
            catalog::restore(&value, &mut workspace).map_err(|error| self.error(error))?;
            // Restore the screen last: project/catalog restoration temporarily changes selection.
            startup::restore(&value, &mut workspace).map_err(|error| self.error(error))?;
        }
        self.saved =
            Some(snapshot(&workspace, &fallbacks, &orphans).map_err(|error| self.error(error))?);
        self.metadata = Arc::new(Metadata { fallbacks, orphans });
        self.source = source;
        Ok(workspace)
    }

    pub fn save(&mut self, workspace: &Workspace) -> Result<(), String> {
        if self.saved.is_none() {
            return Err(self.error("Workspace was not loaded; fix the file and restart Tervia"));
        }
        self.write(
            snapshot(workspace, &self.metadata.fallbacks, &self.metadata.orphans)
                .map_err(|error| error.to_string()),
        )
    }

    fn write(&mut self, snapshot: Result<Value, String>) -> Result<(), String> {
        if self.saved.is_none() {
            return Err(self.error("Workspace was not loaded; fix the file and restart Tervia"));
        }
        let snapshot = snapshot.map_err(|error| self.error(error))?;
        // UI notifications also cover terminal output and Git polling; unchanged history needs no IO.
        if self.saved.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        if self.current()? != self.source {
            return Err(
                self.error("Workspace changed on disk; restart Tervia before saving projects")
            );
        }
        let target = if self.source.is_some() {
            fs::canonicalize(&self.path).map_err(|error| self.error(error))?
        } else {
            self.path.clone()
        };
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|error| self.error(error))?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| self.error(error))?;
        if self.source.is_some() {
            temporary
                .as_file()
                .set_permissions(
                    fs::metadata(&target)
                        .map_err(|error| self.error(error))?
                        .permissions(),
                )
                .map_err(|error| self.error(error))?;
        }
        let updated = serde_json::to_string_pretty(&snapshot).map_err(|error| self.error(error))?;
        if updated.len() > 256 * 1024 * 1024 {
            return Err(self.error("Recovery state exceeds 256 MiB"));
        }
        temporary
            .write_all(updated.as_bytes())
            .map_err(|error| self.error(error))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| self.error(error))?;
        if self.current()? != self.source {
            return Err(
                self.error("Workspace changed on disk; restart Tervia before saving projects")
            );
        }
        if self.source.is_none() {
            // Two first launches must not silently replace each other's initial project lists.
            temporary
                .persist_noclobber(&target)
                .map_err(|error| self.error(error))?;
        } else {
            temporary
                .persist(&target)
                .map_err(|error| self.error(error))?;
        }
        self.source = Some(updated);
        self.saved = Some(snapshot);
        Ok(())
    }

    fn current(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.path) {
            Ok(source) => Ok(Some(source)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(self.error(error)),
        }
    }

    fn error(&self, error: impl std::fmt::Display) -> String {
        format!("{}: {error}", self.path.display())
    }
}

fn snapshot(
    workspace: &Workspace,
    fallbacks: &BTreeMap<PathBuf, PathBuf>,
    orphans: &[Value],
) -> Result<Value, serde_json::Error> {
    let mut connections = std::collections::BTreeSet::new();
    for session in workspace
        .projects
        .iter()
        .flat_map(|project| &project.terminal.sessions)
        .chain(&workspace.local_terminal.sessions)
    {
        if let Some(id) = &session.connection
            && (!crate::state::Session::valid_connection(id) || !connections.insert(id))
        {
            return Err(serde_json::Error::io(io::Error::new(
                io::ErrorKind::InvalidData,
                "A terminal connection must have exactly one workspace owner",
            )));
        }
    }
    let projects = workspace
        .all_project_rows()
        .into_iter()
        .map(|index| {
            let project = &workspace.projects[index];
            // Startup Git discovery is asynchronous; early UI saves must retain the main-directory fallback.
            let root = if project.repository.is_none() {
                fallbacks
                    .get(&project.root)
                    .unwrap_or(workspace.project_root(index))
            } else {
                workspace.project_root(index)
            };
            let root = serde_json::to_value(root)?;
            let directory =
                serde_json::to_value(&workspace.projects[workspace.project_target(index)].root)?;
            let mut saved = json!({ "root": root, "directory": directory });
            if workspace.folder(index) == Some(index) {
                saved["folder"] = json!(true);
                saved["children_expanded"] = json!(workspace.folder_expanded(index));
                saved["directories"] = serde_json::to_value(
                    workspace
                        .saved_directories(index)
                        .iter()
                        .map(|index| &workspace.projects[*index].root)
                        .collect::<Vec<_>>(),
                )?;
            }
            Ok(saved)
        })
        .collect::<Result<Vec<Value>, serde_json::Error>>()?;
    let active = workspace
        .projects
        .get(workspace.active_project)
        .filter(|_| workspace.project_visible(workspace.active_project))
        .map(|project| &project.root);
    Ok(json!({
        "version": 2,
        "projects": projects,
        "active": serde_json::to_value(active)?,
        "sidebar_expanded": workspace.sidebar_expanded,
        "window_maximized": workspace.window_maximized,
        "contexts": recovery::snapshot(workspace, orphans)?,
        "catalog": catalog::snapshot(workspace)?,
        "startup": startup::snapshot(workspace)?,
    }))
}

fn restore(source: &str) -> Result<(Workspace, BTreeMap<PathBuf, PathBuf>), String> {
    let saved: Value =
        serde_json::from_str(source).map_err(|error| format!("Invalid workspace JSON: {error}"))?;
    if !matches!(saved.get("version").and_then(Value::as_u64), Some(1 | 2)) {
        return Err("Unsupported workspace version".into());
    }
    let projects = saved
        .get("projects")
        .and_then(Value::as_array)
        .ok_or("projects must be an array")?;
    let active = saved
        .get("active")
        .filter(|value| !value.is_null())
        .map(directory)
        .transpose()?;
    let mut workspace = Workspace::empty();
    workspace.sidebar_expanded = match saved.get("sidebar_expanded") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or("sidebar_expanded must be a boolean")?,
    };
    workspace.window_maximized = match saved.get("window_maximized") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or("window_maximized must be a boolean")?,
    };
    let mut selected = 0;
    let mut fallbacks = BTreeMap::new();
    for project in projects {
        let root = directory(project.get("root").ok_or("Project root is missing")?)?;
        let target = directory(
            project
                .get("directory")
                .ok_or("Project directory is missing")?,
        )?;
        let folder = project
            .get("folder")
            .map(|value| value.as_bool().ok_or("folder must be a boolean"))
            .transpose()?
            .unwrap_or(false);
        if folder {
            let parent = if root.is_dir() { &root } else { &target };
            if !parent.is_dir() {
                continue;
            }
            let parent = fs::canonicalize(parent).map_err(|error| error.to_string())?;
            let parent_index = workspace.register_directory(parent);
            let index = if target.is_dir() {
                workspace.register_directory(
                    fs::canonicalize(&target).map_err(|error| error.to_string())?,
                )
            } else {
                parent_index
            };
            workspace.group_folder(parent_index, &[index]);
            // Older history only stored the selected directory; keep it in the new explicit list.
            workspace.attach_directory(parent_index, index);
            if let Some(directories) = project.get("directories") {
                let directories = directories
                    .as_array()
                    .ok_or("directories must be an array")?;
                for saved in directories {
                    let path = directory(saved)?;
                    if path.is_dir() {
                        let path = fs::canonicalize(&path).map_err(|error| error.to_string())?;
                        if path.starts_with(&workspace.projects[parent_index].root) {
                            let child = workspace.register_directory(path);
                            workspace.attach_directory(parent_index, child);
                        }
                    }
                }
            }
            let expanded = project
                .get("children_expanded")
                .map(|value| value.as_bool().ok_or("children_expanded must be a boolean"))
                .transpose()?
                .unwrap_or(true);
            if !expanded {
                workspace.toggle_folder(parent_index);
            }
            workspace.switch_project(index);
            if active.as_ref() == Some(&target) || active.as_ref() == Some(&root) {
                selected = index;
            }
            continue;
        }
        let available = if target.is_dir() { &target } else { &root };
        if !available.is_dir() {
            continue;
        }
        let available = fs::canonicalize(available).map_err(|error| error.to_string())?;
        let index = workspace.register_directory(available.clone());
        if active.as_ref() == Some(&target) || active.as_ref() == Some(&root) {
            selected = index;
        }
        fallbacks.entry(available).or_insert(root);
    }
    workspace.switch_project(selected);
    Ok((workspace, fallbacks))
}

fn directory(value: &Value) -> Result<PathBuf, String> {
    let path = PathBuf::from(value.as_str().ok_or("Project directory must be a string")?);
    if !path.is_absolute() {
        return Err("Project directory must be an absolute path".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparing_an_autosave_does_not_wait_for_the_disk_writer() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::new(temporary.path().join("workspace.json"));
        let state = store.load().unwrap();
        let writer_guard = store.storage.lock().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let capturing = store.clone();
        let thread = std::thread::spawn(move || sender.send(capturing.prepare(&state)));
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(writer_guard);
        thread.join().unwrap().unwrap();
        result
            .expect("UI capture must not acquire the writer's IO lock")
            .save()
            .unwrap();
    }
}
