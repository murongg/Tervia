use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub(super) fn configure(command: &mut Command) {
    // Even `go version` and gopls project analysis can download an SDK in auto
    // mode. Every owned probe and service must stay within installed toolchains.
    command.env("GOTOOLCHAIN", "local");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
}

pub(super) fn terminate(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        if let Some(root) = std::env::var_os("SystemRoot") {
            let _ = Command::new(PathBuf::from(root).join("System32/taskkill.exe"))
                .args(["/T", "/F", "/PID", &child.id().to_string()])
                .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(super) fn run(
    mut command: Command,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<String, String> {
    if stop.load(Ordering::Relaxed) {
        return Err("Language service preparation canceled".into());
    }
    let mut output = tempfile::tempfile().map_err(|error| error.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|error| error.to_string())?)
        .stderr(output.try_clone().map_err(|error| error.to_string())?);
    configure(&mut command);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(error) => break Err(error.to_string()),
            Ok(None) => {}
        }
        if stop.load(Ordering::Relaxed) {
            break Err("Language service preparation canceled".into());
        }
        if Instant::now() >= deadline {
            break Err("Language service preparation timed out".into());
        }
        let length = match output.metadata() {
            Ok(metadata) => metadata.len(),
            Err(error) => break Err(error.to_string()),
        };
        if length > 1024 * 1024 {
            break Err("Language service command output exceeds 1 MiB".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    if result.is_err() {
        terminate(&mut child);
    }
    let status = result?;
    output
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut text = String::new();
    output
        .take(64 * 1024)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(text.trim().into())
    } else {
        Err(format!("Language service command failed: {}", text.trim()))
    }
}

pub(super) fn find(program: &str, paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .filter(|path| path.is_absolute())
        .find_map(|directory| {
            #[cfg(windows)]
            let candidate = ["exe", "cmd", "bat", "com"]
                .into_iter()
                .map(|extension| directory.join(format!("{program}.{extension}")))
                .find(|path| path.is_file());
            #[cfg(not(windows))]
            let candidate = Some(directory.join(program));
            candidate.filter(|path| executable(path))
        })
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(super) fn with_paths(command: &mut Command, paths: &[PathBuf]) -> Result<(), String> {
    let inherited = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    command.env(
        "PATH",
        std::env::join_paths(paths.iter().chain(inherited.iter()))
            .map_err(|error| error.to_string())?,
    );
    Ok(())
}
