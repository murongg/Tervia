//! A one-shot launchd job gives terminal hosts their own macOS resource coalition.
//! setsid alone cannot prevent loginwindow from terminating foreground programs when the GUI exits.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    os::unix::ffi::OsStrExt,
    process::{Command, Stdio},
};

pub(super) fn cache_bundle(
    runtime: &Path,
    source: &Path,
    hash: &str,
) -> io::Result<Option<PathBuf>> {
    let Some(macos) = source.parent().filter(|path| path.ends_with("MacOS")) else {
        return Ok(None);
    };
    let Some(contents) = macos.parent().filter(|path| path.ends_with("Contents")) else {
        return Ok(None);
    };
    let Some(bundle) = contents
        .parent()
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
    else {
        return Ok(None);
    };
    let relative = source.strip_prefix(bundle).map_err(io::Error::other)?;
    let destination = runtime.join(format!("{hash}.app"));
    if !destination.exists() {
        let temporary = tempfile::tempdir_in(runtime)?;
        let prepared = temporary.path().join("host.app");
        // A signed main executable depends on its Info.plist and resource seal; a bare copy cannot execute.
        // ditto preserves the complete signed bundle, including nested code, symlinks and notarization metadata.
        let copied = Command::new("/usr/bin/ditto")
            .arg(bundle)
            .arg(&prepared)
            .output()?;
        if !copied.status.success() {
            return Err(io::Error::other(format!(
                "Cannot copy terminal host application: {}",
                String::from_utf8_lossy(&copied.stderr).trim()
            )));
        }
        if runtime::fingerprint(&mut fs::File::open(prepared.join(relative))?)? != hash {
            return Err(io::Error::other(
                "The application changed while preparing its terminal host; retry the terminal",
            ));
        }
        if prepared
            .join("Contents/_CodeSignature/CodeResources")
            .exists()
        {
            let checked = Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&prepared)
                .output()?;
            if !checked.status.success() {
                return Err(io::Error::other(format!(
                    "Terminal host application signature is invalid: {}",
                    String::from_utf8_lossy(&checked.stderr).trim()
                )));
            }
        }
        // Publish only a complete verified bundle; another window may have prepared the same image first.
        match fs::rename(&prepared, &destination) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(Some(destination.join(relative)))
}

fn domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}

fn label(directory: &Path) -> String {
    // The namespace includes the root: isolated workspaces can reuse a session identifier.
    format!(
        "cn.tervia.session.{:x}",
        Sha256::digest(directory.as_os_str().as_bytes())
    )
}

fn xml(value: &str) -> io::Result<String> {
    if value
        .chars()
        .any(|c| c < ' ' && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err(io::Error::other(
            "Invalid character in terminal launcher path",
        ));
    }
    Ok(value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}

pub(super) fn spawn(
    executable: &Path,
    directory: &Path,
    overrides: &[(OsString, OsString)],
) -> io::Result<()> {
    let mut environment: std::collections::BTreeMap<OsString, OsString> =
        std::env::vars_os().collect();
    environment.extend(overrides.iter().cloned());
    // launchd does not inherit the GUI environment. Transfer it privately and erase it before starting the PTY.
    runtime::write_private(
        &directory.join("environment.json"),
        &serde_json::to_value(environment.into_iter().collect::<Vec<_>>())?,
    )?;
    let executable = xml(executable
        .to_str()
        .ok_or_else(|| io::Error::other("Invalid terminal executable path"))?)?;
    let path = xml(directory
        .to_str()
        .ok_or_else(|| io::Error::other("Invalid terminal directory"))?)?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{}</string>
<key>ProgramArguments</key><array><string>{executable}</string><string>--terminal-launcher</string><string>{path}</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><false/>
<key>AbandonProcessGroup</key><true/>
</dict></plist>"#,
        label(directory)
    );
    let mut file = tempfile::Builder::new()
        .suffix(".plist")
        .tempfile_in(directory)?;
    file.write_all(plist.as_bytes())?;
    file.flush()?;
    let result = Command::new("/bin/launchctl")
        .args(["bootstrap", &domain()])
        .arg(file.path())
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "Cannot start persistent terminal: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(())
}

pub(super) fn launch(directory: &Path) -> io::Result<()> {
    let path = directory.join("environment.json");
    let bytes = fs::read(&path)?;
    fs::remove_file(path)?;
    let environment: Vec<(OsString, OsString)> = serde_json::from_slice(&bytes)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--terminal-detached-host")
        .arg(directory)
        .arg(std::process::id().to_string())
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
    // The job must exit before the PTY starts. Keeping a live GUI-domain job ties its lifetime to logout cleanup.
    command.spawn()?;
    Ok(())
}

pub(super) fn wait_for_launcher(parent: i32) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while unsafe { libc::getppid() } == parent {
        if Instant::now() >= deadline {
            return Err(io::Error::other("Terminal launcher did not exit"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

pub(super) fn cleanup(directory: &Path) {
    // AbandonProcessGroup leaves the detached host alone; finished jobs must not accumulate in launchd.
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("{}/{}", domain(), label(directory))])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
