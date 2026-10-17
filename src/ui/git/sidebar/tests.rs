use super::*;
use core::prelude::v1::test;
use gpui_kit::component::menu::PopupMenuItem;
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

fn show_menu(cx: &mut VisualTestContext, row: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds(row).expect("The worktree row is visible");
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn open_menu(cx: &mut VisualTestContext, row: &'static str) {
    show_menu(cx, row);
    cx.simulate_keystrokes("up enter");
    cx.run_until_parked();
}

fn menu_item(
    view: &Entity<Tervia>,
    cx: &mut VisualTestContext,
    index: usize,
    key: &'static str,
) -> PopupMenuItem {
    cx.update(|_, cx| {
        let this = view.read(cx);
        let project = this.state.active_project;
        let tree = &this.git_states[project]
            .snapshot
            .as_ref()
            .unwrap()
            .worktrees[index];
        this.worktree_menu_items(project, tree, view.downgrade()).into_iter().find(|item| {
            matches!(item, PopupMenuItem::Item { label, .. } if label.as_ref() == this.t(key))
        }).unwrap_or_else(|| panic!("Missing worktree menu item: {key}"))
    })
}

fn item_disabled(item: &PopupMenuItem) -> bool {
    match item {
        PopupMenuItem::Item { disabled, .. } => *disabled,
        _ => panic!("Expected a menu action"),
    }
}

fn click_menu(view: &Entity<Tervia>, cx: &mut VisualTestContext, index: usize, key: &'static str) {
    match menu_item(view, cx, index, key) {
        PopupMenuItem::Item {
            disabled: false,
            handler: Some(handler),
            ..
        } => {
            cx.update(|window, cx| handler(&ClickEvent::default(), window, cx));
            cx.run_until_parked();
        }
        _ => panic!("Menu action is disabled or has no handler: {key}"),
    }
}

#[gpui_test::test]
fn sidebar_width_drags_with_limits_and_survives_navigation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            for root in ["/synthetic/sidebar-one", "/synthetic/sidebar-two"] {
                let project = this.state.open_directory(root.into());
                this.init_project(project, window, cx);
                this.state.set_tab(Tab::Git);
                this.git_states[project].snapshot = Some(Arc::new(Snapshot::default()));
            }
            this.state.active_project = 0;
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.simulate_resize(size(px(1400.0), px(800.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();

    fn width(cx: &mut VisualTestContext) -> Pixels {
        cx.debug_bounds("git-sidebar-section-git_worktrees")
            .unwrap()
            .size
            .width
    }

    fn drag(cx: &mut VisualTestContext, desired: f32) {
        let sidebar = cx
            .debug_bounds("git-sidebar-section-git_worktrees")
            .unwrap();
        let start = point(sidebar.right(), sidebar.center().y);
        let end = point(sidebar.left() + px(desired), start.y);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    let initial = width(cx);
    drag(cx, 280.0);
    assert!(
        (width(cx) - px(280.0)).abs() <= px(1.0),
        "Dragging the Git sidebar divider changes its width: initial={initial:?}, after={:?}",
        width(cx)
    );
    for mode in [
        Mode::History,
        Mode::Stash,
        Mode::Remotes,
        Mode::Worktrees,
        Mode::Changes,
    ] {
        view.update(cx, |this, cx| {
            this.git_states[0].mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!((width(cx) - px(280.0)).abs() <= px(1.0));
    }
    view.update(cx, |this, cx| {
        this.git_states[0].review.focused = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-sidebar-section-git_worktrees")
            .is_none()
    );
    view.update(cx, |this, cx| {
        this.git_states[0].review.focused = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!((width(cx) - px(280.0)).abs() <= px(1.0));

    view.update(cx, |this, cx| {
        this.state.active_project = 1;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        (width(cx) - px(174.0)).abs() <= px(1.0),
        "Other projects start at the default width: other={:?}",
        width(cx)
    );
    drag(cx, 220.0);
    view.update(cx, |this, cx| {
        this.state.active_project = 0;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!((width(cx) - px(280.0)).abs() <= px(1.0));

    drag(cx, 60.0);
    assert!((width(cx) - px(174.0)).abs() <= px(1.0));
    drag(cx, 560.0);
    assert!((width(cx) - px(400.0)).abs() <= px(1.0));
    cx.simulate_resize(size(px(880.0), px(560.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let sidebar = cx
        .debug_bounds("git-sidebar-section-git_worktrees")
        .unwrap();
    let changes = cx.debug_bounds("git-changes-pane").unwrap();
    let viewer = cx.debug_bounds("git-diff-viewer").unwrap();
    assert!((px(174.0)..=px(400.0)).contains(&sidebar.size.width));
    assert!(changes.left() >= sidebar.right());
    assert!(changes.size.width >= px(200.0));
    assert!(viewer.size.width >= px(220.0));
    assert!(viewer.right() <= px(880.0));
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
    show_menu(cx, "git-sidebar-worktree-0");
    assert!(item_disabled(&menu_item(&view, cx, 0, "worktree_remove")));
    assert!(!cx.has_pending_prompt());
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        view.git_states[view.state.active_project].busy = true
    });
    show_menu(cx, "git-sidebar-worktree-1");
    assert!(item_disabled(&menu_item(&view, cx, 1, "worktree_remove")));
    assert!(!cx.has_pending_prompt());
    cx.simulate_keystrokes("escape");
    view.update(cx, |view, _| {
        let ui = &mut view.git_states[view.state.active_project];
        ui.busy = false;
        Arc::make_mut(ui.snapshot.as_mut().unwrap()).worktrees[1].locked =
            Some("Synthetic lock".into());
    });
    show_menu(cx, "git-sidebar-worktree-1");
    assert!(item_disabled(&menu_item(&view, cx, 1, "worktree_remove")));
    assert!(!cx.has_pending_prompt());
    assert_eq!(repo.worktrees().unwrap().len(), 2);
}

#[gpui_test::test]
fn worktree_menu_copies_the_clicked_directory_without_changing_the_active_one(
    cx: &mut TestAppContext,
) {
    let (root, repo) = fixture();
    let linked = root.path().join("linked").canonicalize().unwrap();
    let (view, cx) = mount(cx, &repo);
    for key in [
        "worktree_open",
        "worktree_copy_path",
        "project_reveal",
        "worktree_lock",
        "worktree_remove",
    ] {
        assert!(!item_disabled(&menu_item(&view, cx, 1, key)));
    }
    click_menu(&view, cx, 1, "worktree_copy_path");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        linked.to_string_lossy()
    );
    cx.update(|_, cx| assert_eq!(view.read(cx).state.project().root, repo.root));
    assert!(item_disabled(&menu_item(&view, cx, 0, "worktree_open")));
    assert!(item_disabled(&menu_item(&view, cx, 0, "worktree_remove")));
    assert!(!item_disabled(&menu_item(
        &view,
        cx,
        0,
        "worktree_copy_path"
    )));
    assert!(!item_disabled(&menu_item(&view, cx, 0, "project_reveal")));
}

#[gpui_test::test]
fn worktree_menu_opens_the_clicked_worktree(cx: &mut TestAppContext) {
    let (root, repo) = fixture();
    let linked = root.path().join("linked").canonicalize().unwrap();
    let (view, cx) = mount(cx, &repo);
    click_menu(&view, cx, 1, "worktree_open");
    cx.update(|_, cx| assert_eq!(view.read(cx).state.project().root, linked));
}

#[gpui_test::test]
fn worktree_menu_locks_and_unlocks_the_clicked_worktree(cx: &mut TestAppContext) {
    let (_root, repo) = fixture();
    let (view, cx) = mount(cx, &repo);
    click_menu(&view, cx, 1, "worktree_lock");
    assert!(repo.worktrees().unwrap()[1].locked.is_some());
    assert!(item_disabled(&menu_item(&view, cx, 1, "worktree_remove")));
    click_menu(&view, cx, 1, "worktree_unlock");
    assert!(repo.worktrees().unwrap()[1].locked.is_none());
    cx.update(|_, cx| assert_eq!(view.read(cx).state.project().root, repo.root));
}
