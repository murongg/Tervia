#![cfg(feature = "pty")]

use portable_pty::{CommandBuilder, PtySize};
use tervia::pty::{PtyEvent, PtySession, initial_directory};

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
fn a_synthetic_windows_command_round_trip_uses_conpty() {
    use alacritty_terminal::event::Event;
    use tervia::terminal::TerminalBuffer;

    let system_root = std::env::var_os("SystemRoot").expect("Windows must provide SystemRoot");
    let executable = std::path::PathBuf::from(&system_root)
        .join("System32")
        .join("cmd.exe");
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("roundtrip.cmd"),
        "@echo off\r\necho synthetic-ready\r\nset /p \"value=\"\r\necho synthetic:%value%\r\n",
    )
    .unwrap();
    // portable-pty resolves executables from the command's PATH. An isolated
    // environment needs an absolute executable and Windows' system directory.
    let mut command = CommandBuilder::new(executable);
    command.env_clear();
    command.env("SystemRoot", system_root);
    command.cwd(directory.path());
    // Keep shell syntax in the batch file instead of relying on CRT quoting for cmd /C.
    command.args(["/D", "/Q", "/C", "roundtrip.cmd"]);
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
