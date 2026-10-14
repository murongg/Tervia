use super::{Detail, FileStatus, Mode, Side, Snapshot, Tervia};
use core::prelude::v1::test;
use gpui_kit::AppContext as _;
use gpui_test::TestAppContext;
use std::fs;

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
                ui.error
            );
            assert!(!this.git_mutating);
        });
    }
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
            assert_eq!(ui.history_graph.rows.len(), 1);
            assert_eq!(ui.history_graph.rows[0].below.len(), 1);
            let graph = ui.history_graph.clone();
            ui.snapshot = Some(std::sync::Arc::new(snapshot.clone()));
            ui.reconcile_snapshot(&snapshot, cx);
            assert!(std::sync::Arc::ptr_eq(&graph, &ui.history_graph));
            snapshot.commits[0].parents.clear();
            ui.reconcile_snapshot(&snapshot, cx);
            assert!(!std::sync::Arc::ptr_eq(&graph, &ui.history_graph));
            assert!(ui.history_graph.rows[0].below.is_empty());
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
