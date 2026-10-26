#![cfg(all(feature = "pty", windows))]

use portable_pty::PtySize;
use std::time::{Duration, Instant};
use tervia::{pty::PtyEvent, sessions::Target, terminal::TerminalBuffer};

struct Cleanup(Target);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.0.terminate();
    }
}

#[test]
fn custom_startup_arguments_reach_the_windows_terminal_host() {
    let root = tempfile::tempdir().unwrap();
    let target = Target::new(
        root.path().join("sessions"),
        "c".repeat(32),
        env!("CARGO_BIN_EXE_tervia-session").into(),
    )
    .with_shell(tervia::shell::Profile::Cmd.resolve())
    .with_shell_args(vec![
        "/D".into(),
        "/Q".into(),
        "/K".into(),
        "set TERVIA_MOCK_ARGUMENT=synthetic value".into(),
    ]);
    let _cleanup = Cleanup(target.clone());
    let (session, events, tracker) = target
        .open(
            root.path(),
            PtySize {
                rows: 12,
                cols: 90,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .unwrap();
    assert!(tracker.is_none());
    session
        .write(b"echo synthetic-startup-%TERVIA_MOCK_ARGUMENT%\r\n")
        .unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "Output: {}", buffer.text());
        match events.try_recv() {
            Ok(PtyEvent::Data(bytes)) => buffer.feed(&bytes),
            Ok(PtyEvent::Exited(error) | PtyEvent::Error(error)) => panic!("{error}"),
            Err(async_channel::TryRecvError::Closed) => panic!("Terminal disconnected"),
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
        if buffer
            .text()
            .lines()
            .any(|line| line.trim() == "synthetic-startup-synthetic value")
        {
            break;
        }
    }
    session.terminate().unwrap();
}

#[test]
fn cmd_starts_in_a_canonical_project_directory_with_spaces_unicode_and_metacharacters() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("synthetic 项目 & project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("synthetic-marker.txt"),
        "mock working directory",
    )
    .unwrap();
    let project = project.canonicalize().unwrap();
    let target = Target::new(
        root.path().join("sessions"),
        "b".repeat(32),
        env!("CARGO_BIN_EXE_tervia-session").into(),
    )
    .with_shell("cmd.exe")
    .with_env("PROMPT", "synthetic> ");
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target
        .open(
            &project,
            PtySize {
                rows: 24,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .unwrap();
    session.write(b"if exist synthetic-marker.txt (echo synthetic-cwd-ok) else (echo synthetic-cwd-wrong)\r\n").unwrap();
    let mut buffer = TerminalBuffer::new(120, 24);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "Output: {}", buffer.text());
        match events.try_recv() {
            Ok(PtyEvent::Data(bytes)) => buffer.feed(&bytes),
            Ok(PtyEvent::Exited(error) | PtyEvent::Error(error)) => panic!("{error}"),
            Err(async_channel::TryRecvError::Closed) => panic!("Terminal disconnected"),
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
        // Match an output line, not either branch echoed as part of the command input.
        let text = buffer.text();
        if text.lines().any(|line| line.trim() == "synthetic-cwd-ok") {
            break;
        }
        assert!(
            !text
                .lines()
                .any(|line| line.trim() == "synthetic-cwd-wrong"),
            "CMD started outside its project: {text}"
        );
    }
    session.terminate().unwrap();
}

#[test]
fn selected_shell_reaches_the_host_and_reconnect_preserves_it() {
    let root = tempfile::tempdir().unwrap();
    let target = Target::new(
        root.path().join("sessions"),
        "a".repeat(32),
        env!("CARGO_BIN_EXE_tervia-session").into(),
    )
    .with_env("SHELL", "synthetic-missing-shell.exe")
    .with_env("TERVIA_MOCK_VALUE", "selected-shell")
    .with_shell("cmd.exe");
    let _cleanup = Cleanup(target.clone());
    let (session, events, tracker) = target
        .open(
            root.path(),
            PtySize {
                rows: 12,
                cols: 90,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .unwrap();
    assert_eq!(session.shell(), "cmd.exe");
    assert!(tracker.is_none());
    let pid = session.process_id();
    session
        .write(b"echo synthetic-%TERVIA_MOCK_VALUE%\r\n")
        .unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !buffer.text().contains("synthetic-selected-shell") {
        assert!(Instant::now() < deadline, "Output: {}", buffer.text());
        match events.try_recv() {
            Ok(PtyEvent::Data(bytes)) => buffer.feed(&bytes),
            Ok(PtyEvent::Exited(error) | PtyEvent::Error(error)) => panic!("{error}"),
            Err(async_channel::TryRecvError::Closed) => panic!("Terminal disconnected"),
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    drop(session);
    drop(events);
    let (session, _, _) = target
        .with_shell("synthetic-other-shell.exe")
        .reconnect()
        .unwrap();
    assert_eq!(session.process_id(), pid);
    assert_eq!(session.shell(), "cmd.exe");
    session.terminate().unwrap();
}
