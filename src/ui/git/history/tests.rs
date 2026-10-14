use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}

fn repository(root: &Path) -> Repository {
    git(root, &["init", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Synthetic Developer"),
        ("user.email", "synthetic@example.test"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".synthetic-hooks"),
    ] {
        git(root, &["config", key, value]);
    }
    fs::write(root.join("mock.txt"), "Synthetic base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "Synthetic base"]);
    git(root, &["switch", "-c", "feature/mock-history"]);
    fs::write(root.join("feature.txt"), "Synthetic feature\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "Synthetic feature"]);
    git(root, &["switch", "main"]);
    fs::write(root.join("main.txt"), "Synthetic main\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "Synthetic main"]);
    Repository::open(root).unwrap()
}

fn commit(hash: &str) -> tervia::git::Commit {
    tervia::git::Commit {
        hash: hash.into(),
        parents: String::new(),
        author: "Synthetic".into(),
        when: "2026-01-01T00:00:00Z".into(),
        message: "Synthetic commit".into(),
        refs: String::new(),
    }
}

#[test]
fn switching_history_scope_rejects_late_results_and_qualifies_references() {
    let local = Reference::Branch {
        name: "mock-ref".into(),
        remote: false,
    };
    let remote = Reference::Branch {
        name: "origin/mock-ref".into(),
        remote: true,
    };
    let tag = Reference::Tag("mock-ref".into());
    assert_eq!(local.qualified(), "refs/heads/mock-ref");
    assert_eq!(remote.qualified(), "refs/remotes/origin/mock-ref");
    assert_eq!(tag.qualified(), "refs/tags/mock-ref");
    let mut history = History::default();
    history.select(Some(local));
    let old = history.begin().unwrap();
    history.select(Some(tag));
    let current = history.begin().unwrap();
    assert!(!history.finish(old, Ok(vec![commit("mock-old")])));
    assert!(history.commits.is_empty());
    assert!(history.finish(current, Ok(vec![commit("mock-new")])));
    assert_eq!(history.commits[0].hash, "mock-new");
    assert_eq!(history.graph.rows.len(), 1);
    history.select(None);
    assert!(!history.finish(current, Err("Synthetic stale failure".into())));
    assert!(history.error.is_none());
}

#[gpui_test::test]
fn clicking_a_branch_browses_its_history_without_switching_and_all_history_restores(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    fs::write(root.path().join("mock.txt"), "Synthetic unsaved edit\n").unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let project = view.state.open_directory(repo.root.clone());
            view.init_project(project, window, cx);
            view.state.set_tab(Tab::Git);
            let snapshot = repo.snapshot(100).unwrap();
            let ui = &mut view.git_states[project];
            ui.repository = Some(repo.clone());
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(Arc::new(snapshot));
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let branch = cx.debug_bounds("git-sidebar-branch-0").unwrap();
    cx.simulate_click(branch.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-history-switch").is_some());
    view.update(cx, |view, _| {
        let ui = &view.git_states[0];
        assert!(ui.mode == Mode::History);
        assert_eq!(
            ui.history.target,
            Some(Reference::Branch {
                name: "feature/mock-history".into(),
                remote: false
            })
        );
        assert_eq!(
            ui.history
                .commits
                .iter()
                .map(|commit| commit.message.as_str())
                .collect::<Vec<_>>(),
            vec!["Synthetic feature", "Synthetic base"]
        );
        assert_eq!(ui.history.graph.rows.len(), 2);
    });
    assert_eq!(git(root.path(), &["branch", "--show-current"]), "main");
    assert_eq!(
        fs::read_to_string(root.path().join("mock.txt")).unwrap(),
        "Synthetic unsaved edit\n"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git_states[0].limit = 1;
            view.load_git_history(0, window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.git_states[0].history.commits.len(), 1)
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git_states[0].limit = 100;
            view.refresh_git(0, window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(
            view.git_states[0].history.commits[0].message,
            "Synthetic feature"
        );
        assert_eq!(view.git_states[0].history.commits.len(), 2);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_git_mode(0, Mode::History, window, cx)
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.git_states[0].history.target.is_none());
        assert!(
            view.git_states[0]
                .history
                .commits
                .iter()
                .any(|commit| commit.message == "Synthetic main")
        );
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_git_reference(
                0,
                Reference::Branch {
                    name: "feature/mock-history".into(),
                    remote: false,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    git(
        root.path(),
        &["update-ref", "-d", "refs/heads/feature/mock-history"],
    );
    cx.update(|window, cx| view.update(cx, |view, cx| view.refresh_git(0, window, cx)));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.git_states[0].history.target.is_none());
        assert_eq!(view.git_states[0].history.commits.len(), 2);
    });
}

#[gpui_test::test]
fn branch_creation_uses_a_small_form_and_preserves_history_search(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys([KeyBinding::new(
            "escape",
            gpui_kit::base::actions::Cancel,
            Some("Tervia"),
        )]);
    });
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let project = view.state.open_directory(repo.root.clone());
            view.init_project(project, window, cx);
            view.state.set_tab(Tab::Git);
            let snapshot = repo.snapshot(100).unwrap();
            let ui = &mut view.git_states[project];
            ui.repository = Some(repo.clone());
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(Arc::new(snapshot));
            view.select_git_mode(project, Mode::History, window, cx);
            view.git_states[project]
                .first
                .update(cx, |input, cx| input.set_value("Synthetic", window, cx));
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let plus = cx.debug_bounds("git-create-branch-trigger").unwrap();
    cx.simulate_click(plus.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-reference-form").is_some());
    cx.simulate_keystrokes("enter");
    view.update(cx, |view, cx| {
        assert!(view.git_states[0].reference_form.action.is_some());
        assert_eq!(cx.windows().len(), 1);
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.update(cx, |view, cx| {
        assert!(view.git_states[0].reference_form.action.is_none());
        assert_eq!(cx.windows().len(), 1);
    });
    cx.simulate_click(plus.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git_states[0]
                .reference_form
                .input
                .update(cx, |input, cx| {
                    input.set_value("feature/mock-created", window, cx)
                });
        })
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(
            view.git_states[0].first.read(cx).value().as_ref(),
            "Synthetic"
        );
        assert!(view.git_states[0].reference_form.action.is_none());
    });
    assert_eq!(git(root.path(), &["branch", "--show-current"]), "main");
    assert_eq!(
        git(root.path(), &["branch", "--list", "feature/mock-created"]),
        "feature/mock-created"
    );
}

#[gpui_test::test]
fn menu_branch_deletion_keeps_the_existing_confirmation_and_checkout(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    git(root.path(), &["branch", "mock-merged"]);
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let project = view.state.open_directory(repo.root.clone());
            view.init_project(project, window, cx);
            view.state.set_tab(Tab::Git);
            let snapshot = repo.snapshot(100).unwrap();
            let ui = &mut view.git_states[project];
            ui.repository = Some(repo.clone());
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(Arc::new(snapshot));
            view.run_git_action(
                project,
                "git_delete",
                Operation::DeleteBranch("mock-merged".into()),
                window,
                cx,
            );
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_cancel"));
    cx.run_until_parked();
    assert_eq!(
        git(root.path(), &["branch", "--list", "mock-merged"]),
        "mock-merged"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_git_action(
                0,
                "git_delete",
                Operation::DeleteBranch("mock-merged".into()),
                window,
                cx,
            );
        })
    });
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_confirm"));
    cx.run_until_parked();
    assert!(git(root.path(), &["branch", "--list", "mock-merged"]).is_empty());
    assert_eq!(git(root.path(), &["branch", "--show-current"]), "main");
}
