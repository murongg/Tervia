use std::{
    ffi::OsString,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::pipe::{Capture, OutputPipe};

pub(super) fn run(root: &Path, args: &[OsString]) -> Result<Vec<u8>, String> {
    run_input(root, args, None)
}

pub(super) fn run_input(
    root: &Path,
    args: &[OsString],
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    command.arg("--no-pager");
    // Stash internally uses magic pathspecs to remove saved untracked files.
    // Scope literal paths to our file commands, never Git's compound operations.
    if args
        .first()
        .is_some_and(|arg| matches!(arg.to_str(), Some("add" | "restore" | "rm" | "diff")))
    {
        command.arg("--literal-pathspecs");
    }
    command
        .arg("-C")
        .arg(root)
        .args(["-c", "color.ui=false", "-c", "core.quotepath=false"])
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true");
    // A GUI may inherit a launcher's repository environment. Every request must
    // target the explicitly opened project, including its real index.
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(key);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot start Git: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("Git stdout unavailable")?;
    let mut stderr = child.stderr.take().ok_or("Git stderr unavailable")?;
    let writer = input.and_then(|input| {
        child
            .stdin
            .take()
            .map(|mut stdin| thread::spawn(move || stdin.write_all(&input)))
    });
    stdout.prepare().map_err(|error| error.to_string())?;
    stderr.prepare().map_err(|error| error.to_string())?;
    let mut output = Capture::default();
    let mut errors = Capture::default();
    let started = Instant::now();
    let timeout = if args.first().is_some_and(|arg| {
        matches!(
            arg.to_str(),
            Some("status" | "diff" | "log" | "show" | "rev-parse" | "for-each-ref" | "config")
        )
    }) {
        Duration::from_secs(90)
    } else {
        Duration::from_secs(1800)
    };
    let status = loop {
        output
            .drain(&mut stdout)
            .map_err(|error| error.to_string())?;
        errors
            .drain(&mut stderr)
            .map_err(|error| error.to_string())?;
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => {
                // Git has flushed its own output before exit. Drain available
                // bytes without waiting for background hooks to close their pipes.
                output
                    .drain(&mut stdout)
                    .map_err(|error| error.to_string())?;
                errors
                    .drain(&mut stderr)
                    .map_err(|error| error.to_string())?;
                break status;
            }
            None if started.elapsed() > timeout => {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                #[cfg(windows)]
                {
                    let program = std::env::var_os("SystemRoot")
                        .map(std::path::PathBuf::from)
                        .map(|path| path.join("System32/taskkill.exe"))
                        .unwrap_or_else(|| "taskkill.exe".into());
                    let _ = Command::new(program)
                        .args(["/T", "/F", "/PID", &child.id().to_string()])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err("Git timed out. Inspect the repository before retrying; an operation may have partially completed.".into());
            }
            None => thread::sleep(Duration::from_millis(15)),
        }
    };
    let stdout = output.finish()?;
    let stderr = errors.finish()?;
    // Only git apply feeds stdin here; it consumes the finite patch before exit.
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    if status.success() {
        Ok(stdout)
    } else {
        let message = format!(
            "{}\n{}",
            String::from_utf8_lossy(&stderr).trim(),
            String::from_utf8_lossy(&stdout).trim()
        );
        Err(format!(
            "git {}: {}",
            args.first()
                .map(|arg| arg.to_string_lossy())
                .unwrap_or_default(),
            message.trim()
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn a_background_hook_cannot_hold_a_completed_git_command_open() {
        let root = std::env::temp_dir().join(format!("tervia-pipe-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
        run(&root, &args(&["init", "--template=", "-b", "main"])).unwrap();
        fs::write(root.join("mock.txt"), "synthetic content\n").unwrap();
        run(&root, &args(&["add", "mock.txt"])).unwrap();
        let hooks = root.join("hooks");
        fs::create_dir(&hooks).unwrap();
        let hook = hooks.join("post-commit");
        fs::write(&hook, "#!/bin/sh\nsleep 4 &\n").unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();
        let result = run(
            &root,
            &args(&[
                "-c",
                "user.name=Synthetic Developer",
                "-c",
                "user.email=developer@example.test",
                "-c",
                "commit.gpgsign=false",
                "-c",
                &format!("core.hooksPath={}", hooks.display()),
                "commit",
                "-m",
                "Synthetic hook test",
            ]),
        );
        fs::remove_dir_all(root).unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
