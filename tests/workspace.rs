use tervia::state::{File, FileClose, Tab, Workspace};

fn workspace() -> Workspace {
    let mut workspace = Workspace::new("/synthetic/project-a".into());
    workspace.new_session();
    workspace.open_directory("/synthetic/project-b".into());
    workspace.new_session();
    workspace.switch_project(0);
    workspace.select_session(0);
    workspace
}

#[test]
fn opening_the_first_real_project_from_the_welcome_screen_selects_its_workspace() {
    let mut workspace = Workspace::empty();
    assert_eq!(workspace.open_directory("/synthetic/project".into()), 0);
    assert_eq!(
        workspace.project().root,
        std::path::PathBuf::from("/synthetic/project")
    );
    workspace.new_session();
    assert_eq!(workspace.project().sessions.len(), 2);
}

#[test]
fn reopening_a_project_preserves_its_terminals_and_module() {
    let mut workspace = workspace();
    workspace.set_tab(Tab::Files);
    let root = workspace.project().root.clone();
    workspace.switch_project(1);
    assert_eq!(workspace.open_directory(root), 0);
    assert_eq!(workspace.project().sessions.len(), 2);
    assert_eq!(workspace.project().tab, Tab::Files);
}

#[test]
fn retired_worktree_projects_keep_stable_owner_indices_and_reopen_in_place() {
    let mut state = workspace();
    state.retire_project(1);
    assert_eq!(state.active_project, 0);
    assert!(!state.project_visible(1));
    state.switch_project(1);
    assert_eq!(state.active_project, 0);
    assert_eq!(state.projects.len(), 2);
    assert_eq!(state.open_directory("/synthetic/project-b".into()), 1);
    assert!(state.project_visible(1));
    assert_eq!(state.projects.len(), 2);
}

#[test]
fn ordinary_directories_inside_one_repository_keep_their_own_context_and_owner() {
    let mut state = Workspace::new("/synthetic/mock-repository".into());
    state.group_folder(0, &[]);
    let identity = std::path::PathBuf::from("/synthetic/mock-repository/.git");
    state.associate_repository(0, identity.clone(), None);
    let src = state.register_directory("/synthetic/mock-repository/src".into());
    let docs = state.register_directory("/synthetic/mock-repository/docs".into());
    state.attach_directory(0, src);
    state.attach_directory(0, docs);
    state.associate_repository(src, identity.clone(), None);
    state.associate_repository(docs, identity, None);
    state.switch_project(src);
    state.new_session();
    state.set_tab(Tab::Files);
    state.switch_project(docs);
    state.switch_project(src);
    assert_eq!(
        state.project().root,
        std::path::PathBuf::from("/synthetic/mock-repository/src")
    );
    assert_eq!(state.project().sessions.len(), 2);
    assert_eq!(state.project().tab, Tab::Files);
    assert_eq!(state.project_rows(), vec![0]);
    assert_eq!(state.folder(src), Some(0));
}

#[test]
fn folders_keep_repositories_nested_and_remember_the_selected_directory() {
    let mut state = Workspace::new("/synthetic/mock-folder".into());
    let alpha = state.register_directory("/synthetic/mock-folder/mock-alpha".into());
    let beta = state.register_directory("/synthetic/mock-folder/mock-beta".into());
    state.group_folder(0, &[alpha, beta]);
    assert_eq!(state.project_rows(), vec![0]);
    assert_eq!(state.folder_children(0), vec![alpha, beta]);
    assert_eq!(state.folder(beta), Some(0));
    assert!(state.folder_expanded(0));
    state.switch_project(beta);
    state.set_tab(Tab::Files);
    state.new_session();
    state.toggle_folder(0);
    assert!(!state.folder_expanded(0));
    assert_eq!(state.active_project, beta);
    assert_eq!(state.project_target(0), beta);
    assert_eq!(state.project_label(0), "mock-folder");
    state.switch_project(0);
    assert_eq!(
        state.project().root,
        std::path::PathBuf::from("/synthetic/mock-folder")
    );
    state.switch_project(beta);
    assert_eq!(state.project().tab, Tab::Files);
    assert_eq!(state.project().sessions.len(), 2);
}

#[test]
fn rescanning_a_folder_keeps_cached_drafts_owned_without_showing_stale_repositories() {
    let mut state = Workspace::new("/synthetic/mock-folder".into());
    let alpha = state.register_directory("/synthetic/mock-folder/mock-alpha".into());
    let beta = state.register_directory("/synthetic/mock-folder/mock-beta".into());
    state.group_folder(0, &[alpha, beta]);
    state.switch_project(beta);
    state.project_mut().files.push(File {
        path: "mock.txt".into(),
        disk_path: "/synthetic/mock-folder/mock-beta/mock.txt".into(),
        language: "plaintext".into(),
        saved: "Synthetic disk".into(),
        draft: "Synthetic draft".into(),
    });
    state.group_folder(0, &[alpha]);
    assert_eq!(state.project_rows(), vec![0]);
    assert_eq!(state.folder_children(0), vec![alpha]);
    assert_eq!(state.folder(beta), Some(0));
    assert_eq!(state.project_target(0), beta);
    assert!(state.project().files[0].dirty());
}

#[test]
fn nested_worktrees_share_one_child_row_and_keep_the_enclosing_folder() {
    let mut state = Workspace::new("/synthetic/mock-folder".into());
    let main = state.register_directory("/synthetic/mock-folder/mock-main".into());
    let linked = state.register_directory("/synthetic/mock-folder/mock-linked".into());
    let identity = std::path::PathBuf::from("/synthetic/mock-folder/mock-main/.git");
    state.associate_repository(
        main,
        identity.clone(),
        Some("/synthetic/mock-folder/mock-main".into()),
    );
    state.associate_repository(linked, identity.clone(), None);
    state.group_folder(0, &[main, linked]);
    assert_eq!(state.folder_children(0), vec![main]);
    let outside = state.register_directory("/synthetic/mock-worktree".into());
    state.associate_repository(outside, identity, None);
    state.switch_project(outside);
    assert_eq!(state.folder(outside), Some(0));
    assert_eq!(state.project_rows(), vec![0]);
    assert_eq!(state.project_target(main), outside);
    assert_eq!(state.project_target(0), outside);
    state.retire_project(outside);
    state.switch_project(0);
    state.retire_project(outside);
    assert_eq!(state.project_rows(), vec![0]);
}

#[test]
fn opening_an_enclosing_folder_reuses_an_already_open_repository_as_a_child() {
    let mut state = Workspace::new("/synthetic/mock-folder/mock-child".into());
    state.group_folder(0, &[]);
    state.new_session();
    let parent = state.register_directory("/synthetic/mock-folder".into());
    state.group_folder(parent, &[0]);
    assert_eq!(state.project_rows(), vec![parent]);
    assert_eq!(state.folder(0), Some(parent));
    assert_eq!(state.project_target(parent), 0);
    assert_eq!(state.projects[0].sessions.len(), 2);
}

#[test]
fn grouping_an_active_external_worktree_preserves_the_folders_last_directory() {
    let mut state = Workspace::new("/synthetic/mock-folder/mock-main".into());
    let identity = std::path::PathBuf::from("/synthetic/mock-folder/mock-main/.git");
    state.associate_repository(0, identity.clone(), None);
    let linked = state.open_directory("/synthetic/mock-linked".into());
    state.associate_repository(linked, identity, None);
    let parent = state.register_directory("/synthetic/mock-folder".into());
    state.group_folder(parent, &[0]);
    assert_eq!(state.project_rows(), vec![parent]);
    assert_eq!(state.project_target(parent), linked);
}

#[test]
fn retiring_a_folder_worktree_keeps_the_surviving_repository_visible() {
    let mut state = Workspace::new("/synthetic/mock-linked".into());
    let identity = std::path::PathBuf::from("/synthetic/mock-main/.git");
    state.associate_repository(0, identity.clone(), Some("/synthetic/mock-main".into()));
    state.group_folder(0, &[]);
    let main = state.register_directory("/synthetic/mock-main".into());
    state.associate_repository(main, identity, None);
    state.switch_project(main);
    state.retire_project(0);
    assert_eq!(state.project_rows(), vec![main]);
    assert_eq!(state.active_project, main);
}

#[test]
fn worktrees_share_one_project_entry_and_restore_its_last_directory() {
    let mut state = Workspace::new("/synthetic/repository".into());
    state.associate_repository(
        0,
        "/synthetic/repository/.git".into(),
        Some("/synthetic/repository".into()),
    );
    let linked = state.open_directory("/synthetic/feature-worktree".into());
    state.associate_repository(linked, "/synthetic/repository/.git".into(), None);
    state.set_tab(Tab::Files);
    state.new_session();
    assert_eq!(state.project_rows(), vec![0]);
    assert!(state.same_project(0, linked));
    assert_eq!(state.project_label(linked), "repository");
    assert_eq!(state.project_target(0), linked);
    let other = state.open_directory("/synthetic/other".into());
    state.associate_repository(
        other,
        "/synthetic/other/.git".into(),
        Some("/synthetic/other".into()),
    );
    assert_eq!(state.project_rows(), vec![0, other]);
    state.switch_project(state.project_target(0));
    assert_eq!(state.active_project, linked);
    assert_eq!(state.project().tab, Tab::Files);
    assert_eq!(state.project().sessions.len(), 2);
    state.switch_project(other);
    state.retire_project(linked);
    assert_eq!(state.project_target(0), 0);
}

#[test]
fn projects_restore_their_own_module_and_session() {
    let mut workspace = workspace();
    workspace.set_tab(Tab::Files);
    workspace.switch_project(1);
    assert_eq!(workspace.project().tab, Tab::Git);
    workspace.new_session();
    workspace.switch_project(0);
    assert_eq!(workspace.project().tab, Tab::Files);
    assert_eq!(workspace.project().sessions.len(), 2);
    workspace.switch_project(1);
    assert_eq!(workspace.project().sessions.len(), 3);
}

#[test]
fn invalid_project_switch_does_not_lose_current_state() {
    let mut workspace = workspace();
    workspace.set_tab(Tab::Git);
    workspace.switch_project(999);
    assert_eq!(workspace.active_project, 0);
    assert_eq!(workspace.project().tab, Tab::Git);
}

#[test]
fn closing_last_terminal_keeps_one_session_available() {
    let mut workspace = workspace();
    workspace.close_session(1);
    workspace.close_session(0);
    assert_eq!(workspace.project().sessions.len(), 1);
    assert_eq!(workspace.project().active_session, 0);
}

#[test]
fn escape_dismisses_quick_actions_before_closing_the_workspace() {
    let mut workspace = workspace();
    workspace.palette_open = true;
    assert!(workspace.dismiss_palette());
    assert!(!workspace.palette_open);
    assert!(!workspace.dismiss_palette());
}

#[test]
fn closing_the_second_pane_disables_split_instead_of_rendering_one_session_twice() {
    let mut workspace = workspace();
    workspace.toggle_split();
    workspace.close_session(1);
    assert!(!workspace.project().split_terminal);
}

#[test]
fn focused_pane_commands_leave_the_selected_tab_and_pane_order_stable() {
    let mut workspace = workspace();
    workspace.toggle_split();
    workspace.focus_session(1);
    assert_eq!(workspace.command_session(), 1);
    assert_eq!(workspace.project().active_session, 0);
    workspace.close_session(1);
    assert_eq!(workspace.command_session(), 0);
}

fn files_workspace() -> Workspace {
    let mut state = Workspace::new("/synthetic/file-tabs".into());
    state.project_mut().files = (0..4)
        .map(|index| File {
            path: format!("mock-{index}.txt"),
            disk_path: format!("mock-{index}.txt").into(),
            language: "plaintext".into(),
            saved: "Synthetic".into(),
            draft: "Synthetic".into(),
        })
        .collect();
    for index in 0..4 {
        state.open_file(index);
    }
    state
}

#[test]
fn closing_file_tabs_selects_a_neighbor_and_preserves_document_indices() {
    let mut state = files_workspace();
    let targets = state.project().file_targets(FileClose::Right(1));
    assert_eq!(targets, vec![2, 3]);
    assert!(state.project_mut().close_files(&targets));
    assert_eq!(state.project().open_files, vec![0, 1]);
    assert_eq!(state.project().active_file, 1);
    assert_eq!(state.project().files[3].path, "mock-3.txt");
    let targets = state.project().file_targets(FileClose::All);
    state.project_mut().close_files(&targets);
    assert!(state.project().open_files.is_empty());
    assert!(state.project().current_file().is_none());
    assert_eq!(state.project_mut().reopen_file(), Some(1));
    assert_eq!(state.project().open_files, vec![1]);
}

#[test]
fn tab_reordering_and_cycling_keep_the_selected_document_identity() {
    let mut state = files_workspace();
    assert!(state.project_mut().move_file(0, 2));
    assert_eq!(state.project().open_files, vec![1, 2, 0, 3]);
    assert_eq!(state.project().active_file, 3);
    state.project_mut().cycle_file(true);
    assert_eq!(state.project().active_file, 0);
    state.project_mut().cycle_file(false);
    assert_eq!(state.project().active_file, 3);
    assert!(!state.project_mut().move_file(999, 0));
}

#[test]
fn closing_others_and_reopening_is_project_owned_and_deduplicated() {
    let mut state = files_workspace();
    let targets = state.project().file_targets(FileClose::Others(1));
    state.project_mut().close_files(&targets);
    assert_eq!(state.project().open_files, vec![1]);
    assert!(state.project().file_targets(FileClose::One(0)).is_empty());
    state.open_directory("/synthetic/another-project".into());
    assert!(state.project_mut().reopen_file().is_none());
    state.switch_project(0);
    assert_eq!(state.project_mut().reopen_file(), Some(3));
    state.open_file(3);
    assert_eq!(state.project().open_files, vec![1, 3]);
    assert!(
        state
            .project()
            .file_targets(FileClose::Right(999))
            .is_empty()
    );
}
