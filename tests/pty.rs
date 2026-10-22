#![cfg(feature = "pty")]

#[cfg(unix)]
use portable_pty::CommandBuilder;
use portable_pty::PtySize;
use tervia::pty::{PtyEvent, PtySession, initial_directory};

#[cfg(unix)]
#[test]
fn a_discovered_make_task_runs_in_its_directory_and_reports_failure() {
    use std::{
        ffi::OsStr,
        time::{Duration, Instant},
    };
    let root = tempfile::Builder::new()
        .prefix("mock tasks ")
        .tempdir()
        .unwrap();
    std::fs::write(root.path().join("Makefile"), ".PHONY: mock-check\nmock-check:\n\t@printf 'mock-make-output\\n'\n\t@printf 'mock marker\\n' > mock-ran\n\t@exit 7\n").unwrap();
    let task = tervia::tasks::discover(root.path()).tasks.remove(0);
    assert!(!root.path().join("mock-ran").exists());
    let mut command =
        tervia::pty::task_command_for(OsStr::new("/bin/sh"), root.path(), &task).unwrap();
    command.env_clear();
    command.env("HOME", root.path());
    command.env("PATH", "/usr/bin:/bin");
    command.env("TERM", "xterm-256color");
    let (_session, receiver) = PtySession::spawn(
        command,
        PtySize {
            rows: 12,
            cols: 90,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut buffer = tervia::terminal::TerminalBuffer::new(90, 12);
    let mut status = None;
    while Instant::now() < deadline && status.is_none() {
        match receiver.try_recv() {
            Ok(PtyEvent::Data(bytes)) => buffer.feed(&bytes),
            Ok(PtyEvent::Exited(exit)) => status = Some(exit),
            Ok(PtyEvent::Error(error)) => panic!("{error}"),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    assert_eq!(status.as_deref(), Some("Exited with code 2"));
    assert!(buffer.text().contains("mock-make-output"));
    assert_eq!(
        std::fs::read_to_string(root.path().join("mock-ran")).unwrap(),
        "mock marker\n"
    );
}

#[test]
fn task_launch_keeps_script_names_single_arguments_and_does_not_leave_an_interactive_shell() {
    use std::{ffi::OsStr, path::Path};
    use tervia::tasks::Task;
    let task = Task {
        label: "mock 'quote' $(touch unexpected)".into(),
        source: "package.json".into(),
        program: "npm".into(),
        args: vec!["run".into(), "mock 'quote' $(touch unexpected)".into()],
    };
    for shell in ["/bin/bash", "/bin/zsh", "/usr/bin/fish"] {
        let command =
            tervia::pty::task_command_for(OsStr::new(shell), Path::new("mock-project"), &task)
                .unwrap();
        assert_eq!(
            command.get_argv()[1..4],
            ["-l", "-i", "-c"].map(std::ffi::OsString::from)
        );
        assert_eq!(
            shlex::split(&command.get_argv()[4].to_string_lossy()).unwrap(),
            ["npm", "run", task.args[1].as_str()]
        );
        assert_eq!(
            command.get_cwd().unwrap().as_os_str(),
            OsStr::new("mock-project")
        );
    }
    let command =
        tervia::pty::task_command_for(OsStr::new("pwsh.exe"), Path::new("mock-project"), &task)
            .unwrap();
    let args: Vec<_> = command
        .get_argv()
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert_eq!(&args[1..3], ["-NoLogo", "-Command"]);
    assert!(args[3].contains("mock ''quote'' $(touch unexpected)"));
    assert!(args[3].ends_with("exit $LASTEXITCODE"));
    let mut invalid = task.clone();
    invalid.args[1] = "mock%PATH%".into();
    assert!(
        tervia::pty::task_command_for(OsStr::new("cmd.exe"), Path::new("mock-project"), &invalid)
            .is_err()
    );
}

#[cfg(windows)]
#[test]
fn shell_working_directories_use_compatible_paths_without_changing_their_meaning() {
    use std::{ffi::OsStr, path::Path};
    let long = format!(r"\\?\C:\synthetic\{}", "mock-folder\\".repeat(30));
    for (source, expected) in [
        (
            r"\\?\C:\synthetic project\项目 & mock",
            r"C:\synthetic project\项目 & mock",
        ),
        (
            r"C:\synthetic project\项目 & mock",
            r"C:\synthetic project\项目 & mock",
        ),
        (
            r"\\mock-server\mock-share\project",
            r"\\mock-server\mock-share\project",
        ),
        (
            r"\\?\UNC\mock-server\mock-share\project",
            r"\\?\UNC\mock-server\mock-share\project",
        ),
        (r"\\?\C:\synthetic\trailing.", r"\\?\C:\synthetic\trailing."),
        (r"\\?\C:\synthetic\CON", r"\\?\C:\synthetic\CON"),
        (long.as_str(), long.as_str()),
    ] {
        for shell in ["cmd.exe", "powershell.exe", "pwsh.exe"] {
            let command = tervia::pty::shell_command_for(OsStr::new(shell), Path::new(source));
            assert_eq!(
                command.get_cwd().map(|cwd| cwd.as_os_str()),
                Some(OsStr::new(expected)),
                "{shell}: {source}"
            );
        }
    }
}

#[test]
fn explicit_shell_commands_keep_paths_arguments_and_working_directories_separate() {
    use std::{ffi::OsStr, path::Path};
    for (shell, args) in [
        (r"C:\synthetic tools\pwsh.exe", vec!["-NoLogo", "-NoExit"]),
        (
            r"C:\synthetic tools\powershell.exe",
            vec!["-NoLogo", "-NoExit"],
        ),
        (r"C:\synthetic tools\cmd.exe", vec!["/K"]),
    ] {
        let cwd = Path::new("synthetic-project");
        let command = tervia::pty::shell_command_for(OsStr::new(shell), cwd);
        let actual: Vec<_> = command
            .get_argv()
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect();
        assert_eq!(actual[0], shell);
        assert_eq!(&actual[1..], args);
        assert_eq!(
            command.get_cwd().map(|path| path.as_os_str()),
            Some(cwd.as_os_str())
        );
    }
}

#[cfg(unix)]
#[test]
fn shell_integration_preserves_startup_files_and_reports_command_boundaries() {
    use std::time::{Duration, Instant};
    use tervia::{
        pty::ShellIntegration,
        terminal::shell::{Event, Tracker},
    };
    for shell in ["/bin/zsh", "/bin/bash"] {
        if !std::path::Path::new(shell).is_file() {
            continue;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("custom history"),
            "git synthetic-before-start\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join(".zshrc"),
            "PS1='mock> '\nHISTFILE='custom history'\nSAVEHIST=100\nsetopt histignorespace\nprint synthetic-startup\nprecmd() { print synthetic-named-hook; }\n_mock_precmd() { print synthetic-array-hook; }\nprecmd_functions=(_mock_precmd)\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join(".bash_profile"),
            "PS1='mock> '\nHISTFILE='custom history'\nHISTCONTROL=\nHISTIGNORE=\nPROMPT_COMMAND=\"$PROMPT_COMMAND; printf synthetic-profile-prompt\"\nprintf 'synthetic-startup\\n'\nif shopt -q login_shell; then printf 'synthetic-login-mode\\n'; fi\n",
        )
        .unwrap();
        let mut command = CommandBuilder::new(shell);
        command.env_clear();
        command.env("HOME", root.path());
        command.env("PATH", "/usr/bin:/bin");
        command.env("TERM", "xterm-256color");
        if shell == "/bin/bash" {
            command.env("PROMPT_COMMAND", "printf synthetic-inherited-prompt");
        }
        command.args(["-l", "-i"]);
        command.cwd(root.path());
        let integration = ShellIntegration::configure(&mut command).unwrap().unwrap();
        let mut tracker = Tracker::new(integration.token().into(), shell);
        let (session, receiver) = PtySession::spawn(
            command,
            PtySize {
                rows: 16,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut output = String::new();
        let mut ready = false;
        let mut hidden_sent = false;
        let mut matching_prompt_command_sent = false;
        let mut commands = vec![];
        let mut history_path = None;
        while Instant::now() < deadline {
            match receiver.try_recv() {
                Ok(PtyEvent::Data(bytes)) => {
                    for event in tracker.feed(&bytes) {
                        match event {
                            Event::History(path) => history_path = path,
                            Event::Output(bytes) => {
                                output.push_str(&String::from_utf8_lossy(&bytes))
                            }
                            Event::Prompt(_) if !ready => {
                                ready = true;
                                let mut input = b"synthetic-discarded-line".to_vec();
                                input.extend(
                                    tervia::terminal::key_bytes(
                                        "u",
                                        true,
                                        false,
                                        false,
                                        Default::default(),
                                    )
                                    .unwrap(),
                                );
                                input.extend_from_slice(b"printf discarded");
                                input.extend(
                                    tervia::terminal::key_bytes(
                                        "backspace",
                                        true,
                                        false,
                                        false,
                                        Default::default(),
                                    )
                                    .unwrap(),
                                );
                                input.extend_from_slice(b"'synthetic-result\\n'; false\r");
                                session.write(&input).unwrap();
                            }
                            Event::Completed(command) => commands.push(command),
                            Event::Prompt(_) if commands.len() == 1 && !hidden_sent => {
                                hidden_sent = true;
                                session.write(b" printf 'synthetic-hidden\\n'\r").unwrap();
                            }
                            Event::Prompt(_)
                                if commands.len() == 2 && !matching_prompt_command_sent =>
                            {
                                matching_prompt_command_sent = true;
                                session.write(b"printf synthetic-profile-prompt\r").unwrap();
                            }
                            _ => {}
                        }
                    }
                }
                Ok(PtyEvent::Error(error)) => panic!("{shell}: {error}"),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
            if commands.len() == 3
                && (shell != "/bin/zsh" || output.matches("synthetic-array-hook").count() >= 4)
            {
                break;
            }
        }
        assert!(ready, "{shell} never became ready: {output}");
        let history_path = history_path.expect("Report the RC-configured history path");
        assert_eq!(
            std::fs::canonicalize(&history_path).unwrap(),
            std::fs::canonicalize(root.path().join("custom history")).unwrap()
        );
        let imported = tervia::suggestions::Store::new(None)
            .complete_from(
                "git syn",
                root.path(),
                tervia::suggestions::Shell::from_executable(shell),
                Some(&history_path),
            )
            .unwrap();
        assert_eq!(imported[0].command, "git synthetic-before-start");
        assert!(
            output.contains("synthetic-startup"),
            "{shell} did not load the user's startup file: {output}"
        );
        if shell == "/bin/bash" {
            assert!(
                output.contains("synthetic-login-mode"),
                "Bash is no longer a login shell: {output}"
            );
            assert!(output.contains("synthetic-inherited-prompt"));
            assert!(output.contains("synthetic-profile-prompt"));
        }
        assert_eq!(commands.len(), 3, "{shell}: {output}");
        assert_eq!(commands[0].input, "printf 'synthetic-result\\n'; false");
        assert_eq!(
            std::fs::canonicalize(&commands[0].cwd).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
        assert_eq!(commands[0].exit_code, 1);
        assert!(commands[0].output.contains("synthetic-result"));
        assert_eq!(commands[2].input, "printf synthetic-profile-prompt");
        assert!(
            commands[1].input.starts_with(' '),
            "{shell} lost the hidden-command marker: {:?}",
            commands[1].input
        );
        if shell == "/bin/zsh" {
            assert!(
                output.matches("synthetic-array-hook").count() >= 2,
                "The user's prompt hook stopped after a failed command: {output}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn zsh_startup_wrapper_preserves_the_real_default_history_before_user_rc() {
    use std::{
        path::Path,
        time::{Duration, Instant},
    };
    use tervia::{
        pty::ShellIntegration,
        suggestions::{Shell, Store},
        terminal::shell::{Event, Tracker},
    };
    if !Path::new("/bin/zsh").is_file() {
        return;
    }
    for custom_dotdir in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let settings = if custom_dotdir {
            root.path().join("shell settings")
        } else {
            root.path().to_owned()
        };
        std::fs::create_dir_all(&settings).unwrap();
        let history = settings.join(".zsh_history");
        std::fs::write(&history, "lsof -nP -iTCP -sTCP:LISTEN\n").unwrap();
        // A user's RC may read HISTFILE, not just assign it. It must already be correct here.
        std::fs::write(settings.join(".zshrc"), "PS1='mock> '\nSAVEHIST=100\n[[ $HISTFILE == \"${ZDOTDIR:-$HOME}/.zsh_history\" ]] || HISTFILE=''\n").unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.env_clear();
        command.env("HOME", root.path());
        command.env("PATH", "/usr/bin:/bin");
        command.env("TERM", "xterm-256color");
        if custom_dotdir {
            command.env("ZDOTDIR", &settings);
        }
        command.args(["-l", "-i"]);
        command.cwd(root.path());
        let integration = ShellIntegration::configure(&mut command).unwrap().unwrap();
        // Reproduce a system zshrc deriving its default from the injected ZDOTDIR,
        // including on systems whose global zshrc does not assign HISTFILE itself.
        let injected = std::path::PathBuf::from(command.get_env("ZDOTDIR").unwrap());
        command.env("HISTFILE", injected.join(".zsh_history"));
        let mut tracker = Tracker::new(integration.token().into(), "/bin/zsh");
        let (_session, receiver) = PtySession::spawn(
            command,
            PtySize {
                rows: 16,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut reported = None;
        let mut ready = false;
        while Instant::now() < deadline && !ready {
            if let Ok(PtyEvent::Data(bytes)) = receiver.try_recv() {
                for event in tracker.feed(&bytes) {
                    match event {
                        Event::History(path) => reported = path,
                        Event::Prompt(_) => ready = true,
                        _ => {}
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert!(ready, "Synthetic zsh never reached its prompt");
        let reported = reported.expect("The original default history must remain enabled");
        assert_eq!(
            std::fs::canonicalize(&reported).unwrap(),
            std::fs::canonicalize(&history).unwrap()
        );
        let candidates = Store::new(None)
            .complete_from("lsof", root.path(), Shell::Zsh, Some(&reported))
            .unwrap();
        assert_eq!(candidates[0].command, "lsof -nP -iTCP -sTCP:LISTEN");
    }
}

#[test]
fn a_missing_project_directory_falls_back_to_the_home_directory() {
    let project = std::path::Path::new("/synthetic-missing-tervia-project");
    let home = std::path::Path::new("/synthetic-home");
    assert_eq!(initial_directory(project, home), home);
}

#[cfg(unix)]
#[test]
fn a_clean_shell_round_trip_uses_a_real_tty() {
    let mut command = CommandBuilder::new("/bin/sh");
    command.env_clear();
    command.arg("-c");
    command.arg(
        "test -t 0 && printf 'synthetic-ready\\n'; read value; printf 'synthetic:%s\\n' \"$value\"",
    );
    let (session, receiver) = PtySession::spawn(
        command,
        PtySize {
            rows: 8,
            cols: 40,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
    .unwrap();
    session.write(b"mock-value\r").unwrap();
    let mut session = Some(session);
    let mut exited = false;
    let mut output = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "PTY round trip timed out"
        );
        match receiver.try_recv() {
            Ok(PtyEvent::Data(bytes)) => output.extend(bytes),
            Ok(PtyEvent::Exited(_)) => {
                exited = true;
                session.take();
            }
            Ok(PtyEvent::Error(error)) => panic!("{error}"),
            Ok(PtyEvent::Resized { .. } | PtyEvent::ReplayComplete) => {
                panic!("Direct PTYs must not emit session replay events")
            }
            Err(async_channel::TryRecvError::Closed) => break,
            Err(async_channel::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
        }
    }
    assert!(exited);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("synthetic-ready"));
    assert!(output.contains("synthetic:mock-value"));
}

#[cfg(unix)]
#[test]
fn dropping_a_session_terminates_a_synthetic_shell_that_ignores_hangup() {
    let mut command = CommandBuilder::new("/bin/sh");
    command.env_clear();
    command.args([
        "-c",
        "trap '' HUP; printf 'synthetic-hup-ready\\n'; while :; do sleep 1; done",
    ]);
    let (session, receiver) = PtySession::spawn(
        command,
        PtySize {
            rows: 8,
            cols: 40,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
    .unwrap();
    let pid = session.process_id().unwrap();
    let mut output = Vec::new();
    while !String::from_utf8_lossy(&output).contains("synthetic-hup-ready") {
        if let PtyEvent::Data(bytes) = receiver.recv_blocking().unwrap() {
            output.extend(bytes);
        }
    }
    drop(session);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while unsafe { libc::kill(pid as i32, 0) } == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_ne!(unsafe { libc::kill(pid as i32, 0) }, 0);
}

#[cfg(windows)]
#[test]
fn a_discovered_cmd_shell_round_trip_uses_conpty() {
    use alacritty_terminal::event::Event;
    use tervia::terminal::TerminalBuffer;

    let system_root = std::env::var_os("SystemRoot").expect("Windows must provide SystemRoot");
    let executable = tervia::shell::Profile::Cmd.resolve();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("roundtrip.cmd"),
        "@echo off\r\necho synthetic-ready\r\nset /p \"value=\"\r\necho synthetic:%value%\r\nexit\r\n",
    )
    .unwrap();
    // portable-pty resolves executables from the command's PATH. An isolated
    // environment needs an absolute executable and Windows' system directory.
    let mut command = tervia::pty::shell_command_for(&executable, directory.path());
    command.env_clear();
    command.env("SystemRoot", system_root);
    // /K runs the fixture, which exits after the interactive round trip.
    command.arg("roundtrip.cmd");
    let (session, receiver) = PtySession::spawn(
        command,
        PtySize {
            rows: 8,
            cols: 40,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
    .unwrap();
    let mut session = Some(session);
    let mut terminal = TerminalBuffer::new(40, 8);
    let mut raw_output = Vec::new();
    let mut input_sent = false;
    let mut exited = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match receiver.try_recv() {
            Ok(PtyEvent::Data(bytes)) => {
                raw_output.extend_from_slice(&bytes);
                terminal.feed(&bytes);
                // ConPTY's INHERIT_CURSOR mode waits for a cursor report before
                // starting output. Drive the same VT response loop as the GUI.
                for event in terminal.events() {
                    if let Event::PtyWrite(reply) = event
                        && let Some(session) = &session
                    {
                        session.write(reply.as_bytes()).unwrap();
                    }
                }
                if !input_sent
                    && terminal.text().contains("synthetic-ready")
                    && let Some(session) = &session
                {
                    session.write(b"mock-value\r").unwrap();
                    input_sent = true;
                }
            }
            Ok(PtyEvent::Error(error)) => panic!("{error}"),
            Ok(PtyEvent::Exited(_)) => {
                exited = true;
                session.take();
            }
            Ok(PtyEvent::Resized { .. } | PtyEvent::ReplayComplete) => {
                panic!("Direct PTYs must not emit session replay events")
            }
            Err(async_channel::TryRecvError::Closed) => break,
            Err(async_channel::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
        }
    }
    let output = terminal.text();
    let raw = String::from_utf8_lossy(&raw_output);
    assert!(input_sent, "ConPTY never became ready; output: {raw:?}");
    assert!(
        output.contains("synthetic:mock-value"),
        "Output: {output:?}; raw: {raw:?}"
    );
    assert!(exited, "The synthetic ConPTY command did not exit");
}
