use super::{Repository, Snapshot};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
pub struct SnapshotCache {
    saved: Option<(Fingerprint, Snapshot)>,
}

#[derive(Clone, PartialEq, Eq)]
struct Fingerprint {
    root: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
    limit: usize,
    config: Vec<u8>,
    bytes: usize,
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
}

impl SnapshotCache {
    pub fn clear(&mut self) {
        self.saved = None;
    }

    pub fn refresh(
        &mut self,
        repository: &Repository,
        limit: usize,
    ) -> Result<Snapshot, super::Error> {
        let status = repository.status()?;
        let before = Fingerprint::read(repository, limit).ok();
        if let Some((key, snapshot)) = &self.saved
            && before.as_ref() == Some(key)
        {
            let mut snapshot = snapshot.clone();
            snapshot.status = status;
            for tree in &mut snapshot.worktrees {
                tree.missing = !tree.path.is_dir();
                if let Ok(path) = tree.path.canonicalize() {
                    tree.path = path;
                }
            }
            return Ok(snapshot);
        }
        let snapshot = repository.snapshot_with_status(limit, status)?;
        let after = Fingerprint::read(repository, limit).ok();
        // A ref/config change during capture cannot become the key for older metadata.
        // Unknown or oversized metadata falls back to ordinary full reads on every poll.
        self.saved = before
            .filter(|before| Some(before) == after.as_ref())
            .map(|key| (key, snapshot.clone()));
        Ok(snapshot)
    }
}

impl Fingerprint {
    fn read(repository: &Repository, limit: usize) -> Result<Self, super::Error> {
        let mut fingerprint = Self {
            root: repository.root.clone(),
            git_dir: repository.git_dir.clone(),
            common_dir: repository.common_dir.clone(),
            limit,
            // Git resolves system/global configs, includeIf and URL rewrites itself.
            config: repository.run(&["config", "--null", "--list", "--includes"])?,
            files: Vec::new(),
            bytes: 0,
        };
        let mut roots = vec![&repository.git_dir, &repository.common_dir];
        roots.sort();
        roots.dedup();
        for root in roots {
            for name in [
                "HEAD",
                "packed-refs",
                "shallow",
                "info/grafts",
                "logs/refs/stash",
            ] {
                fingerprint.file(&root.join(name))?;
            }
            fingerprint.directory(&root.join("refs"), 0)?;
            fingerprint.directory(&root.join("reftable"), 0)?;
        }
        let worktrees = repository.common_dir.join("worktrees");
        match fs::read_dir(&worktrees) {
            Ok(entries) => {
                let mut entries = entries.take(100_001).collect::<io::Result<Vec<_>>>()?;
                if entries.len() > 100_000 {
                    return Err(
                        io::Error::other("Git metadata exceeds refresh cache bounds").into(),
                    );
                }
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    fingerprint.file(&entry.path().join("gitdir"))?;
                    if let Some((_, Some(bytes))) = fingerprint.files.last() {
                        let target =
                            super::status::path(bytes.strip_suffix(b"\n").unwrap_or(bytes))?;
                        let target = entry.path().join(target);
                        // Prunable diagnostics also depend on the checkout's marker,
                        // which can disappear without changing its administration files.
                        fingerprint.file(&target.canonicalize().unwrap_or(target))?;
                    }
                    for name in ["HEAD", "commondir", "locked", "config.worktree"] {
                        fingerprint.file(&entry.path().join(name))?;
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(fingerprint)
    }

    fn file(&mut self, path: &Path) -> io::Result<()> {
        if self.files.len() >= 100_000 {
            return Err(io::Error::other(
                "Git metadata exceeds refresh cache bounds",
            ));
        }
        let bytes = match fs::File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                if self.bytes + bytes.len() > 16 * 1024 * 1024 {
                    return Err(io::Error::other(
                        "Git metadata exceeds refresh cache bounds",
                    ));
                }
                self.bytes += bytes.len();
                Some(bytes)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        self.files.push((path.into(), bytes));
        Ok(())
    }

    fn directory(&mut self, path: &Path, depth: usize) -> io::Result<()> {
        if depth > 32 {
            return Err(io::Error::other("Git refs exceed refresh cache depth"));
        }
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut entries = entries.take(100_001).collect::<io::Result<Vec<_>>>()?;
        if entries.len() > 100_000 {
            return Err(io::Error::other(
                "Git metadata exceeds refresh cache bounds",
            ));
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if entry.file_type()?.is_dir() {
                self.directory(&entry.path(), depth + 1)?;
            } else {
                self.file(&entry.path())?;
            }
        }
        Ok(())
    }
}
