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
