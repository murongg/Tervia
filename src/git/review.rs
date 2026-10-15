use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReviewKey {
    pub path: PathBuf,
    pub staged: bool,
}

impl ReviewKey {
    pub fn new(path: PathBuf, side: Side) -> Self {
        Self {
            path,
            staged: side == Side::Index,
        }
    }
}

#[derive(Default)]
pub struct Reviews(BTreeMap<ReviewKey, [u8; 32]>, usize);

impl Reviews {
    pub fn contains(&self, key: &ReviewKey) -> bool {
        self.0.contains_key(key)
    }
    pub fn keys(&self) -> impl Iterator<Item = &ReviewKey> {
        self.0.keys()
    }
    pub fn revision(&self) -> usize {
        self.1
    }
    pub fn reconcile_at(&mut self, revision: usize, current: &BTreeMap<ReviewKey, [u8; 32]>) {
        if self.1 == revision {
            self.reconcile(current);
        }
    }
    pub fn toggle(&mut self, key: ReviewKey, token: [u8; 32]) {
        self.1 = self.1.wrapping_add(1);
        if self.0.get(&key) == Some(&token) {
            self.0.remove(&key);
        } else {
            self.0.insert(key, token);
        }
    }
    pub fn reconcile(&mut self, current: &BTreeMap<ReviewKey, [u8; 32]>) {
        let count = self.0.len();
        self.0.retain(|key, token| current.get(key) == Some(token));
        if self.0.len() != count {
            self.1 = self.1.wrapping_add(1);
        }
    }
}

impl Repository {
    pub fn checkpoint(&self) -> Result<String, Error> {
        let temporary = tempfile::tempdir().map_err(|error| Error::from(error.to_string()))?;
        let index = temporary.path().join("index");
        // Start from the real index so tracked ignored files and sparse entries retain their identity.
        match fs::copy(self.git_dir.join("index"), &index) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                command::run_index(&self.root, &index, &arguments(&["read-tree", "--empty"]))?;
            }
            Err(error) => return Err(error.to_string().into()),
        }
        // Capture working content, including untracked files, without staging into the user's index.
        command::run_index(&self.root, &index, &arguments(&["add", "-A", "--", "."]))?;
        let tree = text(command::run_index(
            &self.root,
            &index,
            &arguments(&["write-tree"]),
        )?);
        Ok(tree.trim().to_owned())
    }

    pub fn compare_checkpoint(&self, checkpoint: &str) -> Result<String, Error> {
        if !matches!(checkpoint.len(), 40 | 64)
            || !checkpoint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Invalid checkpoint object".into());
        }
        if text(self.run(&["cat-file", "-t", checkpoint])?).trim() != "tree" {
            return Err("Checkpoint is not a tree".into());
        }
        let current = self.checkpoint()?;
        self.run(&[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--full-index",
            "--no-renames",
            checkpoint,
            &current,
            "--",
        ])
        .map(text)
    }

    pub fn review_tokens(
        &self,
        files: &[(FileStatus, Side)],
    ) -> Result<BTreeMap<ReviewKey, [u8; 32]>, Error> {
        if files.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut args = arguments(&["ls-files", "--stage", "-z", "--"]);
        for (file, _) in files {
            if file.path.is_absolute()
                || file
                    .path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err("Review path is outside the repository".into());
            }
            args.push(file.path.as_os_str().to_owned());
        }
        let mut entries: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
        for record in command::run(&self.root, &args)?.split(|byte| *byte == 0) {
            if let Some(tab) = record.iter().position(|byte| *byte == b'\t') {
                entries
                    .entry(status::path(&record[tab + 1..])?)
                    .or_default()
                    .extend_from_slice(&record[..tab]);
            }
        }
        let head = if self.status()?.unborn {
            Vec::new()
        } else {
            self.run(&["rev-parse", "--verify", "HEAD"])?
        };
        let mut result = BTreeMap::new();
        for (file, side) in files {
            let mut hash = Sha256::new();
            hash.update(&head);
            hash.update(file.path.as_os_str().as_encoded_bytes());
            if let Some(original) = &file.original {
                hash.update(original.as_os_str().as_encoded_bytes());
            }
            if let Some(index) = entries.get(&file.path) {
                hash.update(index);
            }
            if *side == Side::Worktree {
                let path = self.root.join(&file.path);
                match fs::symlink_metadata(&path) {
                    Ok(metadata) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt as _;
                            hash.update(metadata.permissions().mode().to_le_bytes());
                        }
                        if metadata.file_type().is_symlink() {
                            hash.update(
                                fs::read_link(&path)
                                    .map_err(|error| Error::from(error.to_string()))?
                                    .as_os_str()
                                    .as_encoded_bytes(),
                            );
                        } else {
                            let canonical = path
                                .canonicalize()
                                .map_err(|error| Error::from(error.to_string()))?;
                            if !canonical.starts_with(&self.root) {
                                return Err("Review file points outside the repository".into());
                            }
                            if metadata.len() > 64 * 1024 * 1024 {
                                return Err("Review file exceeds 64 MiB".into());
                            }
                            let mut input = fs::File::open(canonical)
                                .map_err(|error| Error::from(error.to_string()))?;
                            let mut bytes = [0; 8192];
                            loop {
                                let count = input
                                    .read(&mut bytes)
                                    .map_err(|error| Error::from(error.to_string()))?;
                                if count == 0 {
                                    break;
                                }
                                hash.update(&bytes[..count]);
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        hash.update(b"missing")
                    }
                    Err(error) => return Err(error.to_string().into()),
                }
            }
            result.insert(
                ReviewKey::new(file.path.clone(), *side),
                hash.finalize().into(),
            );
        }
        Ok(result)
    }
}
