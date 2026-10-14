use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
    Symlink,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// Resolve a directory within the opened root, including after filesystem changes.
pub fn directory(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("Path is outside the project".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let absolute = root
        .join(path)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !absolute.starts_with(&root) {
        return Err("Directory points outside the project".into());
    }
    if !absolute.is_dir() {
        return Err("Path is not a directory".into());
    }
    Ok(absolute)
}

/// Read one directory only. Expensive dependency trees remain unopened until requested.
pub fn list(root: &Path, path: &Path) -> Result<Vec<Entry>, String> {
    let absolute = directory(root, path)?;
    let mut entries = Vec::new();
    for child in fs::read_dir(absolute).map_err(|error| error.to_string())? {
        let child = child.map_err(|error| error.to_string())?;
        if child.file_name() == ".git" {
            continue;
        }
        let kind = child.file_type().map_err(|error| error.to_string())?;
        entries.push(Entry {
            path: path.join(child.file_name()),
            // Never expand directory symlinks: this avoids cycles and escaping the root.
            kind: if kind.is_symlink() {
                EntryKind::Symlink
            } else if kind.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
        });
    }
    entries.sort_by_cached_key(|entry| {
        (
            entry.kind != EntryKind::Directory,
            entry
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
            entry.path.clone(),
        )
    });
    Ok(entries)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Entry {
        entry: Entry,
        depth: usize,
    },
    Loading {
        path: PathBuf,
        depth: usize,
    },
    Empty {
        path: PathBuf,
        depth: usize,
    },
    Error {
        path: PathBuf,
        depth: usize,
        message: String,
    },
}

#[derive(Default)]
struct Directory {
    entries: Option<Vec<Entry>>,
    pending: Option<u64>,
    error: Option<String>,
    stale: bool,
}

/// Explorer state is separate from opened documents, so refreshing never replaces drafts.
pub struct Tree {
    directories: BTreeMap<PathBuf, Directory>,
    expanded: BTreeSet<PathBuf>,
    generation: u64,
    rows: Arc<Vec<Row>>,
}

impl Default for Tree {
    fn default() -> Self {
        let mut tree = Self {
            directories: BTreeMap::from([(PathBuf::new(), Directory::default())]),
            expanded: BTreeSet::from([PathBuf::new()]),
            generation: 0,
            rows: Arc::default(),
        };
        tree.rebuild();
        tree
    }
}

impl Tree {
    pub fn rows(&self) -> Arc<Vec<Row>> {
        self.rows.clone()
    }
    pub fn loading(&self) -> bool {
        self.directories
            .values()
            .any(|directory| directory.pending.is_some())
    }
    pub fn directories(&self) -> Vec<PathBuf> {
        self.directories.keys().cloned().collect()
    }
    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }
    pub fn expanded(&self) -> Vec<PathBuf> {
        self.expanded.iter().cloned().collect()
    }

    pub fn entries(&self, path: &Path) -> Option<&[Entry]> {
        self.directories.get(path)?.entries.as_deref()
    }

    pub fn toggle(&mut self, path: &Path) -> bool {
        if !path.as_os_str().is_empty()
            && !self.directories.values().any(|directory| {
                directory.entries.as_ref().is_some_and(|entries| {
                    entries
                        .iter()
                        .any(|entry| entry.path == path && entry.kind == EntryKind::Directory)
                })
            })
        {
            return false;
        }
        if !self.expanded.remove(path) {
            self.expanded.insert(path.into());
            self.directories.entry(path.into()).or_default();
        }
        self.rebuild();
        self.is_expanded(path)
    }

    pub fn begin(&mut self, path: PathBuf, refresh: bool) -> Option<u64> {
        let directory = self.directories.get_mut(&path)?;
        if !refresh
            && (directory.pending.is_some() || (directory.entries.is_some() && !directory.stale))
        {
            return None;
        }
        self.generation += 1;
        directory.pending = Some(self.generation);
        directory.error = None;
        directory.stale |= refresh;
        self.rebuild();
        Some(self.generation)
    }

    pub fn refresh(&mut self) -> Vec<(PathBuf, u64)> {
        // Closed folders must lose freshness too; reread them lazily when reopened.
        // Keep old entries for display and subtree pruning while new reads are pending.
        for directory in self.directories.values_mut() {
            directory.stale = true;
            directory.pending = None;
        }
        let mut paths = self.expanded.clone();
        paths.insert(PathBuf::new());
        let mut requests = Vec::new();
        for path in paths {
            if let Some(directory) = self.directories.get_mut(&path) {
                self.generation += 1;
                directory.pending = Some(self.generation);
                directory.error = None;
                requests.push((path, self.generation));
            }
        }
        self.rebuild();
        requests
    }

    pub fn finish(&mut self, path: &Path, token: u64, result: Result<Vec<Entry>, String>) -> bool {
        let Some(directory) = self.directories.get_mut(path) else {
            return false;
        };
        if directory.pending != Some(token) {
            return false;
        }
        directory.pending = None;
        match result {
            Ok(entries) => {
                let removed: Vec<_> = directory
                    .entries
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter(|old| {
                        old.kind == EntryKind::Directory
                            && !entries
                                .iter()
                                .any(|new| new.path == old.path && new.kind == EntryKind::Directory)
                    })
                    .map(|entry| entry.path.clone())
                    .collect();
                directory.entries = Some(entries);
                directory.error = None;
                directory.stale = false;
                // Removing the cached subtree also invalidates its in-flight reads.
                for removed in removed {
                    self.directories.retain(|key, _| !key.starts_with(&removed));
                    self.expanded.retain(|key| !key.starts_with(&removed));
                }
            }
            Err(error) => {
                directory.error = Some(error);
                directory.stale = true;
            }
        }
        self.rebuild();
        true
    }

    fn rebuild(&mut self) {
        let mut rows = Vec::new();
        let mut stack = Vec::new();
        if self.is_expanded(Path::new("")) {
            self.push_directory(Path::new(""), 0, &mut stack);
        }
        while let Some(row) = stack.pop() {
            if let Row::Entry { entry, depth } = &row
                && entry.kind == EntryKind::Directory
                && self.is_expanded(&entry.path)
            {
                self.push_directory(&entry.path, depth + 1, &mut stack);
            }
            rows.push(row);
        }
        self.rows = Arc::new(rows);
    }

    fn push_directory(&self, path: &Path, depth: usize, stack: &mut Vec<Row>) {
        let directory = self.directories.get(path);
        if let Some(error) = directory.and_then(|directory| directory.error.as_ref()) {
            stack.push(Row::Error {
                path: path.into(),
                depth,
                message: error.clone(),
            });
        } else if directory
            .is_none_or(|directory| directory.pending.is_some() || directory.entries.is_none())
        {
            stack.push(Row::Loading {
                path: path.into(),
                depth,
            });
        } else if directory
            .is_some_and(|directory| directory.entries.as_ref().is_some_and(Vec::is_empty))
        {
            stack.push(Row::Empty {
                path: path.into(),
                depth,
            });
        }
        if let Some(entries) = directory.and_then(|directory| directory.entries.as_ref()) {
            stack.extend(
                entries
                    .iter()
                    .rev()
                    .cloned()
                    .map(|entry| Row::Entry { entry, depth }),
            );
        }
    }
}
