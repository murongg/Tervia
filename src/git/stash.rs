use super::{Repository, status, text};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

struct Lock {
    path: PathBuf,
    file: Option<File>,
    owned: bool,
}
impl Lock {
    fn acquire(path: PathBuf, permissions: fs::Permissions) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("Stash is busy or its lock cannot be created: {error}"))?;
        let lock = Self {
            path,
            file: Some(file),
            owned: true,
        };
        lock.file
            .as_ref()
            .unwrap()
            .set_permissions(permissions)
            .map_err(|error| error.to_string())?;
        Ok(lock)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let file = self.file.as_mut().unwrap();
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())
    }
    fn commit(&mut self, destination: &Path) -> Result<(), String> {
        self.file.take();
        fs::rename(&self.path, destination).map_err(|error| error.to_string())?;
        self.owned = false;
        Ok(())
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        self.file.take();
        if self.owned {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Repository {
    fn stash_directory(&self) -> Result<PathBuf, String> {
        let format = self
            .run(&["config", "--get", "extensions.refStorage"])
            .map(text)
            .unwrap_or_else(|_| "files".into());
        if format != "files" || self.run(&["symbolic-ref", "-q", "refs/stash"]).is_ok() {
            return Err("Atomic stash removal is unavailable for this ref storage. The stash was preserved; use the terminal to remove it.".into());
        }
        let bytes = self.run(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        status::path(bytes.strip_suffix(b"\n").unwrap_or(&bytes))
    }

    pub(super) fn drop_stash_locked(
        &self,
        reference: &str,
        expected: &str,
        mut after_lock: impl FnMut() -> Result<(), String>,
    ) -> Result<String, String> {
        let index: usize = reference
            .strip_prefix("stash@{")
            .and_then(|value| value.strip_suffix('}'))
            .and_then(|value| value.parse().ok())
            .ok_or("Invalid stash entry")?;
        if !matches!(expected.len(), 40 | 64)
            || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Invalid stash object ID".into());
        }
        let common = self.stash_directory()?;
        let log_path = common.join("logs/refs/stash");
        let ref_path = common.join("refs/stash");
        let log_permissions = fs::metadata(&log_path)
            .map_err(|error| error.to_string())?
            .permissions();
        let ref_permissions = fs::metadata(&ref_path)
            .map(|metadata| metadata.permissions())
            .unwrap_or_else(|_| log_permissions.clone());
        // Git's files backend protects reflog expiry with refs/stash.lock.
        // Its CLI has no expected-OID deletion option: keep this adapter scoped
        // to stash entries and refuse other stores instead of deleting by a stale number.
        let mut reference_lock = Lock::acquire(common.join("refs/stash.lock"), ref_permissions)?;
        after_lock()?;
        let original = fs::read(&log_path).map_err(|error| error.to_string())?;
        let mut entries = Vec::new();
        for line in original.split_inclusive(|byte| *byte == b'\n') {
            let mut fields = line.splitn(3, |byte| *byte == b' ');
            let old = fields.next().ok_or("Malformed stash log")?;
            let new = fields.next().ok_or("Malformed stash log")?;
            let rest = fields.next().ok_or("Malformed stash log")?;
            if old.len() != expected.len()
                || new.len() != expected.len()
                || !old.iter().chain(new).all(u8::is_ascii_hexdigit)
            {
                return Err("Malformed stash log; no entries changed".into());
            }
            entries.push((new.to_vec(), rest.to_vec()));
        }
        let selected = entries
            .len()
            .checked_sub(index + 1)
            .ok_or("Stash list changed; refresh before deleting")?;
        if entries[selected].0 != expected.as_bytes() {
            return Err("Stash list changed; refresh before deleting".into());
        }
        let head = text(self.run(&["rev-parse", "--verify", "refs/stash"])?);
        if entries
            .last()
            .is_none_or(|entry| entry.0 != head.as_bytes())
        {
            return Err("Stash ref and log differ; no entries changed".into());
        }
        entries.remove(selected);
        let mut rewritten = Vec::new();
        let mut previous = vec![b'0'; expected.len()];
        for (new, rest) in &entries {
            rewritten.extend_from_slice(&previous);
            rewritten.push(b' ');
            rewritten.extend_from_slice(new);
            rewritten.push(b' ');
            rewritten.extend_from_slice(rest);
            previous = new.clone();
        }
        let mut log_lock = Lock::acquire(common.join("logs/refs/stash.lock"), log_permissions)?;
        log_lock.write(&rewritten)?;
        if !entries.is_empty() {
            reference_lock.write(&[previous.as_slice(), b"\n"].concat())?;
        }
        log_lock.commit(&log_path)?;
        if entries.is_empty() {
            after_lock()?;
            // Keep the reflog generation protected, even if another writer
            // would store the same OID. OID-only CAS cannot detect that ABA case.
            let packed_path = common.join("packed-refs");
            if let Ok(metadata) = fs::metadata(&packed_path) {
                let mut packed_lock =
                    Lock::acquire(common.join("packed-refs.lock"), metadata.permissions())?;
                let packed = fs::read(&packed_path).map_err(|error| error.to_string())?;
                let mut kept = Vec::new();
                let mut removed = false;
                let mut skip_peeled = false;
                for line in packed.split_inclusive(|byte| *byte == b'\n') {
                    if skip_peeled && line.starts_with(b"^") {
                        skip_peeled = false;
                        continue;
                    }
                    skip_peeled = false;
                    let value = line.strip_suffix(b"\n").unwrap_or(line);
                    if value.splitn(2, |byte| *byte == b' ').nth(1)
                        == Some(b"refs/stash".as_slice())
                    {
                        removed = true;
                        skip_peeled = true;
                    } else {
                        kept.extend_from_slice(line);
                    }
                }
                if removed {
                    packed_lock.write(&kept)?;
                    packed_lock.commit(&packed_path)?;
                }
            }
            match fs::remove_file(&ref_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            fs::remove_file(&log_path).map_err(|error| error.to_string())?;
        } else {
            reference_lock.commit(&ref_path)?;
        }
        Ok(format!("Dropped {reference} ({expected})"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::fs;

    #[test]
    fn committed_locks_never_remove_the_next_writers_lock() {
        let root =
            std::env::temp_dir().join(format!("tervia-lock-lease-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let target = root.join("ref");
        fs::write(&target, "synthetic original\n").unwrap();
        let path = root.join("ref.lock");
        let mut lock =
            super::Lock::acquire(path.clone(), fs::metadata(&target).unwrap().permissions())
                .unwrap();
        lock.write(b"synthetic updated\n").unwrap();
        lock.commit(&target).unwrap();
        fs::write(&path, "synthetic next writer\n").unwrap();
        drop(lock);
        assert!(path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_stash_cleanup_cannot_delete_a_new_entry_with_the_same_oid() {
        let root =
            std::env::temp_dir().join(format!("tervia-stash-aba-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let repo = Repository::initialize(&root).unwrap();
        for (key, value) in [
            ("user.name", "Synthetic Developer"),
            ("user.email", "developer@example.test"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", ".synthetic-hooks"),
        ] {
            repo.run(&["config", key, value]).unwrap();
        }
        fs::write(root.join("mock.txt"), "synthetic original\n").unwrap();
        repo.execute(Operation::StageAll).unwrap();
        repo.execute(Operation::Commit {
            message: "Synthetic initial".into(),
            amend: false,
        })
        .unwrap();
        fs::write(root.join("mock.txt"), "synthetic stash\n").unwrap();
        repo.execute(Operation::StashPush("Original".into()))
            .unwrap();
        let selected = repo.snapshot(10).unwrap().stashes[0].clone();
        repo.run(&["pack-refs", "--all", "--prune"]).unwrap();
        let mut calls = 0;
        repo.drop_stash_locked(&selected.reference, &selected.hash, || {
            calls += 1;
            if calls == 2 {
                assert!(
                    repo.run(&[
                        "stash",
                        "store",
                        "-m",
                        "Synthetic same-OID race",
                        &selected.hash
                    ])
                    .is_err()
                );
            }
            Ok(())
        })
        .unwrap();
        assert!(repo.snapshot(10).unwrap().stashes.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_external_stash_writer_cannot_shift_the_entry_under_deletion() {
        let root =
            std::env::temp_dir().join(format!("tervia-stash-lock-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let repo = Repository::initialize(&root).unwrap();
        for (key, value) in [
            ("user.name", "Synthetic Developer"),
            ("user.email", "developer@example.test"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", ".synthetic-hooks"),
        ] {
            repo.run(&["config", key, value]).unwrap();
        }
        fs::write(root.join("mock.txt"), "synthetic original\n").unwrap();
        repo.execute(Operation::StageAll).unwrap();
        repo.execute(Operation::Commit {
            message: "Synthetic initial".into(),
            amend: false,
        })
        .unwrap();
        fs::write(root.join("mock.txt"), "synthetic first stash\n").unwrap();
        repo.execute(Operation::StashPush("First".into())).unwrap();
        let first = repo.snapshot(10).unwrap().stashes[0].hash.clone();
        fs::write(root.join("mock.txt"), "synthetic second stash\n").unwrap();
        repo.execute(Operation::StashPush("Second".into())).unwrap();
        let selected = repo.snapshot(10).unwrap().stashes[0].clone();
        repo.drop_stash_locked(&selected.reference, &selected.hash, || {
            assert!(
                repo.run(&["stash", "store", "-m", "Synthetic race", &first])
                    .is_err()
            );
            Ok(())
        })
        .unwrap();
        let remaining = repo.snapshot(10).unwrap().stashes;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].hash, first);
        fs::remove_dir_all(root).unwrap();
    }
}
