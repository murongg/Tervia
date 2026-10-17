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

pub(super) fn fingerprint(reader: &mut impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut block = [0u8; 65536];
    loop {
        let count = reader.read(&mut block)?;
        if count == 0 {
            break;
        }
        hasher.update(&block[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn executable(root: &Path, source: &Path) -> io::Result<PathBuf> {
    let runtime = root.join("runtime");
    private_directory(&runtime)?;
    let mut source_file = fs::File::open(source)?;
    let hash = fingerprint(&mut source_file)?;
    #[cfg(target_os = "macos")]
    if let Some(image) = super::macos::cache_bundle(&runtime, source, &hash)? {
        return Ok(image);
    }
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
        // Other launches can reuse the hash path immediately: close our writer before publishing it.
        match temporary.into_temp_path().persist_noclobber(&destination) {
            Ok(_) => {}
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error),
        }
    }
    Ok(destination)
}

#[cfg(any(test, not(target_os = "macos")))]
fn retry_executable<T>(mut spawn: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    // Concurrent fork/exec can briefly inherit a writable image handle despite CLOEXEC.
    // ETXTBSY means no child started, so retry it within a bound without masking other failures.
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match spawn() {
            Err(error)
                if error.kind() == io::ErrorKind::ExecutableFileBusy
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
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
        let mut child = retry_executable(|| command.spawn())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn a_cached_app_retains_its_signature_after_the_installation_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("mock.app");
        let contents = bundle.join("Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::create_dir_all(contents.join("Resources")).unwrap();
        let source = contents.join("MacOS/mock-host");
        fs::copy(std::env::current_exe().unwrap(), &source).unwrap();
        fs::write(contents.join("Resources/mock.txt"), "mock-resource").unwrap();
        fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>test.tervia.mock-host</string>
<key>CFBundleExecutable</key><string>mock-host</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>"#,
        )
        .unwrap();
        let signed = std::process::Command::new("/usr/bin/codesign")
            .args([
                "--force",
                "--sign",
                "-",
                "--timestamp=none",
                "--options",
                "runtime",
            ])
            .arg(&bundle)
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "{}",
            String::from_utf8_lossy(&signed.stderr)
        );
        let cached = executable(root.path(), &source).unwrap();
        assert_eq!(cached, executable(root.path(), &source).unwrap());
        fs::remove_dir_all(&bundle).unwrap();
        let checked = std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(&cached)
            .output()
            .unwrap();
        assert!(
            checked.status.success(),
            "{}",
            String::from_utf8_lossy(&checked.stderr)
        );
        let resources = cached
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("Resources/mock.txt");
        assert_eq!(fs::read_to_string(resources).unwrap(), "mock-resource");
    }

    #[test]
    fn an_executable_that_is_temporarily_busy_is_retried() {
        let mut attempts = 0;
        let result = retry_executable(|| {
            attempts += 1;
            if attempts < 3 {
                Err(io::Error::new(
                    io::ErrorKind::ExecutableFileBusy,
                    "synthetic busy executable",
                ))
            } else {
                Ok("synthetic ready executable")
            }
        });
        assert_eq!(result.unwrap(), "synthetic ready executable");
        assert_eq!(attempts, 3);
    }

    #[test]
    fn other_executable_errors_are_returned_without_retrying() {
        for kind in [
            io::ErrorKind::NotFound,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::Other,
        ] {
            let mut attempts = 0;
            let error = retry_executable::<()>(|| {
                attempts += 1;
                Err(io::Error::new(kind, "synthetic launch failure"))
            })
            .unwrap_err();
            assert_eq!(error.kind(), kind);
            assert_eq!(attempts, 1);
        }
    }

    #[test]
    fn an_executable_that_stays_busy_returns_the_last_error() {
        let mut attempts = 0;
        let error = retry_executable::<()>(|| {
            attempts += 1;
            Err(io::Error::new(
                io::ErrorKind::ExecutableFileBusy,
                "synthetic persistent writer",
            ))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ExecutableFileBusy);
        assert_eq!(error.to_string(), "synthetic persistent writer");
        assert!(attempts > 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn spawning_waits_for_an_executable_writer_to_close() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("mock-host");
        fs::write(&source, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
        let image = executable(root.path(), &source).unwrap();
        let writer = fs::OpenOptions::new().write(true).open(image).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(writer);
        });
        let result = spawn(root.path(), &source, root.path(), &[]);
        release.join().unwrap();
        assert!(
            result.is_ok(),
            "A transient executable writer must not prevent terminal startup: {result:?}"
        );
    }
}
