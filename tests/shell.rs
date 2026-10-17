use tervia::terminal::shell::{Event, Tracker};

fn hex(text: &str) -> String {
    text.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn marker(body: &str) -> Vec<u8> {
    format!("\x1b]777;tervia;mock-token;{body}\x07").into_bytes()
}

#[test]
fn shell_frames_survive_every_chunk_boundary_and_capture_only_the_command_output() {
    let cwd = if cfg!(windows) {
        "C:\\synthetic\\项目"
    } else {
        "/synthetic/项目"
    };
    let mut bytes = b"synthetic banner\r\n".to_vec();
    bytes.extend(marker(&format!("P;{}", hex(cwd))));
    bytes.extend_from_slice(b"mock> git status\r\n");
    bytes.extend(marker(&format!("C;{};{}", hex(cwd), hex("git status"))));
    bytes.extend_from_slice(b"\x1b[31msynthetic failure\x1b[0m\r\n");
    bytes.extend(marker("D;1"));
    bytes.extend(marker(&format!("P;{}", hex(cwd))));
    for split in 0..=bytes.len() {
        let mut tracker = Tracker::new("mock-token".into(), "zsh");
        let mut events = tracker.feed(&bytes[..split]);
        events.extend(tracker.feed(&bytes[split..]));
        let commands: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Completed(command) => Some(command),
                _ => None,
            })
            .collect();
        assert_eq!(commands.len(), 1, "split {split}");
        assert_eq!(commands[0].input, "git status");
        assert_eq!(commands[0].cwd.to_str(), Some(cwd));
        assert_eq!(commands[0].exit_code, 1);
        assert_eq!(commands[0].output, "synthetic failure");
        let visible: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Output(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect();
        assert_eq!(
            visible,
            b"synthetic banner\r\nmock> git status\r\n\x1b[31msynthetic failure\x1b[0m\r\n"
        );
    }
}

#[test]
fn unknown_and_oversized_escape_sequences_cannot_consume_normal_terminal_output() {
    let mut tracker = Tracker::new("mock-token".into(), "zsh");
    let unknown =
        b"\x1b]52;c;c3ludGhldGlj\x07\x1b[31mtext\x1b[0m\x1b]777;tervia;wrong-token;D;1\x07";
    let events = tracker.feed(unknown);
    let output: Vec<_> = events
        .into_iter()
        .filter_map(|event| match event {
            Event::Output(bytes) => Some(bytes),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(output, unknown);
    let mut huge = b"\x1b]".to_vec();
    huge.extend(vec![b'x'; 70_000]);
    huge.extend_from_slice(b"\x07after");
    let output: Vec<_> = tracker
        .feed(&huge)
        .into_iter()
        .filter_map(|event| match event {
            Event::Output(bytes) => Some(bytes),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(output, huge);
}

#[test]
fn shell_history_path_frames_are_private_chunk_safe_and_allow_disabling_imports() {
    let path = if cfg!(windows) {
        r"C:\synthetic\custom history.txt"
    } else {
        "/synthetic/custom history"
    };
    let bytes = marker(&format!("H;{}", hex(path)));
    for split in 0..=bytes.len() {
        let mut tracker = Tracker::new("mock-token".into(), "zsh");
        let mut events = tracker.feed(&bytes[..split]);
        events.extend(tracker.feed(&bytes[split..]));
        assert!(
            matches!(&events[..], [Event::History(Some(actual))] if actual == std::path::Path::new(path))
        );
        assert!(matches!(
            &tracker.feed(&marker("H;"))[..],
            [Event::History(None)]
        ));
        assert!(
            matches!(
                &tracker.feed(&marker("H;72656c6174697665"))[..],
                [Event::Output(_)]
            ),
            "Relative paths cannot be interpreted against the app's own cwd"
        );
    }
}
