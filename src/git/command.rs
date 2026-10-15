use std::{
    ffi::OsString,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{
    Error, ErrorKind,
    pipe::{Capture, OutputPipe},
};

pub(super) fn run(root: &Path, args: &[OsString]) -> Result<Vec<u8>, Error> {
    run_input(root, args, None)
}

pub(super) fn probe(root: &Path, args: &[OsString]) -> Result<Vec<u8>, Error> {
    let mut command = configured(root, args);
    // Locale normalization belongs only to absence classification. Mutations,
    // hooks, signing, and authentication retain the user's inherited locale.
    command.env("LC_ALL", "C").env("LANGUAGE", "C");
    execute(command, root, args, None, timeout(args))
}

pub(super) fn run_input(
    root: &Path,
    args: &[OsString],
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>, Error> {
    execute(configured(root, args), root, args, input, timeout(args))
}

pub(super) fn run_index(root: &Path, index: &Path, args: &[OsString]) -> Result<Vec<u8>, Error> {
    let mut command = configured(root, args);
    // Only checkpoint capture may substitute an application-owned temporary index.
    command.env("GIT_INDEX_FILE", index);
    execute(command, root, args, None, timeout(args))
}

fn configured(root: &Path, args: &[OsString]) -> Command {
    let mut command = Command::new("git");
    command.arg("--no-pager");
    // Stash internally uses magic pathspecs to remove saved untracked files.
    // Scope literal paths to our file commands, never Git's compound operations.
    if args.first().is_some_and(|arg| {
        matches!(
            arg.to_str(),
            Some("add" | "restore" | "rm" | "diff" | "ls-files")
        )
    }) {
        command.arg("--literal-pathspecs");
    }
    command
        .arg("-C")
        .arg(root)
        .args(["-c", "color.ui=false", "-c", "core.quotepath=false"])
        .args(args)
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
    command
}

fn timeout(args: &[OsString]) -> Duration {
    if args.first().is_some_and(|arg| {
        matches!(
            arg.to_str(),
            Some("status" | "diff" | "log" | "show" | "rev-parse" | "for-each-ref" | "config")
        )
    }) {
        Duration::from_secs(90)
    } else {
        Duration::from_secs(1800)
    }
}

fn execute(
    mut command: Command,
    root: &Path,
    args: &[OsString],
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Vec<u8>, Error> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| Error::start(root, args, error))?;
    let mut stdout = child.stdout.take().ok_or("Git stdout unavailable")?;
    let mut stderr = child.stderr.take().ok_or("Git stderr unavailable")?;
    let writer = input.and_then(|input| {
        child
            .stdin
            .take()
            .map(|mut stdin| thread::spawn(move || stdin.write_all(&input)))
    });
    let mut output = Capture::default();
    let mut errors = Capture::default();
    let started = Instant::now();
    let result = (|| -> Result<_, Error> {
        stdout.prepare()?;
        stderr.prepare()?;
        loop {
            output.drain(&mut stdout)?;
            errors.drain(&mut stderr)?;
            match child.try_wait()? {
                Some(status) => {
                    // Git has flushed its own output before exit. Drain available
                    // bytes without waiting for background hooks to close their pipes.
                    output.drain(&mut stdout)?;
                    errors.drain(&mut stderr)?;
                    return Ok(status);
                }
                None if started.elapsed() > timeout => {
                    return Err(Error::process(
                        ErrorKind::Timeout, root, args, None, Vec::new(), Vec::new(),
                        "Git timed out. Inspect the repository before retrying; an operation may have partially completed.".into(),
                    ));
                }
                None => thread::sleep(Duration::from_millis(15)),
            }
        }
    })();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            terminate(&mut child);
            let status = child.wait().ok();
            // Retain bytes already emitted before termination, within the same
            // capture bounds. Never wait for inherited hook pipe handles to close.
            let _ = output.drain(&mut stdout);
            let _ = errors.drain(&mut stderr);
            return Err(error.with_process(
                root,
                args,
                status,
                output.bytes().to_vec(),
                errors.bytes().to_vec(),
            ));
        }
    };
    if output.oversized() || errors.oversized() {
        return Err(Error::process(
            if status.success() {
                ErrorKind::Message
            } else {
                ErrorKind::Command
            },
            root,
            args,
            Some(status),
            output.bytes().to_vec(),
            errors.bytes().to_vec(),
            "Git output exceeds 8 MiB; narrow the comparison or history.".into(),
        ));
    }
    let stdout = output.finish()?;
    let stderr = errors.finish()?;
    // Git consumes finite patch or pathspec input before exit.
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    if status.success() {
        Ok(stdout)
    } else {
        Err(Error::process(
            ErrorKind::Command,
            root,
            args,
            Some(status),
            stdout,
            stderr,
            String::new(),
        ))
    }
}

fn terminate(child: &mut std::process::Child) {
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
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn failed_commands_keep_status_directory_and_both_output_streams() {
        let root = tempfile::tempdir().unwrap();
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
        run(root.path(), &args(&["init", "--template=", "-b", "main"])).unwrap();
        run(
            root.path(),
            &args(&[
                "config",
                "alias.synthetic",
                "!printf 'synthetic stdout\\n'; printf 'synthetic stderr\\n' >&2; exit 7",
            ]),
        )
        .unwrap();
        let error = run(root.path(), &args(&["synthetic"])).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Command);
        assert_eq!(error.exit_code(), Some(7));
        assert!(!error.status().unwrap().success());
        assert_eq!(error.directory(), Some(root.path()));
        assert_eq!(error.arguments(), args(&["synthetic"]));
        assert_eq!(error.stdout(), b"synthetic stdout\n");
        assert_eq!(error.stderr(), b"synthetic stderr\n");
        let detail = error.to_string();
        assert!(detail.contains("synthetic stdout"), "{detail}");
        assert!(detail.contains("synthetic stderr"), "{detail}");
        assert!(detail.contains("exit code: 7"), "{detail}");
        assert!(
            detail.contains(&root.path().display().to_string()),
            "{detail}"
        );
        assert!(detail.contains("stdout:"), "{detail}");
        assert!(detail.contains("stderr:"), "{detail}");
    }

    #[test]
    fn missing_executable_and_non_executable_file_have_different_kinds() {
        let root = tempfile::tempdir().unwrap();
        let args = vec!["status".into()];
        let missing = execute(
            Command::new(root.path().join("synthetic-missing-git")),
            root.path(),
            &args,
            None,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(missing.kind(), ErrorKind::MissingExecutable);
        assert_eq!(missing.directory(), Some(root.path()));
        let program = root.path().join("synthetic-unexecutable-git");
        fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o600)).unwrap();
        let error = execute(
            Command::new(program),
            root.path(),
            &args,
            None,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Start);
        assert_eq!(error.arguments(), args);
    }

    #[test]
    fn timeout_keeps_partial_output_without_recording_stdin_or_argument_values() {
        let root = tempfile::tempdir().unwrap();
        let args = vec![
            "commit".into(),
            "-m".into(),
            "synthetic private body".into(),
        ];
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 'synthetic partial stdout\\n'; printf 'synthetic partial stderr\\n' >&2; sleep 30"]);
        let started = Instant::now();
        let error = execute(
            command,
            root.path(),
            &args,
            Some(b"synthetic stdin payload".to_vec()),
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(error.kind(), ErrorKind::Timeout);
        assert_eq!(error.stdout(), b"synthetic partial stdout\n");
        assert_eq!(error.stderr(), b"synthetic partial stderr\n");
        assert_eq!(error.arguments(), args);
        assert!(error.status().is_some());
        let detail = error.details();
        assert!(detail.contains("may have partially completed"));
        assert!(!detail.contains("stdin payload"));
        assert!(!detail.contains("private body"));
    }

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
