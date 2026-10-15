use super::{Detail, FileStatus, Mode, Side, Snapshot, Tervia};
use core::prelude::v1::test;
use gpui_kit::AppContext as _;
use gpui_test::TestAppContext;
use std::{fs, path::Path};

fn synthetic_commit(hash: &str, parents: &str) -> tervia::git::Commit {
    tervia::git::Commit {
        hash: hash.into(),
        parents: parents.into(),
        author: "Synthetic Developer".into(),
        when: "2026-01-01T00:00:00Z".into(),
        message: "Synthetic commit".into(),
        refs: String::new(),
    }
}

fn review_patch(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n synthetic first\n-synthetic old\n+synthetic new\n synthetic third\n@@ -30,2 +30,2 @@\n-synthetic old end\n+synthetic new end\n synthetic last\n"
    )
}

#[gpui_test::test]
fn review_progress_invalidates_changed_files_and_checkpoint_compares_the_next_round(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mock.rs"), "synthetic first\n").unwrap();
    let repo = tervia::git::Repository::initialize(root.path()).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory(root.path().canonicalize().unwrap());
            this.init_project(project, window, cx);
            let snapshot = repo.snapshot(10).unwrap();
            let file = snapshot.status.files[0].clone();
            let ui = &mut this.git_states[project];
            ui.repository = Some(repo.clone());
            ui.snapshot = Some(std::sync::Arc::new(snapshot));
            let generation = ui.begin_detail(Detail::File(file.clone(), Side::Worktree), cx);
            ui.install_detail(
                generation,
                Ok(repo.diff(&file, Side::Worktree).unwrap()),
                cx,
            );
            this.toggle_reviewed(project, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |this, cx| this.capture_checkpoint(0, window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            view.read(cx).git_states[0]
                .review
                .marks
                .contains(&tervia::git::ReviewKey::new(
                    "mock.rs".into(),
                    Side::Worktree
                ))
        );
        assert!(view.read(cx).git_states[0].review.checkpoint.is_some());
    });
    fs::write(root.path().join("mock.rs"), "synthetic second\n").unwrap();
    cx.update(|window, cx| view.update(cx, |this, cx| this.refresh_git(0, window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert!(
                !this.git_states[0]
                    .review
                    .marks
                    .contains(&tervia::git::ReviewKey::new(
                        "mock.rs".into(),
                        Side::Worktree
                    ))
            );
            this.show_checkpoint(0, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let ui = &view.read(cx).git_states[0];
        assert!(ui.detail.contains("-synthetic first"));
        assert!(ui.detail.contains("+synthetic second"));
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("git-checkpoint-view").is_some());
}

#[gpui_test::test]
fn code_context_diff_excerpts_keep_versions_renamed_paths_and_sparse_line_numbers(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::diff::{DiffFile, DiffLineRange, DiffSide, DiffState};
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        let patch = "diff --git a/old.rs b/new.rs\n--- a/old.rs\n+++ b/new.rs\n@@ -2,2 +2,2 @@\n-synthetic old\n+synthetic new\n same\n@@ -30 +30 @@\n-last old\n+last new\n";
        let diff = cx.new(|cx| DiffState::new(DiffFile::parse(patch).unwrap(), cx));
        diff.update(cx, |state, cx| state.set_selected_lines(Some(DiffLineRange::new("new.rs", DiffSide::Original, 2, 30)), cx));
        let parts = diff.read(cx).selected_excerpts(cx);
        assert_eq!(parts.len(), 2, "Missing patch context must not be invented");
        assert_eq!((parts[0].path.as_ref(), parts[0].start, parts[0].end, parts[0].text.as_str()), ("old.rs", 2, 3, "synthetic old\nsame\n"));
        assert_eq!((parts[1].start, parts[1].end), (30, 30));
        diff.update(cx, |state, cx| state.set_selected_lines(Some(DiffLineRange::new("new.rs", DiffSide::Original, 2, 2).with_end_side(DiffSide::Modified)), cx));
        let parts = diff.read(cx).selected_excerpts(cx);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].side, DiffSide::Original);
        assert_eq!(parts[0].text, "synthetic old\n");
        assert_eq!(parts[1].side, DiffSide::Modified);
        assert_eq!(parts[1].text, "synthetic new\n");
    });
}

#[gpui_test::test]
fn code_context_diff_toolbar_copies_selection_after_focus_changes(cx: &mut TestAppContext) {
    use gpui_kit::component::diff::{DiffLineRange, DiffMode, DiffSide};
    use gpui_kit::{Focusable as _, Modifiers, MouseButton, point, px};
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/context-diff".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let ui = &mut this.git_states[project];
            ui.snapshot = Some(std::sync::Arc::new(Snapshot::default()));
            let generation = ui.begin_detail(Detail::Commit("mock-hash".into()), cx);
            ui.install_detail(generation, Ok(review_patch("mock.rs")), cx);
            ui.diff.update(cx, |state, cx| {
                state.set_selected_lines(
                    Some(DiffLineRange::new("mock.rs", DiffSide::Modified, 2, 2)),
                    cx,
                )
            });
        });
        window.draw(cx).clear(cx);
    });
    let button = cx
        .debug_bounds("git-copy-code-context")
        .expect("Context copying is discoverable beside the diff");
    cx.update(|_, cx| {
        assert!(
            view.read(cx).git_states[0].code_context(cx).is_some(),
            "The rendered diff still owns its selection before clicking"
        )
    });
    cx.simulate_mouse_down(
        button.center(),
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    cx.update(|_, cx| {
        assert!(
            view.read(cx).git_states[0].review.copying.is_some(),
            "The copy gesture keeps its source until release"
        )
    });
    cx.simulate_mouse_up(
        button.center(),
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "mock.rs:2 (modified)\n```\nsynthetic new\n```"
        )
    });
    for mode in [DiffMode::Unified, DiffMode::Split] {
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                let ui = &mut this.git_states[0];
                ui.review.focused = true;
                ui.diff.update(cx, |state, cx| {
                    state.set_mode(mode, cx);
                    state.set_selected_lines(None, cx);
                });
                ui.diff.read(cx).focus_handle(cx).focus(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        let source = cx.debug_bounds("diff-source-0-Modified-2").unwrap();
        let start = source.origin + point(px(0.1), source.size.height * 0.5);
        let end = point(source.right() - px(0.1), start.y);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            assert_eq!(
                view.read(cx).git_states[0].diff.read(cx).selected_text(cx),
                "synthetic new"
            );
            window.draw(cx).clear(cx);
        });
        let button = cx.debug_bounds("git-copy-code-context").unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "mock.rs:2 (modified)\n```\nsynthetic new\n```"
            )
        });
    }
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
            "synthetic sentinel".into(),
        ));
        view.update(cx, |this, cx| {
            this.git_states[0].diff.update(cx, |state, cx| {
                state.set_selected_lines(
                    Some(DiffLineRange::new("mock.rs", DiffSide::Original, 2, 2)),
                    cx,
                )
            });
        });
        window.draw(cx).clear(cx);
    });
    let button = cx.debug_bounds("git-copy-code-context").unwrap();
    cx.simulate_mouse_down(button.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(
        point(px(1.0), px(1.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|_, cx| {
        assert!(view.read(cx).git_states[0].review.copying.is_none());
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "synthetic sentinel"
        );
    });
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            let ui = &mut this.git_states[0];
            ui.diff.update(cx, |state, cx| {
                state.set_selected_lines(
                    Some(DiffLineRange::new("mock.rs", DiffSide::Original, 2, 2)),
                    cx,
                )
            });
            ui.mode = Mode::Remotes;
            this.copy_code_context(cx);
        });
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "synthetic sentinel",
            "A hidden Diff must not supply context while managing remotes"
        );
    });
}

#[test]
fn hunk_lines_identify_modified_ranges_and_use_original_lines_for_deletions() {
    use gpui_kit::component::diff::DiffSide;

    for (header, expected) in [
        (
            "@@ -9,6 +9,7 @@ synthetic scope",
            Some((DiffSide::Modified, 9, 15)),
        ),
        (
            "@@ -188,12 +189,13 @@",
            Some((DiffSide::Modified, 189, 201)),
        ),
        ("@@ -2 +3 @@", Some((DiffSide::Modified, 3, 3))),
        ("@@ -0,0 +1,4 @@", Some((DiffSide::Modified, 1, 4))),
        ("@@ -30,2 +29,0 @@", Some((DiffSide::Original, 30, 31))),
        ("@@ -1 +0,0 @@", Some((DiffSide::Original, 1, 1))),
        ("@@ -0,0 +0,0 @@", None),
        ("@@ -1,2 +mock,2 @@", None),
        ("@@ -1,2 +1,2", None),
        ("synthetic metadata", None),
    ] {
        assert_eq!(super::viewer::hunk_lines(header), expected, "{header}");
    }
}

#[gpui_test::test]
fn review_hunk_actions_appear_with_their_sections_in_both_diff_modes(cx: &mut TestAppContext) {
    use gpui_kit::component::diff::DiffMode;

    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/hunk-review".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let file = FileStatus {
                path: "sample.txt".into(),
                original: None,
                index: 'M',
                worktree: 'M',
                conflicted: false,
            };
            let mut snapshot = Snapshot::default();
            snapshot.status.files.push(file.clone());
            let ui = &mut this.git_states[project];
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(std::sync::Arc::new(snapshot));
        })
    });
    for (side, mode) in [
        (Side::Worktree, DiffMode::Unified),
        (Side::Worktree, DiffMode::Split),
        (Side::Index, DiffMode::Unified),
        (Side::Index, DiffMode::Split),
    ] {
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                let ui = &mut this.git_states[0];
                let file = ui.snapshot.as_ref().unwrap().status.files[0].clone();
                let generation = ui.begin_detail(Detail::File(file, side), cx);
                ui.install_detail(
                    generation,
                    Ok(format!(
                        "{}@@ -60,2 +59,0 @@\n-synthetic deleted first\n-synthetic deleted last\n",
                        review_patch("sample.txt")
                    )),
                    cx,
                );
                ui.diff.update(cx, |state, cx| state.set_mode(mode, cx));
            });
            window.draw(cx).clear(cx);
        });
        let first = cx
            .debug_bounds("git-hunk-0")
            .expect("First section has its own action");
        let second = cx
            .debug_bounds("git-hunk-1")
            .expect("Second section has its own action");
        assert!(
            first.size.width <= gpui_kit::px(32.0),
            "Hunk actions are compact icon buttons"
        );
        let header = cx
            .debug_bounds("diff-hunk-header-0-0")
            .expect("The section has a header");
        assert!(
            header.contains(&first.center()),
            "The action belongs inside its @@ header"
        );
        assert!(
            first.origin.x > header.center().x,
            "The action is on the header's right"
        );
        assert!(
            second.origin.y > first.origin.y + first.size.height,
            "Hunk actions follow their code sections rather than sharing a toolbar"
        );
        let deleted = cx
            .debug_bounds("git-hunk-2")
            .expect("A deleted section has its own action");
        assert!(deleted.origin.y > second.origin.y + second.size.height);
    }
}

#[gpui_test::test]
fn review_working_copy_navigation_reveals_folded_files(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let nested_path = Path::new("nested").join("second.txt");
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("first.txt"), "synthetic first\n").unwrap();
    fs::write(root.path().join(&nested_path), "synthetic second\n").unwrap();
    let repo = tervia::git::Repository::initialize(root.path()).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| view.update(cx, |this, cx| {
        let project = this.state.open_directory(root.path().canonicalize().unwrap());
        this.init_project(project, window, cx);
        this.state.set_tab(tervia::state::Tab::Git);
        let snapshot = repo.snapshot(10).unwrap();
        let first = snapshot.status.files.iter().find(|file| file.path == nested_path).unwrap().clone();
        let ui = &mut this.git_states[project];
        ui.repository = Some(repo);
        ui.collapsed_changes.insert(1);
        ui.collapsed_changes.insert(3);
        ui.collapsed_folders.insert((3, "nested".into()));
        ui.reconcile_snapshot(&snapshot, cx);
        ui.snapshot = Some(std::sync::Arc::new(snapshot));
        let generation = ui.begin_detail(Detail::File(first, Side::Worktree), cx);
        ui.install_detail(generation, Ok(review_patch("nested/second.txt")), cx);
        this.navigate_git_file(project, true, window, cx);
        let ui = &this.git_states[project];
        assert!(matches!(&ui.selected, Some(Detail::File(file, Side::Worktree)) if file.path == Path::new("first.txt")));
        assert!(!ui.collapsed_changes.contains(&3), "Navigation reveals the selected untracked file in its list");
        assert!(ui.collapsed_changes.contains(&1), "Navigation preserves unrelated collapsed groups");
        this.navigate_git_file(project, false, window, cx);
        let ui = &this.git_states[project];
        assert!(matches!(&ui.selected, Some(Detail::File(file, Side::Worktree)) if file.path == nested_path));
        assert!(!ui.collapsed_folders.contains(&(3, "nested".into())), "Navigation reveals the destination's ancestor folder");
    }));
}

#[gpui_test::test]
fn review_returning_to_a_file_reuses_its_view_only_while_the_patch_is_unchanged(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::diff::{DiffLineRange, DiffSide};
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory("/synthetic/review-position".into());
            this.init_project(project, window, cx);
            let detail = |path: &str| {
                Detail::File(
                    FileStatus {
                        path: path.into(),
                        original: None,
                        index: '.',
                        worktree: 'M',
                        conflicted: false,
                    },
                    Side::Worktree,
                )
            };
            let ui = &mut this.git_states[project];
            let first = ui.begin_detail(detail("first.txt"), cx);
            ui.install_detail(first, Ok(review_patch("first.txt")), cx);
            let saved = ui.diff.clone();
            ui.diff.update(cx, |state, cx| {
                state.set_selected_lines(
                    Some(DiffLineRange::new("first.txt", DiffSide::Modified, 30, 30)),
                    cx,
                )
            });
            let second = ui.begin_detail(detail("second.txt"), cx);
            ui.install_detail(second, Ok(review_patch("second.txt")), cx);
            let back = ui.begin_detail(detail("first.txt"), cx);
            ui.install_detail(second, Ok(review_patch("second.txt")), cx);
            ui.install_detail(back, Ok(review_patch("first.txt")), cx);
            assert_eq!(
                ui.diff, saved,
                "The same viewer retains exact scroll, selection and expansion state"
            );
            assert_eq!(
                ui.diff.read(cx).selected_lines(),
                Some(DiffLineRange::new("first.txt", DiffSide::Modified, 30, 30))
            );
            let changed = ui.begin_detail(detail("first.txt"), cx);
            ui.install_detail(
                changed,
                Ok(review_patch("first.txt").replace("synthetic new", "synthetic changed")),
                cx,
            );
            assert!(
                ui.diff.read(cx).selected_lines().is_none(),
                "A changed patch must not keep an obsolete selection"
            );
        })
    });
}

#[gpui_test::test]
fn review_commit_form_starts_collapsed_retains_drafts_and_diff_can_fill_the_workspace(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/review".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let file = FileStatus {
                path: "mock.txt".into(),
                original: None,
                index: 'M',
                worktree: '.',
                conflicted: false,
            };
            let mut snapshot = Snapshot::default();
            snapshot.status.files.push(file.clone());
            let ui = &mut this.git_states[project];
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(std::sync::Arc::new(snapshot));
            let generation = ui.begin_detail(Detail::File(file, Side::Index), cx);
            ui.install_detail(generation, Ok(review_patch("mock.txt")), cx);
            ui.message.update(cx, |input, cx| {
                input.set_value("Synthetic draft", window, cx)
            });
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-commit-form").is_none(),
        "Reviewing does not show a commit form by default"
    );
    let toggle = cx
        .debug_bounds("git-toggle-commit")
        .expect("Commit remains discoverable");
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-commit-form").is_some());
    let toggle = cx.debug_bounds("git-toggle-commit").unwrap();
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-commit-form").is_none());
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(
                this.git_states[0].message.read(cx).value().as_str(),
                "Synthetic draft"
            );
        })
    });
    let before = cx.debug_bounds("git-diff-viewer").unwrap();
    let focus = cx.debug_bounds("git-focus-diff").unwrap();
    cx.simulate_click(focus.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-diff-viewer").unwrap().size.width > before.size.width);
    let focus = cx.debug_bounds("git-focus-diff").unwrap();
    cx.simulate_click(focus.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("git-diff-viewer").unwrap().size.width,
        before.size.width
    );
}

#[gpui_test::test]
fn review_file_navigation_tracks_commit_files_and_exposes_change_navigation(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory("/synthetic/review-history".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let ui = &mut this.git_states[project];
            ui.mode = Mode::History;
            ui.snapshot = Some(std::sync::Arc::new(Snapshot::default()));
            let generation = ui.begin_detail(Detail::Commit("mock-commit".into()), cx);
            ui.install_detail(
                generation,
                Ok(format!(
                    "commit mock-commit\n\n{}{}",
                    review_patch("first.txt"),
                    review_patch("second.txt")
                )),
                cx,
            );
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let next = cx
        .debug_bounds("git-next-file")
        .expect("Next file is available in the viewer");
    cx.simulate_click(next.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(
                this.git_states[0].commit_file.as_ref().unwrap().as_str(),
                "second.txt"
            );
        })
    });
    let previous = cx.debug_bounds("git-previous-file").unwrap();
    cx.simulate_click(previous.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(
                this.git_states[0].commit_file.as_ref().unwrap().as_str(),
                "first.txt"
            );
        })
    });
    assert!(cx.debug_bounds("git-next-change").is_some());
    assert!(cx.debug_bounds("git-previous-change").is_some());
}

#[gpui_test::test]
fn review_open_full_file_and_return_keeps_the_diff_and_selection(cx: &mut TestAppContext) {
    use gpui_kit::component::diff::{DiffLineRange, DiffSide};
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("mock.txt"),
        "synthetic first\nsynthetic new\nsynthetic third\n",
    )
    .unwrap();
    let repo = tervia::git::Repository::initialize(root.path()).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    let mut original_diff = None;
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory(root.path().canonicalize().unwrap());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let snapshot = repo.snapshot(10).unwrap();
            let file = snapshot.status.files[0].clone();
            let patch = repo.diff(&file, Side::Worktree).unwrap();
            let ui = &mut this.git_states[project];
            ui.repository = Some(repo.clone());
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(std::sync::Arc::new(snapshot));
            let generation = ui.begin_detail(Detail::File(file, Side::Worktree), cx);
            ui.install_detail(generation, Ok(patch), cx);
            ui.diff.update(cx, |state, cx| {
                state.set_selected_lines(
                    Some(DiffLineRange::new("mock.txt", DiffSide::Modified, 2, 2)),
                    cx,
                )
            });
            original_diff = Some(ui.diff.clone());
            assert_eq!(
                ui.diff.read(cx).selected_lines(),
                Some(DiffLineRange::new("mock.txt", DiffSide::Modified, 2, 2)),
                "The synthetic diff contains the selected source line"
            );
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(
                this.git_states[0].diff.read(cx).selected_lines(),
                Some(DiffLineRange::new("mock.txt", DiffSide::Modified, 2, 2)),
                "Drawing retains the source selection before opening the file"
            );
        })
    });
    let open = cx
        .debug_bounds("git-open-full-file")
        .expect("Full file can be opened from a diff");
    cx.simulate_click(open.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.project().tab, tervia::state::Tab::Files);
            assert_eq!(this.editors[0][0].read(cx).cursor_position().line, 1);
        })
    });
    let back = cx
        .debug_bounds("git-return-to-diff")
        .expect("Opening a full file provides a return path");
    cx.simulate_click(back.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.project().tab, tervia::state::Tab::Git);
            assert_eq!(this.git_states[0].diff, original_diff.clone().unwrap());
            assert_eq!(
                this.git_states[0].diff.read(cx).selected_lines(),
                Some(DiffLineRange::new("mock.txt", DiffSide::Modified, 2, 2))
            );
        })
    });
}

#[gpui_test::test]
fn git_probes_treat_plain_folders_as_empty_states_and_preserve_real_errors(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    for (broken, initialize, expect_error) in [
        (false, false, false),
        (true, false, true),
        (true, true, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        if broken {
            fs::write(root.path().join(".git"), "synthetic invalid git file\n").unwrap();
        }
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut this = Tervia::new(window, cx);
                let project = this
                    .state
                    .open_directory(root.path().canonicalize().unwrap());
                this.init_project(project, window, cx);
                this.state.set_tab(tervia::state::Tab::Git);
                this.git_job(project, None, initialize, window, cx);
                this
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        cx.run_until_parked();
        view.unwrap().update(cx, |this, _| {
            let ui = &this.git_states[this.state.active_project];
            assert!(!ui.busy, "Git detection completed");
            assert!(ui.repository.is_none());
            assert!(ui.snapshot.is_none());
            assert_eq!(
                !ui.error.is_empty(),
                expect_error,
                "Only ordinary non-repository detection is silent: {}",
                ui.error.details().unwrap_or_default()
            );
            assert!(!this.git_mutating);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("git-empty-heading").is_some(),
            !expect_error
        );
        assert_eq!(cx.debug_bounds("git-init").is_some(), !expect_error);
        assert_eq!(cx.debug_bounds("git-clone").is_some(), !expect_error);
        assert_eq!(cx.debug_bounds("git-retry").is_some(), expect_error);
    }
}

#[gpui_test::test]
fn git_probe_failure_offers_retry_and_recovers_after_metadata_repair(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this
                .state
                .open_directory(root.path().canonicalize().unwrap());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            this.refresh_git(project, window, cx);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.update(cx, |this, _| {
        assert!(
            !this.git_states[0].error.is_empty(),
            "Empty Git metadata is a failure"
        )
    });
    assert!(
        cx.debug_bounds("git-init").is_none(),
        "Do not initialize over damaged metadata"
    );
    assert!(
        cx.debug_bounds("git-retry").is_some(),
        "A failed probe offers retry"
    );
    let toggle = cx.debug_bounds("git-toggle-error-details").unwrap();
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let retry = cx.debug_bounds("git-retry").unwrap();
    cx.simulate_click(retry.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-error-details").is_some(),
        "Repeating the same failed probe preserves disclosure"
    );
    fs::remove_dir(root.path().join(".git")).unwrap();
    tervia::git::Repository::initialize(root.path()).unwrap();
    let retry = cx.debug_bounds("git-retry").unwrap();
    cx.simulate_click(retry.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert!(this.git_states[0].snapshot.is_some());
        assert!(
            this.git_states[0].error.is_empty(),
            "Successful retry clears the recovered probe failure"
        );
    });
}

#[gpui_test::test]
fn git_error_controls_disclose_and_copy_full_diagnostics_in_both_states(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for loaded in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let repo = tervia::git::Repository::initialize(root.path()).unwrap();
        let failure = repo
            .execute(tervia::git::Operation::Switch(
                "synthetic-missing-branch".into(),
            ))
            .unwrap_err();
        let details = failure.details();
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut this = Tervia::new(window, cx);
                let project = this
                    .state
                    .open_directory(root.path().canonicalize().unwrap());
                this.init_project(project, window, cx);
                this.state.set_tab(tervia::state::Tab::Git);
                let ui = &mut this.git_states[project];
                ui.repository = Some(repo.clone());
                if loaded {
                    ui.snapshot = Some(std::sync::Arc::new(repo.snapshot(10).unwrap()));
                }
                ui.error.action(failure.clone());
                this
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("git-error-details").is_none(),
            "Diagnostics begin collapsed"
        );
        let copy = cx
            .debug_bounds("git-copy-error")
            .expect("Copy exists in every error state");
        cx.simulate_click(copy.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            details,
            "Copy retains directory, status and stderr"
        );
        let toggle = cx
            .debug_bounds("git-toggle-error-details")
            .expect("Diagnostics can be expanded");
        cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx
            .debug_bounds("git-error-details")
            .expect("Expanded diagnostic is visible");
        assert!(
            bounds.size.height <= gpui_kit::px(160.0),
            "Diagnostic height stays bounded"
        );
        let changed = repo
            .execute(tervia::git::Operation::Switch(
                "synthetic-different-missing-branch".into(),
            ))
            .unwrap_err();
        view.update(cx, |this, cx| {
            this.git_states[0].error.action(changed);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("git-error-details").is_none(),
            "A changed diagnostic starts collapsed"
        );
        let toggle = cx.debug_bounds("git-toggle-error-details").unwrap();
        cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("git-error-details").is_some());
        let toggle = cx.debug_bounds("git-toggle-error-details").unwrap();
        cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("git-error-details").is_none(),
            "Hide removes the diagnostic"
        );
        view.update(cx, |_, _| {});
    }
}

#[gpui_test::test]
fn git_action_failure_survives_successful_passive_refresh(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    tervia::git::Repository::initialize(root.path()).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this
                .state
                .open_directory(root.path().canonicalize().unwrap());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            this.git_job(
                project,
                Some(tervia::git::Operation::Switch(
                    "synthetic-missing-branch".into(),
                )),
                false,
                window,
                cx,
            );
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.run_until_parked();
    let mut diagnostic = String::new();
    view.update(cx, |this, _| {
        assert!(
            this.git_states[0].snapshot.is_some(),
            "A failed action still reconciles status"
        );
        assert!(
            !this.git_states[0].error.is_empty(),
            "The failed action is reported"
        );
        diagnostic = this.git_states[0]
            .error
            .details()
            .unwrap_or_default()
            .to_owned();
    });
    cx.update(|window, cx| view.update(cx, |this, cx| this.refresh_git(0, window, cx)));
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(
            this.git_states[0]
                .error
                .details()
                .unwrap_or_default()
                .to_owned(),
            diagnostic,
            "Polling cannot erase the failed user action"
        )
    });
}

#[gpui_test::test]
fn commit_file_navigation_selects_the_target_and_full_metadata_can_be_expanded(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/commit-files".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let snapshot = Snapshot {
                commits: vec![synthetic_commit("mock-tip", "mock-parent")],
                ..Default::default()
            };
            let ui = &mut this.git_states[project];
            ui.mode = Mode::History;
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(std::sync::Arc::new(snapshot));
            let mut patch = "commit mock-tip\nAuthor: Synthetic Developer\n\n    Synthetic body\n\ndiff --git a/first.rs b/first.rs\nnew file mode 100644\n--- /dev/null\n+++ b/first.rs\n@@ -0,0 +1,80 @@\n".to_owned();
            for index in 0..80 {
                patch.push_str(&format!("+// Synthetic line {index}\n"));
            }
            patch.push_str("diff --git a/second.rs b/second.rs\nnew file mode 100644\n--- /dev/null\n+++ b/second.rs\n@@ -0,0 +1 @@\n+// Synthetic second file\n");
            let generation = ui.begin_detail(Detail::Commit("mock-tip".into()), cx);
            ui.install_detail(generation, Ok(patch), cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let file = cx
        .debug_bounds("git-commit-file-1")
        .expect("Commit files have a navigation list");
    cx.simulate_click(file.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(
                this.git_states[this.state.active_project]
                    .commit_file
                    .as_ref()
                    .unwrap()
                    .as_str(),
                "second.rs"
            );
        })
    });
    assert!(cx.debug_bounds("git-commit-metadata").is_none());
    let toggle = cx.debug_bounds("git-toggle-commit-metadata").unwrap();
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-commit-metadata").is_some(),
        "Full original metadata remains accessible"
    );
}

#[gpui_test::test]
fn history_graph_is_cached_and_rebuilt_when_parent_relationships_change(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/graph-cache".into());
            this.init_project(project, window, cx);
            let mut snapshot = Snapshot {
                commits: vec![synthetic_commit("mock-child", "mock-parent")],
                ..Default::default()
            };
            let ui = &mut this.git_states[project];
            ui.reconcile_snapshot(&snapshot, cx);
            assert_eq!(ui.history.graph.rows.len(), 1);
            assert_eq!(ui.history.graph.rows[0].below.len(), 1);
            let graph = ui.history.graph.clone();
            ui.snapshot = Some(std::sync::Arc::new(snapshot.clone()));
            ui.reconcile_snapshot(&snapshot, cx);
            assert!(std::sync::Arc::ptr_eq(&graph, &ui.history.graph));
            snapshot.commits[0].parents.clear();
            ui.reconcile_snapshot(&snapshot, cx);
            assert!(!std::sync::Arc::ptr_eq(&graph, &ui.history.graph));
            assert!(ui.history.graph.rows[0].below.is_empty());
        })
    });
}

#[gpui_test::test]
fn parent_navigation_clears_filters_and_commit_metadata_rejects_stale_results(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let repo = tervia::git::Repository::initialize(root.path()).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root.path().into());
            this.init_project(project, window, cx);
            let snapshot = Snapshot {
                commits: vec![
                    synthetic_commit("mock-child", "mock-parent"),
                    synthetic_commit("mock-parent", "mock-unloaded"),
                ],
                ..Default::default()
            };
            let ui = &mut this.git_states[project];
            ui.repository = Some(repo);
            ui.mode = Mode::History;
            ui.reconcile_snapshot(&snapshot, cx);
            ui.snapshot = Some(std::sync::Arc::new(snapshot));
            ui.first
                .update(cx, |input, cx| input.set_value("mock-child", window, cx));
            this.open_git_commit(project, "mock-parent".into(), window, cx);
            let ui = &mut this.git_states[project];
            assert!(ui.first.read(cx).value().is_empty());
            assert!(ui.selected == Some(Detail::Commit("mock-parent".into())));
            assert_eq!(ui.commit.as_ref().unwrap().parents, "mock-unloaded");
            let parent_generation = ui.generation;
            this.open_git_commit(project, "mock-unloaded".into(), window, cx);
            let ui = &mut this.git_states[project];
            assert!(ui.selected == Some(Detail::Commit("mock-unloaded".into())));
            assert!(ui.commit.is_none());
            ui.install_commit(
                parent_generation,
                synthetic_commit("mock-parent", "mock-unloaded"),
            );
            assert!(ui.commit.is_none());
            ui.install_commit(ui.generation, synthetic_commit("mock-unloaded", ""));
            assert_eq!(ui.commit.as_ref().unwrap().hash, "mock-unloaded");
            ui.clear_detail(cx);
            assert!(ui.commit.is_none());
        })
    });
}

#[gpui_test::test]
fn worktree_open_reuses_project_state_and_removal_refuses_any_cached_dirty_draft(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let source = this.state.open_directory("/synthetic/main".into());
            this.init_project(source, window, cx);
            let linked = this.state.open_directory("/synthetic/linked".into());
            this.init_project(linked, window, cx);
            this.state.projects[linked].tab = tervia::state::Tab::Files;
            this.state.projects[linked].files.push(tervia::state::File {
                path: "mock.txt".into(),
                disk_path: "/synthetic/linked/mock.txt".into(),
                language: "plaintext".into(),
                saved: "Synthetic disk".into(),
                draft: "Synthetic draft".into(),
            });
            this.state.switch_project(source);
            assert!(!this.worktree_removal_safe("/synthetic/linked".as_ref()));
            this.activate_worktree("/synthetic/linked".into(), window, cx);
            assert_eq!(this.state.projects.len(), 2);
            assert_eq!(this.state.active_project, linked);
            assert_eq!(this.state.project().tab, tervia::state::Tab::Files);
            assert_eq!(this.state.project().files[0].draft, "Synthetic draft");
            this.state.switch_project(source);
            this.state.projects[linked].files[0].draft = "Synthetic disk".into();
            assert!(this.worktree_removal_safe("/synthetic/linked".as_ref()));
            let group = std::sync::Arc::new(tervia::git::Discovery {
                root: "/synthetic".into(),
                repositories: vec!["/synthetic/main".into(), "/synthetic/linked".into()],
                limited: false,
                identities: Default::default(),
            });
            this.git_states[source].workspace = Some(group.clone());
            this.git_states[linked].workspace = Some(group);
            this.retire_worktree("/synthetic/linked".as_ref(), cx);
            assert!(!this.state.project_visible(linked));
            assert_eq!(this.state.active_project, source);
            assert_eq!(this.state.projects.len(), 2);
            assert_eq!(
                this.git_states[source]
                    .workspace
                    .as_ref()
                    .unwrap()
                    .repositories,
                vec![std::path::PathBuf::from("/synthetic/main")]
            );
            let parent = this.state.open_directory("/synthetic".into());
            this.init_project(parent, window, cx);
            this.state.projects[parent].files.push(tervia::state::File {
                path: "linked/mock.txt".into(),
                disk_path: "linked/mock.txt".into(),
                language: "plaintext".into(),
                saved: "Synthetic disk".into(),
                draft: "Synthetic parent draft".into(),
            });
            this.state.switch_project(source);
            assert!(!this.worktree_removal_safe("/synthetic/linked".as_ref()));
            this.state.projects[parent].files[0].draft = "Synthetic disk".into();
            this.state.projects[parent].open_file(0);
            assert!(!this.worktree_removal_safe("/synthetic/linked".as_ref()));
            this.state.projects[parent].close_files(&[0]);
            assert!(this.worktree_removal_safe("/synthetic/linked".as_ref()));
        })
    });
}

#[test]
fn worktree_navigation_tokens_reject_late_completions() {
    let mut navigation = super::worktrees::Navigation::default();
    let first = navigation.begin();
    let second = navigation.begin();
    assert!(!navigation.current(first));
    assert!(navigation.current(second));
    navigation.invalidate();
    assert!(!navigation.current(second));
}

#[test]
fn prune_confirmation_contains_the_repository_and_exact_captured_missing_paths() {
    let tree = tervia::git::Worktree {
        path: "/synthetic/missing directory".into(),
        ..Default::default()
    };
    let message = super::worktrees::prune_description(
        "/synthetic/repository".as_ref(),
        &[tree],
        "Synthetic prune hint",
    );
    assert!(message.contains("/synthetic/repository"));
    assert!(message.contains("/synthetic/missing directory"));
    assert!(message.ends_with("Synthetic prune hint"));
}

#[gpui_test::test]
fn worktree_removal_reservation_blocks_every_directory_navigation_entry(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let source = this.state.open_directory("/synthetic/main".into());
            this.init_project(source, window, cx);
            this.git_states[source].worktrees.removing = Some("/synthetic/removing".into());
            this.open_repository("/synthetic/removing".into(), window, cx);
            assert_eq!(this.state.active_project, source);
            assert_eq!(this.state.projects.len(), 1);
            this.git_states[source].mode = Mode::Worktrees;
            let token = this.git_states[source].worktrees.navigation.begin();
            this.select_git_mode(source, Mode::Worktrees, window, cx);
            assert!(!this.git_states[source].worktrees.navigation.current(token));
            let token = this.git_states[source].worktrees.navigation.begin();
            this.open_repository("/synthetic/main".into(), window, cx);
            assert!(!this.git_states[source].worktrees.navigation.current(token));
            this.open_workspace(
                tervia::git::Discovery {
                    root: "/synthetic/main".into(),
                    repositories: vec!["/synthetic/main".into(), "/synthetic/removing".into()],
                    limited: false,
                    identities: Default::default(),
                },
                true,
                window,
                cx,
            );
            assert_eq!(this.state.active_project, source);
            assert_eq!(this.state.projects.len(), 1);
        })
    });
}

#[gpui_test::test]
fn opening_a_folder_keeps_the_parent_selected_and_nests_detected_repositories(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| {
        view.unwrap().update(cx, |this, cx| {
            this.open_workspace(
                tervia::git::Discovery {
                    root: "/synthetic/mock-folder".into(),
                    repositories: vec![
                        "/synthetic/mock-folder/mock-alpha".into(),
                        "/synthetic/mock-folder/mock-beta".into(),
                    ],
                    limited: false,
                    identities: Default::default(),
                },
                true,
                window,
                cx,
            );
            assert_eq!(
                this.state.active_project, 0,
                "Opening a folder must select that folder"
            );
            assert_eq!(
                this.state.project_rows(),
                vec![0],
                "Child repositories are not top-level projects"
            );
            this.state.set_tab(tervia::state::Tab::Files);
            this.change_project(1, window, cx);
            assert_eq!(this.state.project().tab, tervia::state::Tab::Terminal);
            this.new_session(window, cx);
            this.state.set_tab(tervia::state::Tab::Terminal);
            this.change_project(0, window, cx);
            assert_eq!(this.state.project().tab, tervia::state::Tab::Files);
            this.change_project(1, window, cx);
            assert_eq!(this.state.project().sessions.len(), 2);
            assert_eq!(this.state.project().tab, tervia::state::Tab::Terminal);
            this.open_workspace(
                tervia::git::Discovery {
                    root: "/synthetic/mock-folder/mock-alpha".into(),
                    repositories: vec!["/synthetic/mock-folder/mock-alpha/mock-nested".into()],
                    limited: false,
                    identities: Default::default(),
                },
                true,
                window,
                cx,
            );
            assert_eq!(
                this.state.projects.len(),
                3,
                "Reopening a nested directory reuses its enclosing project"
            );
            assert_eq!(this.state.project_rows(), vec![0]);
        });
    });
}

#[gpui_test::test]
fn workspace_discovery_registers_siblings_without_switching_projects_or_losing_drafts(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut workspace = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        workspace = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let workspace = workspace.unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let existing = this.state.open_directory("/synthetic/existing".into());
            this.init_project(existing, window, cx);
            this.git_states[existing].message.update(cx, |input, cx| {
                input.set_value("Synthetic draft", window, cx)
            });
            let discovery = tervia::git::Discovery {
                root: "/synthetic/workspace".into(),
                repositories: vec![
                    "/synthetic/workspace/alpha".into(),
                    "/synthetic/workspace/beta".into(),
                ],
                limited: false,
                identities: Default::default(),
            };
            this.open_workspace(discovery.clone(), false, window, cx);
            this.open_workspace(discovery, false, window, cx);
            assert_eq!(this.state.active_project, existing);
            assert_eq!(this.state.projects.len(), 4);
            assert_eq!(this.state.project_rows(), vec![existing, 1]);
            assert_eq!(this.working_directory_rows(1, cx), vec![1, 2, 3]);
            assert_eq!(
                this.git_states[existing].message.read(cx).value().as_ref(),
                "Synthetic draft"
            );
            for ui in &this.git_states[1..] {
                assert_eq!(ui.workspace.as_ref().unwrap().repositories.len(), 2);
            }
            this.open_workspace(
                tervia::git::Discovery {
                    root: "/synthetic/workspace".into(),
                    repositories: vec!["/synthetic/workspace/alpha".into()],
                    limited: false,
                    identities: Default::default(),
                },
                false,
                window,
                cx,
            );
            assert_eq!(
                this.state.projects.len(),
                4,
                "Removed repositories keep their project and drafts"
            );
            assert_eq!(this.state.project_rows(), vec![existing, 1]);
            assert_eq!(this.working_directory_rows(1, cx), vec![1, 2]);
            for ui in &this.git_states[1..] {
                assert_eq!(ui.workspace.as_ref().unwrap().repositories.len(), 1);
            }
        })
    });
}

#[test]
fn sibling_scans_share_one_flight_and_old_completions_cannot_replace_a_new_scan() {
    let root = std::path::PathBuf::from("/synthetic/workspace");
    let mut scans = super::Scans::default();
    let first = scans.begin(root.clone()).unwrap();
    assert!(scans.begin(root.clone()).is_none());
    scans.invalidate(&root);
    let second = scans.begin(root.clone()).unwrap();
    assert!(!scans.finish(&root, first));
    assert!(scans.running(&root));
    assert!(scans.finish(&root, second));
    assert!(!scans.running(&root));
}

#[test]
fn raw_commit_fallback_does_not_repeat_metadata_and_preserves_binary_patch_text() {
    assert_eq!(
        super::viewer::commit_patch("commit synthetic\nAuthor: Synthetic\n\n    Empty commit\n"),
        ""
    );
    assert_eq!(
        super::viewer::commit_patch(
            "commit synthetic\n\ndiff --git a/image b/image\nBinary files differ\n"
        ),
        "diff --git a/image b/image\nBinary files differ\n"
    );
}

#[test]
fn folding_change_groups_and_directories_preserves_file_ownership_and_both_sides() {
    use super::changes::{ChangeRow, change_rows};
    let files = vec![
        FileStatus {
            path: "src/nested/both.rs".into(),
            original: None,
            index: 'M',
            worktree: 'M',
            conflicted: false,
        },
        FileStatus {
            path: "readme.txt".into(),
            original: None,
            index: '.',
            worktree: '?',
            conflicted: false,
        },
    ];
    let rows = change_rows(&files, &Default::default(), &Default::default());
    assert!(rows.contains(&ChangeRow::File {
        group: 1,
        index: 0,
        depth: 2
    }));
    assert!(rows.contains(&ChangeRow::File {
        group: 2,
        index: 0,
        depth: 2
    }));
    let rows = change_rows(
        &files,
        &std::collections::BTreeSet::from([1]),
        &Default::default(),
    );
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, ChangeRow::File { group: 1, .. }))
    );
    assert!(rows.contains(&ChangeRow::Group { group: 1, count: 1 }));
    let rows = change_rows(
        &files,
        &Default::default(),
        &std::collections::BTreeSet::from([(1, "src".into())]),
    );
    assert!(!rows.iter().any(|row| matches!(
        row,
        ChangeRow::File {
            group: 1,
            index: 0,
            ..
        }
    )));
    assert!(rows.contains(&ChangeRow::File {
        group: 3,
        index: 1,
        depth: 0
    }));
    assert!(rows.contains(&ChangeRow::File {
        group: 2,
        index: 0,
        depth: 2
    }));
}

#[test]
fn untracked_changes_have_their_own_group() {
    use super::changes::{ChangeRow, change_rows};
    let make = |path: &str, index, worktree, conflicted| FileStatus {
        path: path.into(),
        original: None,
        index,
        worktree,
        conflicted,
    };
    let snapshot = Snapshot {
        status: tervia::git::Status {
            files: vec![
                make("src/conflict.rs", 'U', 'U', true),
                make("src/both.rs", 'M', 'M', false),
                make("src/new.rs", '.', '?', false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let rows = change_rows(
        &snapshot.status.files,
        &Default::default(),
        &Default::default(),
    );
    let groups: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            ChangeRow::Group { group, count } => Some((*group, *count)),
            _ => None,
        })
        .collect();
    assert_eq!(groups, vec![(0, 1), (1, 1), (3, 1), (2, 1)]);
    let entries: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            ChangeRow::File { group, index, .. } => Some((*group, *index)),
            _ => None,
        })
        .collect();
    assert_eq!(entries, vec![(0, 0), (1, 1), (3, 2), (2, 1)]);
    let folded = change_rows(
        &snapshot.status.files,
        &std::collections::BTreeSet::from([3]),
        &Default::default(),
    );
    assert!(folded.contains(&ChangeRow::Group { group: 3, count: 1 }));
    assert!(
        !folded
            .iter()
            .any(|row| matches!(row, ChangeRow::File { group: 3, .. }))
    );
    let folder_folded = change_rows(
        &snapshot.status.files,
        &Default::default(),
        &std::collections::BTreeSet::from([(3, "src".into())]),
    );
    assert!(folder_folded.contains(&ChangeRow::Folder {
        group: 3,
        path: "src".into(),
        depth: 0,
        count: 1,
    }));
    assert!(
        !folder_folded
            .iter()
            .any(|row| matches!(row, ChangeRow::File { group: 3, .. }))
    );

    let empty = change_rows(&[], &Default::default(), &Default::default());
    assert!(empty.contains(&ChangeRow::Group { group: 3, count: 0 }));
}

#[gpui_test::test]
fn large_workbench_lists_only_build_visible_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut workspace = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        workspace = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let workspace = workspace.unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory("/synthetic/large-workspace".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            let mut snapshot = Snapshot::default();
            snapshot.status.branch = "main".into();
            snapshot.commits = (0..2000)
                .map(|i| tervia::git::Commit {
                    hash: format!("{i:040x}"),
                    parents: String::new(),
                    author: "Synthetic".into(),
                    when: "2026-01-01".into(),
                    message: format!("Synthetic commit {i}"),
                    refs: String::new(),
                })
                .collect();
            snapshot.branches = (0..2000)
                .map(|i| tervia::git::Branch {
                    name: if i == 0 {
                        "main".into()
                    } else {
                        format!("feature/synthetic-{i}")
                    },
                    current: i == 0,
                    remote: false,
                    upstream: String::new(),
                })
                .collect();
            snapshot.stashes = (0..1000)
                .map(|i| tervia::git::Stash {
                    reference: format!("stash@{{{i}}}"),
                    hash: format!("{i:040x}"),
                    message: format!("Synthetic stash {i}"),
                })
                .collect();
            snapshot.status.files = (0..5000)
                .map(|i| FileStatus {
                    path: format!("synthetic/{i}.rs").into(),
                    original: None,
                    index: '.',
                    worktree: 'M',
                    conflicted: false,
                })
                .collect();
            this.git_states[project].reconcile_snapshot(&snapshot, cx);
            this.git_states[project].snapshot = Some(std::sync::Arc::new(snapshot));
        })
    });
    for mode in [Mode::History, Mode::Changes, Mode::Stash] {
        cx.update(|_, cx| {
            workspace.update(cx, |this, _| {
                this.git_states[0].mode = mode;
                this.git_states[0].rendered_rows.set(0);
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|_, cx| {
            workspace.update(cx, |this, _| {
                let rendered = this.git_states[0].rendered_rows.get();
                assert!(
                    rendered > 0 && rendered < 100,
                    "Built {rendered} rows for a bounded viewport"
                );
            })
        });
    }
}

#[test]
fn workbench_selection_follows_real_changes_without_losing_an_explicit_selection() {
    let make = |path: &str, index, worktree, conflicted| FileStatus {
        path: path.into(),
        original: None,
        index,
        worktree,
        conflicted,
    };
    let mut snapshot = Snapshot::default();
    snapshot.status.files = vec![
        make("staged.rs", 'M', '.', false),
        make("worktree.rs", '.', 'M', false),
    ];
    let worktree = Detail::File(snapshot.status.files[1].clone(), Side::Worktree);
    let staged = Detail::File(snapshot.status.files[0].clone(), Side::Index);
    assert!(
        super::navigation::choose_detail(Mode::Changes, None, &snapshot) == Some(worktree.clone())
    );
    assert!(
        super::navigation::choose_detail(Mode::Changes, Some(&staged), &snapshot) == Some(staged)
    );
    snapshot.status.files.remove(1);
    assert!(matches!(
        super::navigation::choose_detail(Mode::Changes, Some(&worktree), &snapshot),
        Some(Detail::File(_, Side::Index))
    ));
    snapshot
        .status
        .files
        .push(make("conflict.rs", 'U', 'U', true));
    assert!(
        super::navigation::choose_detail(Mode::Changes, None, &snapshot)
            == Some(Detail::Conflict("conflict.rs".into()))
    );
    snapshot.status.files.clear();
    assert!(super::navigation::choose_detail(Mode::Changes, Some(&worktree), &snapshot).is_none());
}

#[test]
fn history_and_stash_defaults_use_immutable_ids() {
    let mut snapshot = Snapshot::default();
    snapshot.commits.push(tervia::git::Commit {
        hash: "synthetic-commit".into(),
        parents: String::new(),
        author: "Synthetic".into(),
        when: "2026-01-01".into(),
        message: "Synthetic commit".into(),
        refs: String::new(),
    });
    snapshot.stashes.push(tervia::git::Stash {
        reference: "stash@{0}".into(),
        hash: "synthetic-stash".into(),
        message: "Synthetic stash".into(),
    });
    assert!(
        super::navigation::choose_detail(Mode::History, None, &snapshot)
            == Some(Detail::Commit("synthetic-commit".into()))
    );
    assert!(
        super::navigation::choose_detail(Mode::Stash, None, &snapshot)
            == Some(Detail::Stash("synthetic-stash".into()))
    );
}

#[gpui_test::test]
fn switching_diff_files_clears_old_content_and_rejects_late_results(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut workspace = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        workspace = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let workspace = workspace.unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/project".into());
            this.init_project(project, window, cx);
            let make = |path: &str| {
                Detail::File(
                    FileStatus {
                        path: path.into(),
                        original: None,
                        index: '.',
                        worktree: 'M',
                        conflicted: false,
                    },
                    Side::Worktree,
                )
            };
            let ui = &mut this.git_states[project];
            let first = ui.begin_detail(make("first.rs"), cx);
            ui.install_detail(first, Ok("synthetic first patch".into()), cx);
            assert_eq!(ui.detail, "synthetic first patch");
            let second = ui.begin_detail(make("second.rs"), cx);
            assert!(ui.detail.is_empty());
            ui.install_detail(first, Ok("late synthetic first patch".into()), cx);
            assert!(ui.detail.is_empty());
            ui.install_detail(second, Ok("synthetic second patch".into()), cx);
            assert_eq!(ui.detail, "synthetic second patch");
            let mut snapshot = Snapshot::default();
            if let Detail::File(file, _) = make("next.rs") {
                snapshot.status.files.push(file);
            }
            ui.reconcile_snapshot(&snapshot, cx);
            assert!(ui.selected == Some(make("next.rs")));
            assert!(ui.detail.is_empty());
            ui.install_detail(second, Ok("late synthetic removed file patch".into()), cx);
            assert!(ui.detail.is_empty());
            ui.clear_detail(cx);
            ui.install_detail(second, Ok("late synthetic second patch".into()), cx);
            assert!(ui.selected.is_none());
            assert!(ui.detail.is_empty());
        })
    });
}

#[gpui_test::test]
fn opening_and_switching_projects_saves_the_workspace_before_quitting(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let alpha = root.path().join("mock-alpha");
    let beta = root.path().join("mock-beta");
    fs::create_dir(&alpha).unwrap();
    fs::create_dir(&beta).unwrap();
    let alpha = alpha.canonicalize().unwrap();
    let beta = beta.canonicalize().unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            Tervia::configured(
                Ok(tervia::config::Config::new(root.path().join("config.toml"))),
                window,
                cx,
            )
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| {
        view.unwrap().update(cx, |this, cx| {
            this.open_workspace(
                tervia::git::Repository::discover(&alpha).unwrap(),
                true,
                window,
                cx,
            );
            this.open_workspace(
                tervia::git::Repository::discover(&beta).unwrap(),
                true,
                window,
                cx,
            );
            this.change_project(0, window, cx);
        });
    });
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("workspace.json")).unwrap())
            .unwrap();
    assert_eq!(saved["projects"].as_array().unwrap().len(), 2);
    assert_eq!(saved["active"], serde_json::json!(alpha));
    assert_eq!(saved["projects"][1]["directory"], serde_json::json!(beta));
}

#[gpui_test::test]
fn corrupt_project_history_preserves_preferences_and_reports_the_state_file(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    let history = root.path().join("workspace.json");
    fs::write(&config, "[ui]\nlanguage = 'zh-CN'\ntheme = 'light'\n").unwrap();
    fs::write(&history, "{synthetic invalid JSON").unwrap();
    let project = root.path().join("mock-project");
    fs::create_dir(&project).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity =
            cx.new(|cx| Tervia::configured(Ok(tervia::config::Config::new(config)), window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| {
        view.unwrap().update(cx, |this, cx| {
            assert!(this.appearance.light_theme);
            assert_eq!(this.appearance.language, tervia::locale::Language::Chinese);
            assert!(this.config_error.read(cx).is_none());
            assert!(this.state.notice.contains("workspace.json"));
            assert!(this.state.projects.is_empty());
            this.open_workspace(
                tervia::git::Repository::discover(&project).unwrap(),
                true,
                window,
                cx,
            );
            assert_eq!(this.state.projects.len(), 1);
        });
    });
    assert_eq!(
        fs::read_to_string(history).unwrap(),
        "{synthetic invalid JSON"
    );
}

#[gpui_test::test]
fn git_passive_detail_failure_preserves_the_failed_action(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| {
        view.unwrap().update(cx, |this, cx| {
            let project = this
                .state
                .open_directory("/synthetic/detail-failure".into());
            this.init_project(project, window, cx);
            let ui = &mut this.git_states[project];
            ui.error.action("synthetic failed action");
            let generation = ui.begin_detail(Detail::Commit("synthetic-commit".into()), cx);
            ui.install_detail_with_origin(
                generation,
                Err("synthetic failed passive detail".into()),
                super::error::Origin::Probe,
                cx,
            );
            assert_eq!(ui.error.details(), Some("synthetic failed action"));
            assert!(!ui.detail_loading, "Failed passive reads finish loading");
        })
    });
}

#[cfg(unix)]
#[gpui_test::test]
fn git_action_and_snapshot_failures_recover_independently(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    cx.update(gpui_kit::init);
    for operation_fails in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let repo = tervia::git::Repository::initialize(root.path()).unwrap();
        for (key, value) in [
            ("user.name", "Synthetic Developer"),
            ("user.email", "synthetic@example.test"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", ".git/hooks"),
        ] {
            assert!(
                std::process::Command::new("git")
                    .current_dir(root.path())
                    .args(["config", "--local", key, value])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::write(root.path().join("mock.txt"), "synthetic content\n").unwrap();
        repo.execute(tervia::git::Operation::StageAll).unwrap();
        let head = fs::read(root.path().join(".git/HEAD")).unwrap();
        let hook = root.path().join(if operation_fails {
            ".git/hooks/pre-commit"
        } else {
            ".git/hooks/post-commit"
        });
        fs::write(
            &hook,
            format!(
                "#!/bin/sh\necho 'synthetic action failure' >&2\nrm .git/HEAD\nexit {}\n",
                u8::from(operation_fails)
            ),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut this = Tervia::new(window, cx);
                let project = this
                    .state
                    .open_directory(root.path().canonicalize().unwrap());
                this.init_project(project, window, cx);
                this.state.set_tab(tervia::state::Tab::Git);
                this.git_states[project].message.update(cx, |input, cx| {
                    input.set_value("Synthetic draft", window, cx)
                });
                this.git_job(
                    project,
                    Some(tervia::git::Operation::Commit {
                        message: "Synthetic commit".into(),
                        amend: false,
                    }),
                    false,
                    window,
                    cx,
                );
                this
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        cx.run_until_parked();
        let mut diagnostic = String::new();
        view.update(cx, |this, cx| {
            let ui = &this.git_states[0];
            assert!(
                ui.snapshot.is_none(),
                "The hook damaged metadata before status reconciliation"
            );
            diagnostic = ui.error.details().unwrap().to_owned();
            if operation_fails {
                assert!(
                    diagnostic.contains("synthetic action failure"),
                    "The snapshot failure cannot replace the action diagnostic: {diagnostic}"
                );
            }
            assert_eq!(
                ui.message.read(cx).value().as_str(),
                if operation_fails {
                    "Synthetic draft"
                } else {
                    ""
                }
            );
        });
        fs::write(root.path().join(".git/HEAD"), head).unwrap();
        fs::remove_file(hook).unwrap();
        cx.update(|window, cx| view.update(cx, |this, cx| this.refresh_git(0, window, cx)));
        cx.run_until_parked();
        view.update(cx, |this, _| {
            assert!(this.git_states[0].snapshot.is_some());
            if operation_fails {
                assert_eq!(
                    this.git_states[0].error.details(),
                    Some(diagnostic.as_str())
                );
            } else {
                assert!(
                    this.git_states[0].error.is_empty(),
                    "A successful operation's recovered snapshot failure is passive"
                );
            }
        });
    }
}

#[gpui_test::test]
fn git_repeated_passive_detail_failure_keeps_disclosure_open_until_recovery(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let repo = tervia::git::Repository::initialize(root.path()).unwrap();
    fs::write(root.path().join(".git/info/attributes"), "mock.txt diff\n").unwrap();
    fs::write(root.path().join("mock.txt"), b"synthetic non-UTF-8: \xff\n").unwrap();
    repo.execute(tervia::git::Operation::StageAll).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this
                .state
                .open_directory(root.path().canonicalize().unwrap());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            this.refresh_git(project, window, cx);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx
        .debug_bounds("git-toggle-error-details")
        .expect("The forced-text diff reports non-UTF-8 guidance");
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-error-details").is_some());
    cx.update(|window, cx| view.update(cx, |this, cx| this.refresh_git(0, window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-error-details").is_some(),
        "An unchanged detail error cannot collapse during successful snapshot polling"
    );
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.git_job(
                0,
                Some(tervia::git::Operation::Switch(
                    "synthetic-missing-branch".into(),
                )),
                false,
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert!(
            this.git_states[0]
                .error
                .details()
                .unwrap()
                .contains("synthetic-missing-branch"),
            "The automatic failed diff cannot replace the failed action"
        )
    });
    // Start a successful action to dismiss the prior action diagnostic before checking probe recovery.
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.git_job(0, Some(tervia::git::Operation::StageAll), false, window, cx)
        })
    });
    cx.run_until_parked();
    fs::write(root.path().join("mock.txt"), "synthetic repaired content\n").unwrap();
    repo.execute(tervia::git::Operation::StageAll).unwrap();
    cx.update(|window, cx| view.update(cx, |this, cx| this.refresh_git(0, window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-error").is_none(),
        "A repaired passive detail clears its failure"
    );
}

#[gpui_test::test]
fn git_plain_folder_keeps_init_and_clone_guidance_after_a_clone_failure(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let failure = tervia::git::Repository::clone_into(
        root.path()
            .join("synthetic-missing-source")
            .to_str()
            .unwrap(),
        &root.path().join("synthetic-destination"),
    )
    .unwrap_err();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this
                .state
                .open_directory(root.path().canonicalize().unwrap());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            this.refresh_git(project, window, cx);
            this
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        this.git_states[0].error.action(failure);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("git-empty-heading").is_some());
    assert!(
        cx.debug_bounds("git-init").is_some(),
        "The known ordinary folder still offers initialization"
    );
    assert!(
        cx.debug_bounds("git-clone").is_some(),
        "Clone can be retried in the known ordinary folder"
    );
    assert!(cx.debug_bounds("git-copy-error").is_some());
}

#[gpui_test::test]
fn git_late_successful_detail_cannot_clear_a_failed_repository_probe(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| {
        view.unwrap().update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/late-detail".into());
            this.init_project(project, window, cx);
            let ui = &mut this.git_states[project];
            let generation = ui.begin_detail(Detail::Commit("synthetic-commit".into()), cx);
            ui.snapshot = None;
            ui.error.probe("synthetic failed repository probe".into());
            ui.install_detail_with_origin(
                generation,
                Ok("synthetic successful late detail".into()),
                super::error::Origin::Probe,
                cx,
            );
            assert_eq!(
                ui.error.details(),
                Some("synthetic failed repository probe"),
                "A successful detail without a loaded status cannot recover a repository failure"
            );
        })
    });
}

#[gpui_test::test]
fn git_unprobed_directory_waits_for_a_positive_probe_before_offering_initialization(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this.state.open_directory("/synthetic/unprobed".into());
            this.init_project(project, window, cx);
            this.state.set_tab(tervia::state::Tab::Git);
            this
        });
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("git-init").is_none(),
        "Unknown metadata cannot offer initialization"
    );
    assert!(
        cx.debug_bounds("git-empty-heading").is_none(),
        "An unprobed directory is not declared a non-repository"
    );
    assert!(
        cx.debug_bounds("git-retry").is_some(),
        "An unprobed directory offers a status refresh"
    );
}

#[gpui_test::test]
fn git_failed_worktree_snapshot_invalidates_cached_recovery_evidence(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for failed_action in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let repo = tervia::git::Repository::initialize(root.path()).unwrap();
        let snapshot = std::sync::Arc::new(repo.snapshot(10).unwrap());
        let workspace =
            std::sync::Arc::new(tervia::git::Repository::discover(root.path()).unwrap());
        fs::remove_file(root.path().join(".git/HEAD")).unwrap();
        let failure = repo.snapshot(10).unwrap_err();
        let diagnostic = failure.details();
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| Tervia::new(window, cx));
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        cx.update(|window, cx| {
            view.unwrap().update(cx, |this, cx| {
                let project = this.state.open_directory(root.path().canonicalize().unwrap());
                this.init_project(project, window, cx);
                let ui = &mut this.git_states[project];
                ui.repository = Some(repo);
                ui.workspace = Some(workspace.clone());
                ui.snapshot = Some(snapshot);
                ui.message.update(cx, |input, cx| input.set_value("Synthetic commit draft", window, cx));
                let generation = ui.begin_detail(Detail::Commit("synthetic-commit".into()), cx);
                if failed_action { ui.error.action("synthetic failed worktree action"); }
                ui.fail_snapshot(failure, super::error::Origin::Probe);
                ui.install_detail_with_origin(generation, Ok("synthetic successful late detail".into()), super::error::Origin::Probe, cx);
                assert_eq!(ui.error.details(), Some(if failed_action { "synthetic failed worktree action" } else { diagnostic.as_str() }), "A matching-generation detail cannot recover failed worktree metadata from a cached snapshot");
                assert!(ui.snapshot.is_none(), "Failed metadata invalidates cached snapshot evidence");
                assert!(!ui.plain_directory);
                assert!(!ui.detail_loading);
                assert_eq!(ui.detail, "synthetic successful late detail", "The current-generation detail still completes");
                assert_eq!(ui.message.read(cx).value().as_str(), "Synthetic commit draft");
                assert!(std::sync::Arc::ptr_eq(ui.workspace.as_ref().unwrap(), &workspace), "Workspace ownership is retained");
                assert!(ui.repository.is_some());
            })
        });
    }
}
