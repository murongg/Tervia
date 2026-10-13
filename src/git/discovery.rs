use super::Repository;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Discovery {
    pub root: PathBuf,
    pub repositories: Vec<PathBuf>,
    pub limited: bool,
    pub identities: BTreeMap<PathBuf, PathBuf>,
}

impl Discovery {
    pub fn projects(&self) -> Vec<PathBuf> {
        let mut groups: BTreeMap<PathBuf, Vec<&PathBuf>> = BTreeMap::new();
        for root in &self.repositories {
            groups
                .entry(self.identities.get(root).unwrap_or(root).clone())
                .or_default()
                .push(root);
        }
        groups
            .into_iter()
            .filter_map(|(identity, roots)| {
                roots
                    .iter()
                    .find(|root| identity.starts_with(root))
                    .or_else(|| roots.first())
                    .map(|root| (*root).clone())
            })
            .collect()
    }
}

impl Repository {
    pub fn discover(root: &Path) -> Result<Discovery, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        if !root.is_dir() {
            return Err("Select a project directory".into());
        }
        let mut pending = VecDeque::from([(root.clone(), 0usize)]);
        let mut repositories = BTreeSet::new();
        let mut identities = BTreeMap::new();
        let mut visited = 0;
        let mut limited = false;
        let started = Instant::now();
        while let Some((directory, depth)) = pending.pop_front() {
            if visited >= 8192
                || repositories.len() >= 128
                || started.elapsed() > Duration::from_secs(3)
            {
                limited = true;
                break;
            }
            visited += 1;
            // A .git file is normal for linked worktrees and submodules.
            if directory.join(".git").exists()
                && let Ok(repository) = Self::open(&directory)
                && repository.root.starts_with(&root)
            {
                identities.insert(repository.root.clone(), repository.common_directory()?);
                repositories.insert(repository.root);
            }
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if directory == root => return Err(error.to_string()),
                Err(_) => {
                    limited = true;
                    continue;
                }
            };
            let mut children = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name();
                if matches!(
                    name.to_str(),
                    Some(".git" | "node_modules" | "target" | "build" | ".venv" | ".cache")
                ) {
                    continue;
                }
                // Child symlinks can escape the chosen workspace or make scanning cyclic.
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    if depth >= 8 {
                        limited = true;
                    } else {
                        children.push(entry.path());
                    }
                }
            }
            children.sort();
            pending.extend(children.into_iter().map(|path| (path, depth + 1)));
        }
        Ok(Discovery {
            root,
            repositories: repositories.into_iter().collect(),
            limited,
            identities,
        })
    }
}
