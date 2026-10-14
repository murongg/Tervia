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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScanLimit {
    Time(u64),
    Directories(usize),
    Repositories(usize),
    Depth(usize),
    Unreadable,
}

#[derive(Clone, Debug)]
pub struct Scan {
    pub discovery: Discovery,
    pub notices: BTreeSet<ScanLimit>,
}

struct Limits {
    depth: usize,
    directories: usize,
    repositories: usize,
    time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            depth: 3,
            directories: 8192,
            repositories: 128,
            time: Duration::from_secs(3),
        }
    }
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
        Ok(Self::scan(root)?.discovery)
    }

    pub fn scan(root: &Path) -> Result<Scan, String> {
        scan_with(root, Limits::default())
    }
}

fn scan_with(root: &Path, limits: Limits) -> Result<Scan, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("Select a project directory".into());
    }
    let mut pending = VecDeque::from([(root.clone(), 0usize)]);
    let mut repositories = BTreeSet::new();
    let mut identities = BTreeMap::new();
    let mut visited = 0;
    let mut notices = BTreeSet::new();
    let started = Instant::now();
    while let Some((directory, depth)) = pending.pop_front() {
        // Depth and read notices still allow shallow siblings to be scanned.
        if visited >= limits.directories
            || repositories.len() >= limits.repositories
            || started.elapsed() >= limits.time
        {
            if visited >= limits.directories {
                notices.insert(ScanLimit::Directories(limits.directories));
            }
            if repositories.len() >= limits.repositories {
                notices.insert(ScanLimit::Repositories(limits.repositories));
            }
            if started.elapsed() >= limits.time {
                notices.insert(ScanLimit::Time(limits.time.as_secs()));
            }
            break;
        }
        visited += 1;
        // A .git file is normal for linked worktrees and submodules.
        if directory.join(".git").exists()
            && let Ok(repository) = Repository::open(&directory)
            && repository.root.starts_with(&root)
        {
            identities.insert(repository.root.clone(), repository.common_directory()?);
            repositories.insert(repository.root);
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory == root => return Err(error.to_string()),
            Err(_) => {
                notices.insert(ScanLimit::Unreadable);
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
                if depth >= limits.depth {
                    notices.insert(ScanLimit::Depth(limits.depth));
                } else {
                    children.push(entry.path());
                }
            }
        }
        children.sort();
        pending.extend(children.into_iter().map(|path| (path, depth + 1)));
    }
    Ok(Scan {
        discovery: Discovery {
            root,
            repositories: repositories.into_iter().collect(),
            limited: !notices.is_empty(),
            identities,
        },
        notices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_scanning_stops_after_three_child_directory_levels() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("mock-one/mock-two/mock-three/mock-four"))
            .unwrap();
        let scan = Repository::scan(root.path()).unwrap();
        assert_eq!(scan.notices, BTreeSet::from([ScanLimit::Depth(3)]));
        assert!(scan.discovery.limited);
    }

    #[test]
    fn scan_reports_the_depth_budget() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("mock-child/mock-deep")).unwrap();
        let scan = scan_with(
            root.path(),
            Limits {
                depth: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        assert!(scan.notices.contains(&ScanLimit::Depth(1)));
        assert!(scan.discovery.limited);
    }

    #[test]
    fn scan_reports_the_directory_budget() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("mock-child")).unwrap();
        let scan = scan_with(
            root.path(),
            Limits {
                directories: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(scan.notices, BTreeSet::from([ScanLimit::Directories(1)]));
        assert!(scan.discovery.limited);
    }

    #[test]
    fn scan_reports_the_time_budget() {
        let root = tempfile::tempdir().unwrap();
        let scan = scan_with(
            root.path(),
            Limits {
                time: Duration::ZERO,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(scan.notices, BTreeSet::from([ScanLimit::Time(0)]));
    }

    #[test]
    fn scan_reports_the_repository_budget() {
        let root = tempfile::tempdir().unwrap();
        Repository::initialize(root.path()).unwrap();
        std::fs::create_dir(root.path().join("mock-child")).unwrap();
        let scan = scan_with(
            root.path(),
            Limits {
                repositories: 1,
                time: Duration::from_secs(60),
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(scan.discovery.repositories.len(), 1);
        assert_eq!(scan.notices, BTreeSet::from([ScanLimit::Repositories(1)]));
    }
}
