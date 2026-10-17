use super::*;
use core::prelude::v1::test;
use gpui_test::{TestAppContext, VisualTestContext};
use std::{fs, path::Path, process::Command};
use tervia::locale::Language;

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

fn fixture() -> (tempfile::TempDir, Repository) {
    let root = tempfile::tempdir().unwrap();
    let main = root.path().join("main");
    fs::create_dir(&main).unwrap();
    git(
        &main,
        &[
            "init",
            "-b",
            "feature/mock-primary-with-a-very-long-branch-name",
        ],
    );
    for (key, value) in [
        ("user.name", "Synthetic Developer"),
        ("user.email", "synthetic@example.test"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".synthetic-hooks"),
    ] {
        git(&main, &["config", key, value]);
    }
    git(
        &main,
        &["commit", "--allow-empty", "-m", "Synthetic initial commit"],
    );
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "feature/mock-linked",
            root.path().join("linked").to_str().unwrap(),
        ],
    );
    let repo = Repository::open(&main).unwrap();
    (root, repo)
}

fn mount<'a>(
    cx: &'a mut TestAppContext,
    repo: &Repository,
) -> (Entity<Tervia>, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let project = view.state.open_directory(repo.root.clone());
            view.init_project(project, window, cx);
            view.state.set_tab(Tab::Git);
            let snapshot = repo.snapshot(10).unwrap();
            let ui = &mut view.git_states[project];
            ui.repository = Some(repo.clone());
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(Arc::new(snapshot));
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    cx.simulate_resize(size(px(880.0), px(560.0)));
    (entity.unwrap(), cx)
}

fn open_menu(cx: &mut VisualTestContext, row: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds(row).expect("The worktree row is visible");
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
}

#[gpui_test::test]
fn primary_worktree_badge_stays_visible_beside_long_names_in_both_themes(cx: &mut TestAppContext) {
    let (_root, repo) = fixture();
    let (view, cx) = mount(cx, &repo);
    for language in [
        Language::Chinese,
        Language::English,
        Language::German,
        Language::Russian,
    ] {
        for light in [false, true] {
            view.update(cx, |view, cx| {
                view.preferences.update(cx, |preferences, cx| {
                    preferences.language = language;
                    preferences.light_theme = light;
                    cx.notify();
                })
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let row = cx.debug_bounds("git-sidebar-worktree-0").unwrap();
            let badge = cx
                .debug_bounds("git-worktree-main-0")
                .expect("Main-directory identity must be separate from the ellipsized branch");
            let name = cx.debug_bounds("git-worktree-name-0").unwrap();
            let current = cx.debug_bounds("git-worktree-current-0").unwrap();
            assert!(badge.left() >= name.right());
            assert!(badge.right() <= current.left());
            assert!(current.right() <= row.right());
            assert!(badge.top() >= row.top() && badge.bottom() <= row.bottom());
            assert!(
                name.size.width >= px(20.0),
                "The badge must leave room to identify the branch"
            );
        }
    }
}

#[gpui_test::test]
fn worktree_remove_action_rejects_protected_targets_before_confirmation(cx: &mut TestAppContext) {
    use tervia::git::WorktreeOperation;
    let (_root, repo) = fixture();
    let (view, cx) = mount(cx, &repo);
    let trees = repo.worktrees().unwrap();
    let linked = trees[1].clone();
    let protected = [
        trees[0].clone(),
        tervia::git::Worktree {
            main: false,
            ..trees[0].clone()
        },
        tervia::git::Worktree {
            main: true,
            ..linked.clone()
        },
        tervia::git::Worktree {
            bare: true,
            ..linked.clone()
        },
        tervia::git::Worktree {
            missing: true,
            ..linked.clone()
        },
        tervia::git::Worktree {
            locked: Some("Synthetic lock".into()),
            ..linked.clone()
        },
        tervia::git::Worktree {
            prunable: Some("Synthetic missing registration".into()),
            ..linked
        },
    ];
    for tree in protected {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.request_worktree_operation(
                    view.state.active_project,
                    WorktreeOperation::Remove(tree.clone()),
                    window,
                    cx,
                );
            })
        });
        assert!(
            !cx.has_pending_prompt(),
            "Protected target must not reach confirmation: {tree:?}"
        );
    }
}

#[gpui_test::test]
fn worktree_context_menu_removal_confirms_the_clicked_path_and_preserves_its_branch(
    cx: &mut TestAppContext,
) {
    let (root, repo) = fixture();
    let linked = root.path().join("linked").canonicalize().unwrap();
    let (view, cx) = mount(cx, &repo);
    cx.simulate_resize(size(px(880.0), px(800.0)));
    open_menu(cx, "git-sidebar-worktree-1");
    let (title, _) = cx
        .pending_prompt()
        .expect("Right-click removal must request confirmation");
    assert!(title.contains(linked.to_str().unwrap()));
    cx.update(|_, cx| assert_eq!(view.read(cx).state.project().root, repo.root));
    cx.simulate_prompt_answer(Language::System.text("git_cancel"));
    cx.run_until_parked();
    assert!(linked.exists());
    fs::write(linked.join("mock-edit.txt"), "Synthetic untracked edit\n").unwrap();
    open_menu(cx, "git-sidebar-worktree-1");
    cx.simulate_prompt_answer(Language::System.text("git_confirm"));
    cx.run_until_parked();
    assert!(
        linked.exists(),
        "A dirty worktree must survive context-menu removal"
    );
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.git_states[view.state.active_project].error.is_empty());
    });
    fs::remove_file(linked.join("mock-edit.txt")).unwrap();
    open_menu(cx, "git-sidebar-worktree-1");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer(Language::System.text("git_confirm"));
    cx.run_until_parked();
    assert!(!linked.exists());
    assert_eq!(
        git(&repo.root, &["branch", "--list", "feature/mock-linked"]),
        "feature/mock-linked"
    );
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.state.project().root, repo.root);
        assert!(view.git_states[view.state.active_project].error.is_empty());
    });
}

#[gpui_test::test]
fn worktree_context_menu_disables_removal_for_main_locked_and_busy_rows(cx: &mut TestAppContext) {
    let (_root, repo) = fixture();
    let (view, cx) = mount(cx, &repo);
    cx.simulate_resize(size(px(880.0), px(800.0)));
    open_menu(cx, "git-sidebar-worktree-0");
    assert!(!cx.has_pending_prompt());
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        view.git_states[view.state.active_project].busy = true
    });
    open_menu(cx, "git-sidebar-worktree-1");
    assert!(!cx.has_pending_prompt());
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        let ui = &mut view.git_states[view.state.active_project];
        ui.busy = false;
        Arc::make_mut(ui.snapshot.as_mut().unwrap()).worktrees[1].locked =
            Some("Synthetic lock".into());
    });
    open_menu(cx, "git-sidebar-worktree-1");
    assert!(!cx.has_pending_prompt());
    assert_eq!(repo.worktrees().unwrap().len(), 2);
}
