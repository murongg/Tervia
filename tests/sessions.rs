#![cfg(all(feature = "pty", unix))]

use portable_pty::PtySize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tervia::{pty::PtyEvent, sessions::Target, terminal::TerminalBuffer};

fn target(root: &tempfile::TempDir, id: char) -> Target {
    std::fs::write(
        root.path().join(".bash_profile"),
        "PS1='mock> '\nHISTFILE=/dev/null\n",
    )
    .unwrap();
    Target::new(
        root.path().join("sessions"),
        id.to_string().repeat(32),
        PathBuf::from(env!("CARGO_BIN_EXE_tervia-session")),
    )
    .with_env("SHELL", "/bin/bash")
    .with_env("HOME", root.path())
    .with_env("ENV", "/dev/null")
    .with_env("BASH_ENV", "/dev/null")
    .with_env("BASH_SILENCE_DEPRECATION_WARNING", "1")
    .with_env("PROMPT_COMMAND", "")
}

struct Cleanup(Target);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.0.terminate();
    }
}

#[test]
fn task_runner_hosts_keep_the_selected_file_and_replay_without_rerunning() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Makefile"), "mock-make:\n\t@echo mock\n").unwrap();
    std::fs::write(root.path().join("justfile"), "mock-just:\n  echo mock\n").unwrap();
    std::fs::write(
        root.path().join("Taskfile.yml"),
        "version: '3'\ntasks:\n  'mock-task;touch unexpected': echo mock\n",
    )
    .unwrap();
    let tasks = tervia::tasks::discover(root.path()).tasks;
    assert_eq!(tasks.len(), 3);
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for program in ["make", "just", "task"] {
        let path = bin.join(program);
        std::fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > args-{program}\nprintf 'run\\n' >> runs-{program}\nprintf 'mock-runner-output\\n'\nexit 7\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for (index, task) in tasks.iter().enumerate() {
        let target = target(&root, ['b', 'c', 'd'][index])
            .with_env("MOCK_TASK_BIN", &bin)
            .with_task(task.clone());
        std::fs::write(
            root.path().join(".bash_profile"),
            "export PATH=\"$MOCK_TASK_BIN:/usr/bin:/bin\"\n",
        )
        .unwrap();
        let _cleanup = Cleanup(target.clone());
        let (session, events, _) = target.open(root.path(), size()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut exit = None;
        while Instant::now() < deadline && exit.is_none() {
            match events.try_recv() {
                Ok(PtyEvent::Exited(status)) => exit = Some(status),
                Ok(PtyEvent::Error(error)) => panic!("{error}"),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        assert_eq!(exit.as_deref(), Some("Exited with code 7"));
        drop(session);
        drop(events);
        let (_session, events, _) = target.reconnect().unwrap();
        let mut buffer = TerminalBuffer::new(90, 12);
        output(&events, &mut buffer, "mock-runner-output");
        assert_eq!(
            std::fs::read_to_string(root.path().join(format!("args-{}", task.program))).unwrap(),
            format!("{}\n", task.args.join("\n"))
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(format!("runs-{}", task.program))).unwrap(),
            "run\n"
        );
    }
    assert!(!root.path().join("unexpected").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn hosted_programs_leave_the_gui_launchers_resource_coalition() {
    fn coalition(pid: u32) -> u64 {
        // Darwin's PROC_PIDCOALITIONINFO contains two coalition IDs and three reserved u64s.
        let mut info = [0u64; 5];
        let count = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                20,
                0,
                info.as_mut_ptr().cast(),
                std::mem::size_of_val(&info) as i32,
            )
        };
        assert_eq!(count as usize, std::mem::size_of_val(&info));
        info[0]
    }
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, '5');
    let _cleanup = Cleanup(target.clone());
    let (session, _events, _) = target.open(root.path(), size()).unwrap();
    assert_ne!(
        coalition(session.process_id().unwrap()),
        coalition(std::process::id()),
        "setsid alone leaves foreground programs subject to macOS app-quit cleanup"
    );
    session.terminate().unwrap();
}

#[test]
fn detaching_client_entry() {
    let Some(root) = std::env::var_os("TERVIA_MOCK_CLIENT_ROOT") else {
        return;
    };
    let target = Target::new(PathBuf::from(root), "9".repeat(32), PathBuf::new());
    let (session, _events, _) = target.reconnect().unwrap();
    session.write(&vec![b'm'; 32768]).unwrap();
    session.flush().unwrap();
    // Exiting the client must not rely on Rust background threads finishing during process teardown.
    std::process::exit(0);
}

#[test]
fn accepted_input_is_flushed_before_the_client_process_exits() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, '9');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    let pid = session.process_id();
    let mut before = TerminalBuffer::new(90, 12);
    session.write(b"stty -echo -icanon min 1 time 0; printf '__RAW_%s__\\n' READY; IFS= read -r -n 32768 MOCK_RAW; printf %s \"$MOCK_RAW\" > mock-input; stty sane; printf '\\nRAW_%s\\n' DONE\r").unwrap();
    output(&events, &mut before, "__RAW_READY__");
    drop(session);
    drop(events);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "detaching_client_entry"])
        .env("TERVIA_MOCK_CLIENT_ROOT", root.path().join("sessions"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let (session, events, _) = target.reconnect().unwrap();
    assert_eq!(session.process_id(), pid);
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "RAW_DONE");
    assert_eq!(
        std::fs::read(root.path().join("mock-input")).unwrap(),
        vec![b'm'; 32768]
    );
    session.terminate().unwrap();
}

fn size() -> PtySize {
    PtySize {
        rows: 12,
        cols: 90,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[test]
fn project_task_reconnect_replays_output_and_exit_without_executing_again() {
    use tervia::tasks::Task;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = bin.join("npm");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf 'mock-task-output\\n'\nprintf '%s\\n' \"$2\" >> runs\nexit 7\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let target = target(&root, 'a')
        .with_env("MOCK_TASK_BIN", &bin)
        .with_task(Task {
            label: "mock 'quoted' $(touch unexpected)".into(),
            source: "package.json".into(),
            program: "npm".into(),
            args: vec!["run".into(), "mock 'quoted' $(touch unexpected)".into()],
        });
    std::fs::write(
        root.path().join(".bash_profile"),
        "export PATH=\"$MOCK_TASK_BIN:/usr/bin:/bin\"\nprintf 'mock-login-profile\\n'\n",
    )
    .unwrap();
    let _cleanup = Cleanup(target.clone());
    let (session, events, tracker) = target.open(root.path(), size()).unwrap();
    assert!(tracker.is_none());
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut buffer = TerminalBuffer::new(90, 12);
    let mut exit = None;
    while Instant::now() < deadline && exit.is_none() {
        match events.try_recv() {
            Ok(PtyEvent::Data(bytes)) => buffer.feed(&bytes),
            Ok(PtyEvent::Exited(status)) => exit = Some(status),
            Ok(PtyEvent::Error(error)) => panic!("{error}"),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    assert_eq!(
        exit.as_deref(),
        Some("Exited with code 7"),
        "{}",
        buffer.text()
    );
    assert!(buffer.text().contains("mock-login-profile"));
    assert!(buffer.text().contains("mock-task-output"));
    drop(session);
    drop(events);
    let (_session, events, tracker) = target.reconnect().unwrap();
    assert!(tracker.is_none());
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "mock-task-output");
    assert_eq!(
        std::fs::read_to_string(root.path().join("runs")).unwrap(),
        "mock 'quoted' $(touch unexpected)\n"
    );
    assert!(!root.path().join("unexpected").exists());
}

fn output(receiver: &async_channel::Receiver<PtyEvent>, buffer: &mut TerminalBuffer, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut tail = Vec::new();
    while Instant::now() < deadline {
        match receiver.try_recv() {
            Ok(PtyEvent::Data(bytes)) => {
                buffer.feed(&bytes);
                tail.extend(bytes);
                if tail.len() > 65536 {
                    tail.drain(..tail.len() - 32768);
                }
            }
            Ok(PtyEvent::Resized { columns, rows }) => buffer.resize(columns, rows),
            Ok(PtyEvent::ReplayComplete) => {}
            Ok(PtyEvent::Error(error)) => panic!("{error}"),
            Ok(PtyEvent::Exited(status)) if !buffer.text().contains(text) => {
                panic!("{status}: {}", buffer.text())
            }
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
        if tail
            .windows(text.len())
            .any(|bytes| bytes == text.as_bytes())
        {
            assert!(buffer.text().contains(text));
            return;
        }
    }
    panic!("missing {text:?}: {}", buffer.visible_text());
}

#[test]
fn detach_and_reopen_keep_the_same_shell_environment_and_absent_output() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, 'a');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    let pid = session.process_id().unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    session
        .write(b"stty -echo; printf '__MOCK_%s__\\n' READY\r")
        .unwrap();
    output(&events, &mut buffer, "__MOCK_READY__");
    session.write(b"export MOCK_VALUE=synthetic-state; mkdir -p mock-dir; cd mock-dir; printf '\\033[31mBEFORE-DETACH\\033[0m\\n'; sleep 1; printf 'WHILE-DETACHED\\n'\r").unwrap();
    output(&events, &mut buffer, "BEFORE-DETACH");
    drop(session);
    drop(events);
    std::thread::sleep(Duration::from_millis(1200));
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    assert_eq!(session.process_id(), Some(pid));
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "WHILE-DETACHED");
    session
        .write(b"printf '\\nVALUE=%s DIR=%s PID=%s\\n' \"$MOCK_VALUE\" \"${PWD##*/}\" \"$$\"\r")
        .unwrap();
    output(
        &events,
        &mut restored,
        &format!("VALUE=synthetic-state DIR=mock-dir PID={pid}"),
    );
    session.terminate().unwrap();
    assert!(restored.text().contains("BEFORE-DETACH"));
}

#[test]
fn replay_retains_resize_alternate_screen_and_pending_escape_sequences() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, 'b');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    let mut before = TerminalBuffer::new(90, 12);
    session
        .write(b"stty -echo; printf '__MOCK_%s__\\n' READY\r")
        .unwrap();
    output(&events, &mut before, "__MOCK_READY__");
    session.write(b"printf '\\033[?1049h\\033[?1h\\033[2J\\033[HMOCK-TUI'; printf '\\033['; sleep 1; printf '2;1HCONTINUED'; sleep 30\r").unwrap();
    output(&events, &mut before, "MOCK-TUI");
    session.resize(70, 18, 8, 16).unwrap();
    drop(session);
    drop(events);
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "CONTINUED");
    assert_eq!(restored.size(), (70, 18));
    use alacritty_terminal::term::TermMode;
    assert!(
        restored
            .term
            .mode()
            .contains(TermMode::ALT_SCREEN | TermMode::APP_CURSOR)
    );
    session.write(b"\x03").unwrap();
    session
        .write(b"printf '\\033[?1049l\\nAFTER-TUI\\n'\r")
        .unwrap();
    output(&events, &mut restored, "AFTER-TUI");
    session.terminate().unwrap();
}

#[test]
fn stale_connections_do_not_silently_launch_a_replacement_shell() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, 'c');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    session.terminate().unwrap();
    drop(session);
    drop(events);
    let deadline = Instant::now() + Duration::from_secs(5);
    while target.directory().exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!target.directory().exists());
    assert!(target.reconnect().is_err());
}

#[test]
fn executable_replacement_does_not_interrupt_a_detached_session() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("mock-installed-app");
    std::fs::copy(env!("CARGO_BIN_EXE_tervia-session"), &executable).unwrap();
    let target = Target::new(
        root.path().join("sessions"),
        "d".repeat(32),
        executable.clone(),
    )
    .with_env("SHELL", "/bin/sh")
    .with_env("HOME", root.path())
    .with_env("ENV", "/dev/null");
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    let pid = session.process_id();
    session
        .write(b"stty -echo; printf 'MOCK-%s\\n' OLD\r")
        .unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    output(&events, &mut buffer, "MOCK-OLD");
    drop(session);
    drop(events);
    std::fs::rename(&executable, root.path().join("mock-old-app")).unwrap();
    std::fs::write(
        &executable,
        b"synthetic replacement executable; reconnection must not spawn it",
    )
    .unwrap();
    let (session, events, _) = target.reconnect().unwrap();
    assert_eq!(session.process_id(), pid);
    session.write(b"printf 'MOCK-%s\\n' UPDATED\r").unwrap();
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "MOCK-UPDATED");
    assert!(restored.text().contains("MOCK-OLD"));
    session.terminate().unwrap();
}

#[test]
fn ended_shell_retains_its_final_output_and_exit_status_for_reopen() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, 'e');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    assert!(!target.has_exited());
    session
        .write(b"head -c 2097152 /dev/zero | tr '\\000' x; printf '\\nFINAL-%s\\n' MOCK; exit 7\r")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    while !target.directory().join("finished.json").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(target.has_exited());
    let saved = std::fs::read(target.directory().join("output")).unwrap();
    assert!(
        saved
            .windows(b"FINAL-MOCK".len())
            .any(|bytes| bytes == b"FINAL-MOCK"),
        "Final output must precede the saved exit record"
    );
    drop(session);
    drop(events);
    let (_session, events, _) = target.reconnect().unwrap();
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "FINAL-MOCK");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match events.try_recv() {
            Ok(PtyEvent::Exited(status)) => {
                assert!(status.contains('7'));
                break;
            }
            _ if Instant::now() >= deadline => panic!("Missing saved exit status"),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

#[test]
fn detached_host_answers_terminal_queries_and_rejects_unauthenticated_input() {
    use std::io::{BufRead, BufReader, Write};
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, 'f');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    session
        .write(b"stty -echo; printf 'MOCK-%s\\n' READY\r")
        .unwrap();
    output(&events, &mut buffer, "MOCK-READY");
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.directory().join("endpoint.json")).unwrap())
            .unwrap();
    let stream =
        std::net::TcpStream::connect(("127.0.0.1", meta["port"].as_u64().unwrap() as u16)).unwrap();
    let mut reader = BufReader::new(stream);
    reader
        .get_mut()
        .write_all(b"{\"version\":1,\"token\":\"invalid\",\"op\":\"input\",\"bytes\":[3]}\n")
        .unwrap();
    let mut reply = String::new();
    reader.read_line(&mut reply).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reply).unwrap()["ok"],
        false
    );
    session.write(b"stty -echo -icanon min 1 time 0; printf '\\033[6n'; IFS= read -r -d R mock_reply; printf '\\nQUERY-%s\\n' ANSWERED; stty sane\r").unwrap();
    drop(session);
    drop(events);
    std::thread::sleep(Duration::from_millis(200));
    let (session, events, _) = target.reconnect().unwrap();
    let mut restored = TerminalBuffer::new(90, 12);
    output(&events, &mut restored, "QUERY-ANSWERED");
    session.terminate().unwrap();
}

#[test]
fn changing_theme_updates_color_queries_in_the_existing_host() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, '7');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    session.colors(vec![[0x12, 0x34, 0x56]; 269]).unwrap();
    session.flush().unwrap();
    session.write(b"stty -echo -icanon min 1 time 0; printf '\\033]10;?\\007'; IFS= read -r -d $'\\007' MOCK_REPLY; printf '\\nCOLOR=%s\\n' \"${MOCK_REPLY##*rgb:}\"; stty sane\r").unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    output(&events, &mut buffer, "COLOR=1212/3434/5656");
    session.terminate().unwrap();
}

#[test]
fn detached_host_preserves_pixel_metrics_for_terminal_size_queries() {
    let root = tempfile::tempdir().unwrap();
    let target = target(&root, '6');
    let _cleanup = Cleanup(target.clone());
    let (session, events, _) = target.open(root.path(), size()).unwrap();
    session.resize(70, 18, 8, 16).unwrap();
    session.flush().unwrap();
    session.write(b"stty -echo -icanon min 1 time 0; printf '\\033[14t'; IFS= read -r -d t MOCK_REPLY; printf '\\nPIXELS=%s\\n' \"${MOCK_REPLY#*\\[}\"; stty sane\r").unwrap();
    let mut buffer = TerminalBuffer::new(90, 12);
    output(&events, &mut buffer, "PIXELS=4;288;560");
    session.terminate().unwrap();
}
