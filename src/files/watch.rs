use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _,
    event::{ModifyKind, RenameMode},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub struct Changes {
    pub paths: BTreeSet<PathBuf>,
    pub removed: BTreeSet<PathBuf>,
    pub rescan: bool,
    pub error: Option<String>,
}

impl Changes {
    pub fn record(&mut self, root: &Path, event: notify::Result<Event>) {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                self.error = Some(error.to_string());
                self.rescan = true;
                return;
            }
        };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        self.rescan |= event.need_rescan();
        for (index, path) in event.paths.into_iter().enumerate() {
            if let Ok(path) = path.strip_prefix(root)
                && !path.components().any(|part| part.as_os_str() == ".git")
            {
                if matches!(
                    event.kind,
                    EventKind::Remove(_)
                        | EventKind::Modify(ModifyKind::Name(RenameMode::From | RenameMode::Any))
                ) || (index == 0
                    && matches!(
                        event.kind,
                        EventKind::Modify(ModifyKind::Name(RenameMode::Both))
                    ))
                {
                    self.removed.insert(path.into());
                }
                self.paths.insert(path.into());
            }
        }
        // Coalesce checkout/build bursts without letting the event queue grow unbounded.
        if self.paths.len() + self.removed.len() > 2048 {
            self.paths.clear();
            self.removed.clear();
            self.rescan = true;
        }
    }
}

#[derive(Clone)]
pub struct Watch {
    root: PathBuf,
    watcher: Arc<Mutex<RecommendedWatcher>>,
    changes: Arc<Mutex<Changes>>,
}

impl Watch {
    /// Initialization and registration run on the background executor, never during rendering.
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let changes = Arc::new(Mutex::new(Changes::default()));
        let output = changes.clone();
        let watched_root = root.clone();
        let mut watcher = notify::recommended_watcher(move |event| {
            output.lock().unwrap().record(&watched_root, event);
        })
        .map_err(|error| error.to_string())?;
        watcher
            .watch(&root, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
        // Watching the parent also detects deletion and recreation of the project root.
        if let Some(parent) = root.parent() {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }
        Ok(Self {
            root,
            watcher: Arc::new(Mutex::new(watcher)),
            changes,
        })
    }

    pub fn watch_directory(&self, path: &Path) -> Result<(), String> {
        if path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("Path is outside the project".into());
        }
        let directory = self
            .root
            .join(path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !directory.starts_with(&self.root) {
            return Err("Directory points outside the project".into());
        }
        let mut watcher = self.watcher.lock().unwrap();
        let _ = watcher.unwatch(&directory);
        watcher
            .watch(&directory, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())
    }

    pub fn drain(&self) -> Changes {
        std::mem::take(&mut *self.changes.lock().unwrap())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    Unchanged,
    Replace(String),
    Conflict,
    Unavailable(String),
}

/// Compare with the current draft at completion, including edits typed while reading disk.
pub fn reconcile(saved: &str, draft: &str, disk: Result<String, String>) -> Update {
    match disk {
        Ok(contents) if contents == saved => Update::Unchanged,
        Ok(contents) if saved == draft || contents == draft => Update::Replace(contents),
        Ok(_) => Update::Conflict,
        Err(error) => Update::Unavailable(error),
    }
}
