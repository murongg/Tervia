use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub enum Operation {
    CreateFile(PathBuf),
    CreateDirectory(PathBuf),
    Move { from: PathBuf, to: PathBuf },
    Copy { from: PathBuf, to: PathBuf },
    Trash(PathBuf),
}

pub fn apply(root: &Path, operation: &Operation) -> Result<(), String> {
    apply_with_trash(root, operation, |path| {
        trash::delete(path).map_err(|error| error.to_string())
    })
}

fn apply_with_trash(
    root: &Path,
    operation: &Operation,
    trash: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    match operation {
        Operation::CreateFile(path) => {
            let path = resolve(&root, path)?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        Operation::CreateDirectory(path) => {
            fs::create_dir(resolve(&root, path)?).map_err(|error| error.to_string())
        }
        Operation::Move { from, to } | Operation::Copy { from, to } => {
            let from = resolve(&root, from)?;
            let to = resolve(&root, to)?;
            let metadata = fs::symlink_metadata(&from).map_err(|error| error.to_string())?;
            if to.starts_with(&from) || from == to {
                return Err("Choose a destination outside the source directory".into());
            }
            match fs::symlink_metadata(&to) {
                Ok(_) if matches!(operation, Operation::Move { .. }) && case_alias(&from, &to)? => {
                    return rename_case(&from, &to);
                }
                Ok(_) => return Err("A file or folder already exists at the destination".into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            if matches!(operation, Operation::Move { .. }) {
                return rename(&from, &to).map_err(|error| error.to_string());
            }
            // Stage copies beside their destination: a failed recursive copy leaves no partial entry.
            let staging = tempfile::Builder::new()
                .prefix(".tervia-copy-")
                .tempdir_in(to.parent().unwrap())
                .map_err(|error| error.to_string())?;
            let staged = staging.path().join("entry");
            copy(&from, &staged, &metadata)?;
            rename(&staged, &to).map_err(|error| error.to_string())
        }
        Operation::Trash(path) => {
            let path = resolve(&root, path)?;
            fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            trash(&path)
        }
    }
}

fn resolve(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty()
        || path.components().any(
            |part| !matches!(part, Component::Normal(name) if !name.eq_ignore_ascii_case(".git")),
        )
    {
        return Err("Choose a path inside the project, outside Git metadata".into());
    }
    let mut absolute = root.to_path_buf();
    let mut parts = path.components().peekable();
    while let Some(part) = parts.next() {
        absolute.push(part);
        if parts.peek().is_some() {
            let metadata = fs::symlink_metadata(&absolute).map_err(|error| error.to_string())?;
            // Ancestor aliases would make document paths ambiguous and could escape the project.
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(
                    "Parent must be a directory inside the project, not a symbolic link".into(),
                );
            }
        }
    }
    Ok(absolute)
}

fn copy(from: &Path, to: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    if metadata.is_symlink() {
        return Err("Copy symbolic links using the system file manager".into());
    }
    if metadata.is_file() {
        fs::copy(from, to)
            .map(|_| ())
            .map_err(|error| error.to_string())
    } else if metadata.is_dir() {
        fs::create_dir(to).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(from).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
            copy(&entry.path(), &to.join(entry.file_name()), &metadata)?;
        }
        fs::set_permissions(to, metadata.permissions()).map_err(|error| error.to_string())
    } else {
        Err("Only regular files and folders can be copied".into())
    }
}

fn case_alias(from: &Path, to: &Path) -> Result<bool, String> {
    if from == to
        || from.parent() != to.parent()
        || !from
            .file_name()
            .unwrap_or_default()
            .eq_ignore_ascii_case(to.file_name().unwrap_or_default())
    {
        return Ok(false);
    }
    // A case-sensitive volume can have two distinct entries, even when their names fold equally.
    for entry in fs::read_dir(to.parent().unwrap()).map_err(|error| error.to_string())? {
        if entry.map_err(|error| error.to_string())?.file_name() == to.file_name().unwrap() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn rename_case(from: &Path, to: &Path) -> Result<(), String> {
    // No-replace syscalls reject aliases of the source too, so change spelling through a private slot.
    let staging = tempfile::Builder::new()
        .prefix(".tervia-rename-")
        .tempdir_in(from.parent().unwrap())
        .map_err(|error| error.to_string())?;
    let staged = staging.path().join("entry");
    rename(from, &staged).map_err(|error| error.to_string())?;
    if let Err(error) = rename(&staged, to) {
        if let Err(rollback) = rename(&staged, from) {
            // Never let TempDir cleanup remove the original if another process prevents rollback.
            let retained = staging.keep().join("entry");
            return Err(format!(
                "{error}. Could not restore the original ({rollback}); it is preserved at {}",
                retained.display()
            ));
        }
        return Err(error.to_string());
    }
    Ok(())
}

fn rename(from: &Path, to: &Path) -> io::Result<()> {
    // The no-replace syscall also protects a destination created after our initial existence check.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let from = CString::new(from.as_os_str().as_bytes())?;
        let to = CString::new(to.as_os_str().as_bytes())?;
        #[cfg(target_os = "macos")]
        let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_passes_the_entry_to_the_system_without_following_its_target() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("mock.rs"), "synthetic source").unwrap();
        let expected = root.path().canonicalize().unwrap().join("mock.rs");
        apply_with_trash(root.path(), &Operation::Trash("mock.rs".into()), |path| {
            assert_eq!(path, expected);
            fs::remove_file(path).map_err(|error| error.to_string())
        })
        .unwrap();
        assert!(!expected.exists());
    }
}
