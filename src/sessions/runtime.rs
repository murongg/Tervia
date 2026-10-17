use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, Write};
#[cfg(not(target_os = "macos"))]
use std::process::{Command, Stdio};

pub(super) fn private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::other(
            "Terminal storage cannot be a symbolic link",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if fs::metadata(path)?.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::other("Terminal storage belongs to another user"));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(super) fn write_private(path: &Path, value: &Value) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| io::Error::other("Missing terminal directory"))?,
    )?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.as_file().sync_all()?;
    // Presence is the ready/finished signal: readers must never observe partially written JSON.
    file.persist_noclobber(path).map_err(|error| error.error)?;
    Ok(())
}

fn executable(root: &Path, source: &Path) -> io::Result<PathBuf> {
    let runtime = root.join("runtime");
    private_directory(&runtime)?;
    let mut hasher = Sha256::new();
    let mut source_file = fs::File::open(source)?;
    let mut block = [0u8; 65536];
    loop {
        let count = source_file.read(&mut block)?;
        if count == 0 {
            break;
        }
        hasher.update(&block[..count]);
    }
    let hash = format!("{:x}", hasher.finalize());
    let destination = runtime.join(format!("{hash}{}", std::env::consts::EXE_SUFFIX));
    // Hosts run immutable copies, so replacing an app bundle/EXE cannot invalidate a live PTY.
    if !destination.exists() {
        let mut temporary = tempfile::NamedTempFile::new_in(&runtime)?;
        // Keep the same open image if an updater replaces the installation path between hashing and copying.
        source_file.rewind()?;
        io::copy(&mut source_file, &mut temporary)?;
        temporary
            .as_file()
            .set_permissions(source_file.metadata()?.permissions())?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&destination) {
            Ok(_) => {}
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error),
        }
    }
    Ok(destination)
}

pub(super) fn spawn(
    root: &Path,
    source: &Path,
    directory: &Path,
    environment: &[(OsString, OsString)],
) -> io::Result<()> {
    let executable = executable(root, source)?;
    #[cfg(target_os = "macos")]
    {
        super::macos::spawn(&executable, directory, environment)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut command = Command::new(executable);
        command
            .arg("--terminal-host")
            .arg(directory)
            .envs(environment.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x00000008 | 0x00000200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP.
        }
        let mut child = command.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
