use super::{Detail, FileStatus, Mode, Side, Snapshot, Tervia};
use core::prelude::v1::test;
use gpui_kit::AppContext as _;
use gpui_test::TestAppContext;

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
    assert!(rows.contains(&ChangeRow::Group { group: 1, count: 2 }));
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
        group: 1,
        index: 1,
        depth: 0
    }));
    assert!(rows.contains(&ChangeRow::File {
        group: 2,
        index: 0,
        depth: 2
    }));
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
    for mode in [Mode::History, Mode::Changes, Mode::Branches, Mode::Stash] {
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
fn history_and_stash_defaults_use_immutable_ids_and_branch_selection_has_a_safe_fallback() {
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
    snapshot.branches = vec![
        tervia::git::Branch {
            name: "main".into(),
            current: true,
            remote: false,
            upstream: String::new(),
        },
        tervia::git::Branch {
            name: "feature/synthetic".into(),
            current: false,
            remote: false,
            upstream: String::new(),
        },
    ];
    assert_eq!(
        super::navigation::branch_target(Some("feature/synthetic"), &snapshot)
            .unwrap()
            .name,
        "feature/synthetic"
    );
    assert_eq!(
        super::navigation::branch_target(Some("removed"), &snapshot)
            .unwrap()
            .name,
        "main"
    );
    assert!(super::navigation::choose_detail(Mode::Branches, None, &snapshot).is_none());
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
