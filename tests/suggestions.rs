use tervia::suggestions::{Command, History, Shell, Source, corrections};

fn command(input: &str, output: &str, exit_code: i32) -> Command {
    Command {
        input: input.into(),
        output: output.into(),
        cwd: if cfg!(windows) {
            r"C:\synthetic\project"
        } else {
            "/synthetic/project"
        }
        .into(),
        exit_code,
        shell: Shell::Zsh,
    }
}

#[test]
fn git_upstream_correction_uses_the_actual_remote_and_branch_from_output() {
    let last = command(
        "git push",
        "fatal: The current branch mock-feature has no upstream branch.\n    git push --set-upstream mock-origin mock-feature\n",
        128,
    );
    let suggestions = corrections(&last);
    assert_eq!(
        suggestions[0].command,
        "git push --set-upstream mock-origin mock-feature"
    );
    assert_eq!(suggestions[0].source, Source::Correction);
    assert!(corrections(&command("printf mock", &last.output, 1)).is_empty());
    assert!(corrections(&command("git push", &last.output, 0)).is_empty());
}

#[test]
fn corrections_quote_paths_and_never_accept_output_as_arbitrary_shell_code() {
    assert_eq!(
        corrections(&command(
            "mkdir 'mock parent/child'",
            "mkdir: mock parent/child: No such file or directory",
            1
        ))[0]
            .command,
        "mkdir -p 'mock parent/child'"
    );
    let bad = command(
        "git push",
        "fatal: no upstream branch\n git push --set-upstream mock-origin mock; touch /synthetic/injected\n",
        128,
    );
    assert!(corrections(&bad).is_empty());
    assert!(
        corrections(&command(
            "mkdir mock && printf mock",
            "No such file or directory",
            1
        ))
        .is_empty()
    );
}

#[test]
fn corrections_preserve_dry_run_flags_and_powershell_path_separators() {
    let last = command(
        "git push --dry-run",
        "fatal: The current branch mock-feature has no upstream branch.\n git push --set-upstream mock-origin mock-feature\n",
        128,
    );
    assert_eq!(
        corrections(&last)[0].command,
        "git push --set-upstream mock-origin mock-feature --dry-run"
    );
    let mut windows = command(
        r"gti -C C:\synthetic\project status",
        "gti is not recognized",
        1,
    );
    windows.shell = Shell::PowerShell;
    assert_eq!(
        corrections(&windows)[0].command,
        r"git -C 'C:\synthetic\project' status"
    );
}

#[test]
fn corrections_leave_shell_expansion_to_the_shell() {
    for input in [
        "mkdir ~/mock-parent/child",
        "mkdir mock-*/child",
        "mkdir mock-{a,b}/child",
    ] {
        assert!(
            corrections(&command(input, "No such file or directory", 1)).is_empty(),
            "Changed expansion semantics: {input}"
        );
    }
}

#[test]
fn common_typos_and_git_conflicts_have_scoped_corrections() {
    assert_eq!(
        corrections(&command("gti status", "zsh: command not found: gti", 127))[0].command,
        "git status"
    );
    assert_eq!(corrections(&command("git merge mock-branch", "CONFLICT (content): Merge conflict in mock.rs\nAutomatic merge failed; fix conflicts and then commit the result.", 1))[0].command, "git status");
    for exit in [0, 130, 141] {
        assert!(corrections(&command("gti status", "command not found: gti", exit)).is_empty());
    }
    assert!(corrections(&command("printf mock", "synthetic unrecognized failure", 1)).is_empty());
}

#[test]
fn history_learns_transitions_without_linking_different_sessions() {
    let mut history = History::default();
    let status = command(
        "git status",
        "synthetic output that must not be persisted",
        0,
    );
    let diff = command("git diff", "", 0);
    history.record("mock-a", &status);
    history.record("mock-b", &diff);
    assert!(history.predict(&status).is_none());
    history.record("mock-a", &diff);
    history.record("mock-c", &status);
    history.record("mock-c", &diff);
    assert_eq!(history.predict(&status).unwrap().command, "git diff");
    assert_eq!(history.predict(&status).unwrap().source, Source::History);
    let mut elsewhere = status.clone();
    elsewhere.cwd = "/synthetic/other".into();
    assert!(history.predict(&elsewhere).is_none());
    let mut failed = status.clone();
    failed.exit_code = 1;
    assert!(history.predict(&failed).is_none());
    let mut other_shell = status.clone();
    other_shell.shell = Shell::PowerShell;
    assert!(history.predict(&other_shell).is_none());
}

#[test]
fn history_learns_followup_commands_after_changing_directory() {
    let mut history = History::default();
    let change = command("cd mock-child", "", 0);
    let mut listing = command("ls", "", 0);
    listing.cwd.push("mock-child");
    for session in ["mock-a", "mock-b"] {
        history.record(session, &change);
        history.record(session, &listing);
    }
    assert_eq!(history.predict(&change).unwrap().command, "ls");
}

#[test]
fn history_round_trips_metadata_without_output_and_rejects_invalid_data() {
    let mut history = History::default();
    history.record(
        "mock-a",
        &command("git status", "synthetic-private-output", 0),
    );
    history.record("mock-a", &command("git diff", "", 0));
    history.record("mock-a", &command("git status", "", 0));
    history.record("mock-a", &command("git diff", "", 0));
    let serialized = history.encode().unwrap();
    assert!(!serialized.contains("synthetic-private-output"));
    let restored = History::decode(&serialized).unwrap();
    assert_eq!(
        restored
            .predict(&command("git status", "", 0))
            .unwrap()
            .command,
        "git diff"
    );
    assert!(History::decode("{invalid}").is_err());
    assert!(History::decode("{\"version\":999,\"entries\":[]}").is_err());
}

#[test]
fn history_does_not_learn_hidden_or_multiline_commands() {
    let mut history = History::default();
    let status = command("git status", "", 0);
    for session in ["mock-a", "mock-b"] {
        history.record(session, &status);
        history.record(session, &command(" mock-secret-command", "", 0));
        history.record(session, &command("printf mock\nprintf other", "", 0));
    }
    assert!(history.predict(&status).is_none());
    assert!(!history.encode().unwrap().contains("mock-secret-command"));
}

#[test]
fn store_keeps_history_private_and_preserves_a_damaged_file() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.json");
    let mut store = Store::new(Some(path.clone()));
    store
        .record(
            "mock-session",
            &command("git status", "synthetic-private-output", 0),
        )
        .unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("synthetic-private-output"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::write(&path, "synthetic damaged history").unwrap();
    let mut reopened = Store::new(Some(path.clone()));
    assert!(
        reopened
            .record("mock-other", &command("git diff", "", 0))
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "synthetic damaged history"
    );
}

#[cfg(feature = "pty")]
#[test]
fn background_history_worker_records_in_submission_order() {
    use tervia::suggestions::Worker;
    let worker = Worker::new(None);
    let mut replies = vec![];
    for input in [
        "git status",
        "git diff",
        "git status",
        "git diff",
        "git status",
    ] {
        replies.push(
            worker
                .submit("mock-session".into(), command(input, "", 0))
                .unwrap(),
        );
    }
    let results: Vec<_> = replies
        .into_iter()
        .map(|reply| reply.recv_blocking().unwrap().unwrap())
        .collect();
    assert_eq!(
        results.last().unwrap().as_ref().unwrap().command,
        "git diff"
    );
}

#[test]
fn prefix_completion_combines_ranked_history_with_common_commands() {
    let mut history = History::default();
    let context = command("git", "", 0);
    for input in ["git status", "git log --oneline", "git log --oneline"] {
        history.record("mock-session", &command(input, "", 0));
    }
    history.record("mock-session", &command("git broken", "", 1));
    let mut elsewhere = command("git elsewhere", "", 0);
    elsewhere.cwd.push("other");
    history.record("mock-session", &elsewhere);
    let results = history.complete("git", &context.cwd, context.shell);
    assert_eq!(results[0].command, "git log --oneline");
    assert_eq!(results[0].source, Source::History);
    assert_eq!(results[1].command, "git status");
    assert_eq!(
        results.iter().filter(|s| s.command == "git status").count(),
        1
    );
    assert!(results.iter().any(|s| s.command == "git stash"));
    assert!(results.iter().any(|s| s.command == "git commit"));
    assert!(
        !results
            .iter()
            .any(|s| s.command.contains("broken") || s.command.contains("elsewhere"))
    );
    assert!(results.len() <= 6);
    let filtered = history.complete("git sta", &context.cwd, context.shell);
    assert_eq!(
        filtered
            .iter()
            .map(|s| s.command.as_str())
            .collect::<Vec<_>>(),
        ["git status", "git stash"]
    );
    for prefix in ["", "g", " git", "git\n", "git status"] {
        assert!(
            !history
                .complete(prefix, &context.cwd, context.shell)
                .iter()
                .any(|s| s.command == prefix)
        );
    }
    assert!(
        history
            .complete(" git", &context.cwd, context.shell)
            .is_empty()
    );
    assert!(
        history
            .complete("git log", &context.cwd, Shell::Fish)
            .iter()
            .all(|s| s.source != Source::History)
    );
    assert_eq!(
        History::decode(&history.encode().unwrap())
            .unwrap()
            .complete("git l", &context.cwd, context.shell)[0]
            .command,
        "git log --oneline"
    );
}

#[test]
fn prefix_input_tracks_only_known_shell_edits_and_protects_typeahead() {
    use tervia::suggestions::Input;
    let mut input = Input::default();
    input.prompt();
    input.send(b"git");
    assert_eq!(input.prefix(), Some("git"));
    input.send(b"\x7f");
    assert_eq!(input.prefix(), Some("gi"));
    input.send(b"\x1b[200~t sta\x1b[201~");
    assert_eq!(input.prefix(), Some("git sta"));
    input.send(b"\x1b[D");
    input.send(b"x");
    assert_eq!(
        input.prefix(),
        None,
        "Unknown cursor edits must never append a guessed suffix"
    );
    input.send(b"\r");
    input.started();
    input.send(b"git");
    input.prompt();
    assert_eq!(
        input.prefix(),
        None,
        "Typed-ahead text is already waiting in the shell"
    );
    input.send(b"\r");
    input.started();
    input.prompt();
    input.send(b"git");
    assert_eq!(input.prefix(), Some("git"));
    input.send(b"\x15");
    assert_eq!(input.prefix(), None);
    input.send(b" git");
    assert_eq!(
        input.prefix(),
        None,
        "Leading spaces opt out of suggestions"
    );
}

#[cfg(feature = "pty")]
#[test]
fn prefix_worker_queries_persisted_history_before_a_new_command_finishes() {
    use tervia::suggestions::{Store, Worker};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.json");
    let context = command("git mock-history", "", 0);
    Store::new(Some(path.clone()))
        .record("mock-session", &context)
        .unwrap();
    let worker = Worker::new(Some(path));
    let results = worker
        .complete("git mo".into(), context.cwd, context.shell)
        .unwrap()
        .recv_blocking()
        .unwrap()
        .unwrap();
    assert_eq!(results[0].command, "git mock-history");
}

#[test]
fn existing_shell_files_supply_completion_without_inventing_directory_or_exit_metadata() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("custom-history");
    let local = root.path().join("tervia-history.json");
    let context = command("git", "", 0);
    std::fs::write(&source, ": 1700000000:0;git mock-global\n: 1700000001:0;git mock-global\ngit status\n git hidden\ngit multiline \\\ngit continuation\ngit recent\n").unwrap();
    let before = std::fs::read(&source).unwrap();
    let mut store = Store::new(Some(local.clone()));
    store
        .record("mock-session", &command("git mock-local", "", 0))
        .unwrap();
    let results = store
        .complete_from("git", &context.cwd, Shell::Zsh, Some(&source))
        .unwrap();
    assert_eq!(results[0].command, "git mock-local");
    assert_eq!(results[1].command, "git mock-global");
    assert_eq!(
        results.iter().filter(|s| s.command == "git status").count(),
        1
    );
    assert!(
        !results
            .iter()
            .any(|s| s.command.contains("continuation") || s.command.contains("hidden"))
    );
    let elsewhere = store
        .complete_from(
            "git mock",
            &root.path().join("elsewhere"),
            Shell::Zsh,
            Some(&source),
        )
        .unwrap();
    assert_eq!(elsewhere[0].command, "git mock-global");
    assert!(!elsewhere.iter().any(|s| s.command == "git mock-local"));
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert!(
        !std::fs::read_to_string(local)
            .unwrap()
            .contains("mock-global")
    );
    std::fs::write(&source, "git newly-saved\n").unwrap();
    assert_eq!(
        store
            .complete_from("git new", &context.cwd, Shell::Zsh, Some(&source))
            .unwrap()[0]
            .command,
        "git newly-saved"
    );
    assert!(
        store
            .complete_from("git new", &context.cwd, Shell::Zsh, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn shell_history_formats_keep_single_line_commands_and_skip_record_metadata() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("history");
    let context = command("git", "", 0);
    for (shell, contents, expected) in [
        (
            Shell::Bash,
            "#1700000000\ngit bash-old\n#1700000001\nprintf 'mock\ngit not-a-command'\n#1700000002\ngit bash-new\n",
            vec!["git bash-new", "git bash-old"],
        ),
        (
            Shell::Fish,
            "- cmd: git fish-old\n  when: 1700000000\n  paths:\n    - git not-a-command\n- cmd: git fish-new\\\\path\n  when: 1700000001\n- cmd: printf mock\\ngit not-a-command\n  when: 1700000002\n",
            vec!["git fish-new\\path", "git fish-old"],
        ),
        (
            Shell::PowerShell,
            "\u{feff}git ps-old\r\ngit multi`\r\ngit not-a-command\r\ngit ps-new\r\n",
            vec!["git ps-new", "git ps-old"],
        ),
    ] {
        std::fs::write(&source, contents).unwrap();
        let actual = Store::new(None)
            .complete_from("git ", &context.cwd, shell, Some(&source))
            .unwrap();
        assert_eq!(
            actual
                .into_iter()
                .filter(|s| s.source == Source::History)
                .map(|s| s.command)
                .collect::<Vec<_>>(),
            expected,
            "{shell:?}"
        );
    }
}

#[test]
fn shell_import_bounds_large_files_and_rejects_partial_or_invalid_commands() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history");
    let context = command("git", "", 0);
    let mut large = b"git outside-window\n".to_vec();
    large.extend(vec![b'x'; 2 * 1024 * 1024]);
    large.extend_from_slice(b"\ngit latest-tail\ngit partial");
    std::fs::write(&path, &large).unwrap();
    let mut store = Store::new(None);
    let results = store
        .complete_from("git ", &context.cwd, Shell::Zsh, Some(&path))
        .unwrap();
    assert_eq!(results[0].command, "git latest-tail");
    assert!(
        !results
            .iter()
            .any(|s| s.command.contains("outside-window") || s.command == "git partial")
    );
    let bounded = (0..2105)
        .map(|index| format!("git synthetic-{index}\n"))
        .collect::<String>();
    std::fs::write(&path, bounded).unwrap();
    assert!(
        store
            .complete_from("git synthetic-0", &context.cwd, Shell::Zsh, Some(&path))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .complete_from("git synthetic-210", &context.cwd, Shell::Zsh, Some(&path))
            .unwrap()[0]
            .command,
        "git synthetic-2104"
    );
    std::fs::write(&path, b"git \xffbroken\ngit valid\n").unwrap();
    assert_eq!(
        store
            .complete_from("git ", &context.cwd, Shell::Zsh, Some(&path))
            .unwrap()[0]
            .command,
        "git valid"
    );
    assert!(
        store
            .complete_from("git", &context.cwd, Shell::Zsh, Some(root.path()))
            .is_err()
    );
    assert!(
        store
            .complete_from(
                "git missing",
                &context.cwd,
                Shell::Zsh,
                Some(&root.path().join("missing"))
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn timestamped_bash_history_never_turns_empty_leading_lines_into_standalone_commands() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history");
    let context = command("git", "", 0);
    std::fs::write(&path, "#1700000000\n\ngit multiline-fragment\n#1700000001\ngit complete\n#1700000002\ngit unfinished\npartial").unwrap();
    let results = Store::new(None)
        .complete_from("git ", &context.cwd, Shell::Bash, Some(&path))
        .unwrap();
    assert_eq!(
        results
            .iter()
            .filter(|s| s.source == Source::History)
            .map(|s| s.command.as_str())
            .collect::<Vec<_>>(),
        ["git complete"]
    );
}

#[test]
fn invalid_shell_history_records_keep_their_raw_continuation_boundaries() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history");
    let context = command("git", "", 0);
    for (shell, contents) in [
        (
            Shell::Zsh,
            b"printf \xff \\\ngit synthetic-fragment\ngit valid\n".as_slice(),
        ),
        (
            Shell::Bash,
            b"printf \xff \\\ngit synthetic-fragment\ngit valid\n".as_slice(),
        ),
        (
            Shell::PowerShell,
            b"Write-Host \xff `\ngit synthetic-fragment\ngit valid\n".as_slice(),
        ),
    ] {
        std::fs::write(&path, contents).unwrap();
        let results = Store::new(None)
            .complete_from("git ", &context.cwd, shell, Some(&path))
            .unwrap();
        assert_eq!(
            results
                .iter()
                .filter(|s| s.source == Source::History)
                .map(|s| s.command.as_str())
                .collect::<Vec<_>>(),
            ["git valid"],
            "{shell:?}"
        );
    }
}

#[test]
fn plain_bash_literal_backslashes_do_not_hide_the_following_history_record() {
    use tervia::suggestions::Store;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history");
    let context = command("git", "", 0);
    let literal = format!("git synthetic-literal {}", "\\".repeat(2));
    std::fs::write(&path, format!("{literal}\ngit following-record\n")).unwrap();
    let results = Store::new(None)
        .complete_from("git ", &context.cwd, Shell::Bash, Some(&path))
        .unwrap();
    assert_eq!(
        results
            .iter()
            .filter(|s| s.source == Source::History)
            .map(|s| s.command.as_str())
            .collect::<Vec<_>>(),
        ["git following-record", literal.as_str()]
    );
}

#[cfg(feature = "pty")]
#[test]
fn shared_worker_keeps_each_terminals_shell_history_source_separate() {
    use tervia::suggestions::Worker;
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("history-a");
    let second = root.path().join("history-b");
    std::fs::write(&first, "git source-a\n").unwrap();
    std::fs::write(&second, "git source-b\n").unwrap();
    let worker = Worker::new(None);
    let context = command("git", "", 0);
    for (source, expected) in [
        (Some(first.clone()), Some("git source-a")),
        (Some(second), Some("git source-b")),
        (None, None),
        (Some(first), Some("git source-a")),
    ] {
        let results = worker
            .complete_from("git source".into(), context.cwd.clone(), Shell::Zsh, source)
            .unwrap()
            .recv_blocking()
            .unwrap()
            .unwrap();
        assert_eq!(results.first().map(|s| s.command.as_str()), expected);
    }
}
