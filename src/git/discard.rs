use super::*;
use std::{collections::BTreeMap, fs, path::Component};

impl Repository {
    pub(super) fn discard_files(&self, files: &[FileStatus]) -> Result<String, Error> {
        if files.is_empty() {
            return Ok(String::new());
        }
        let status = self.status()?;
        let current: BTreeMap<_, _> = status.files.iter().map(|file| (&file.path, file)).collect();
        let mut tracked = Vec::new();
        let mut untracked = Vec::new();
        // Validate the entire captured selection before making any change. A file
        // staged while confirmation was open must never be treated as untracked.
        for file in files {
            if current.get(&file.path).copied() != Some(file) {
                return Err("File status changed. Refresh changes and try again.".into());
            }
            if file.conflicted {
                return Err("Resolve conflicts before discarding changes.".into());
            }
            if file.worktree == '.' {
                continue;
            }
            let path = discard_path(&self.root, &file.path)?;
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() => {
                    return Err("Discarding directories or submodule changes is not supported. Inspect them in the terminal.".into());
                }
                Ok(_) => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && file.worktree == 'D' => {}
                Err(error) => return Err(error.into()),
            }
            if file.worktree == '?' {
                untracked.push(path);
            } else {
                tracked.extend_from_slice(file.path.as_os_str().as_encoded_bytes());
                tracked.push(0);
            }
        }
        if !tracked.is_empty() {
            // The index is the restore source, preserving partially staged edits
            // and staged renames, including repositories without an initial commit.
            command::run_input(
                &self.root,
                &arguments(&[
                    "restore",
                    "--worktree",
                    "--pathspec-from-file=-",
                    "--pathspec-file-nul",
                ]),
                Some(tracked),
            )?;
        }
        // Trash only explicitly confirmed files; never recursively clean a
        // directory that may now contain ignored files or another repository.
        if !untracked.is_empty() {
            let trash = trash::TrashContext::new();
            #[cfg(target_os = "macos")]
            let trash = {
                use trash::macos::{DeleteMethod, TrashContextExtMacos as _};
                let mut trash = trash;
                // Use the native API without launching Finder or requesting automation access.
                trash.set_delete_method(DeleteMethod::NsFileManager);
                trash
            };
            // A failed trash operation must never fall back to permanent deletion.
            trash.delete_all(untracked).map_err(|error| {
                Error::from(format!(
                    "Could not move untracked files to the Trash: {error}"
                ))
            })?;
        }
        Ok(String::new())
    }
}

fn discard_path(root: &Path, path: &Path) -> Result<PathBuf, Error> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Invalid Git file path".into());
    }
    let mut parent = root.to_path_buf();
    for part in path.parent().into_iter().flat_map(Path::components) {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(
                    "Cannot discard files through a symlink or non-directory parent.".into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(path))
}
