use std::path::PathBuf;
use tervia::state::{File, Tab, Workspace};

fn workspace() -> Workspace {
    let root = std::env::temp_dir().join("tervia-synthetic-catalog");
    let mut state = Workspace::new(root.join("mock-alpha"));
    state.open_directory(root.join("mock-beta"));
    state.open_directory(root.join("mock-gamma"));
    state
}

#[test]
fn project_avatars_are_assigned_without_repeats_until_the_catalog_is_used() {
    let mut state = Workspace::empty();
    let mut assigned = std::collections::BTreeSet::new();
    for index in 0..tervia::state::PROJECT_AVATARS.len() {
        let owner = state.open_directory(std::env::temp_dir().join(format!("mock-avatar-{index}")));
        assert!(assigned.insert(state.project_avatar(owner)));
    }
    let avatars: Vec<_> = (0..state.projects.len())
        .map(|owner| state.project_avatar(owner))
        .collect();
    state.move_project(1, 0);
    state.toggle_project_favorite(2);
    state.close_project(0);
    state.reopen_project(0);
    assert_eq!(
        (0..state.projects.len())
            .map(|owner| state.project_avatar(owner))
            .collect::<Vec<_>>(),
        avatars
    );
    let extra = state.open_directory(std::env::temp_dir().join("mock-avatar-extra"));
    assert!(assigned.contains(state.project_avatar(extra)));
}

#[test]
fn changing_an_avatar_updates_the_logical_project_and_validates_the_choice() {
    let mut state = workspace();
    let child = state.register_directory(state.projects[0].root.join("mock-web"));
    state.attach_directory(0, child);
    assert!(state.set_project_avatar(child, "cat"));
    assert_eq!(state.project_avatar(0), "cat");
    assert_eq!(state.project_avatar(child), "cat");
    let other = state.project_avatar(1);
    assert!(!state.set_project_avatar(0, "unknown-synthetic-avatar"));
    assert!(!state.set_project_avatar(usize::MAX, "cat"));
    assert_eq!(state.project_avatar(0), "cat");
    assert_eq!(state.project_avatar(1), other);

    let identity = state.projects[1].root.join(".git");
    let root = state.projects[1].root.clone();
    state.associate_repository(1, identity.clone(), Some(root.clone()));
    let linked = state.register_directory(std::env::temp_dir().join("mock-avatar-linked"));
    state.associate_repository(linked, identity, Some(root));
    assert!(state.set_project_avatar(linked, "moon"));
    assert_eq!(state.project_avatar(1), "moon");
    assert_eq!(state.project_avatar(linked), "moon");
}

#[test]
fn favorites_and_manual_order_do_not_move_owner_slots_or_follow_recent_switches() {
    let mut state = workspace();
    let roots: Vec<_> = state
        .projects
        .iter()
        .map(|project| project.root.clone())
        .collect();
    assert!(state.move_project(2, 0));
    assert_eq!(state.project_rows(), vec![2, 0, 1]);
    assert!(state.toggle_project_favorite(1));
    assert_eq!(state.project_rows(), vec![1, 2, 0]);
    state.switch_project(0);
    state.switch_project(2);
    assert_eq!(state.project_rows(), vec![1, 2, 0]);
    assert!(
        !state.move_project(0, 1),
        "Dragging does not silently change favorites"
    );
    assert_eq!(
        state
            .projects
            .iter()
            .map(|project| project.root.clone())
            .collect::<Vec<_>>(),
        roots
    );
    assert!(!state.toggle_project_favorite(1));
    assert_eq!(state.project_rows(), vec![2, 0, 1]);
}

#[test]
fn previous_project_restores_its_last_directory_and_skips_same_project_visits() {
    let mut state = workspace();
    let child = state.register_directory(state.projects[0].root.join("mock-web"));
    state.attach_directory(0, child);
    state.switch_project(child);
    state.switch_project(1);
    assert_eq!(state.previous_project(), Some(child));
    state.switch_project(child);
    state.switch_project(0);
    assert_eq!(state.previous_project(), Some(1));
    assert_eq!(state.project_rows(), vec![0, 1, 2]);
}

#[test]
fn global_search_matches_names_paths_and_fuzzy_directory_names() {
    let mut state = workspace();
    let child = state.register_directory(state.projects[0].root.join("mock-apps/mock-web"));
    state.attach_directory(0, child);
    assert_eq!(state.search_projects("mock-beta", false)[0].owner, 1);
    assert_eq!(state.search_projects("MKWEB", false)[0].index, child);
    let query = state.projects[1].root.to_string_lossy().into_owned();
    assert_eq!(state.search_projects(&query, false)[0].owner, 1);
    assert!(
        state
            .search_projects("no-synthetic-match", false)
            .is_empty()
    );
    state.toggle_project_favorite(0);
    assert_eq!(state.search_projects("", false)[0].owner, 0);
}

#[test]
fn name_matches_rank_ahead_of_favorite_path_matches() {
    let mut state = workspace();
    state.projects[0].root = std::env::temp_dir().join("mock-beta-assets/mock-alpha");
    state.toggle_project_favorite(0);
    assert_eq!(state.search_projects("mock-beta", false)[0].owner, 1);
}

#[test]
fn closing_a_project_hides_all_directories_and_reopening_retains_drafts_and_layout() {
    let mut state = workspace();
    let child = state.register_directory(state.projects[0].root.join("mock-web"));
    state.attach_directory(0, child);
    state.switch_project(child);
    state.new_session();
    state.set_tab(Tab::Files);
    state.project_mut().dock.visible = true;
    state.project_mut().files.push(File {
        path: "mock.rs".into(),
        disk_path: PathBuf::from("mock.rs"),
        language: "rust".into(),
        saved: "base".into(),
        draft: "synthetic unsaved draft".into(),
    });
    state.project_mut().open_file(0);
    assert_eq!(state.close_project(0), Some(vec![0, child]));
    assert!(!state.project_visible(child));
    assert_eq!(state.project_rows(), vec![1, 2]);
    assert_eq!(state.closed_project_rows(), vec![0]);
    assert_eq!(
        state.folder(child),
        Some(0),
        "Closing retains logical ownership"
    );
    assert_eq!(state.search_projects("mock-web", true)[0].owner, 0);
    assert_eq!(state.reopen_project(0), Some(child));
    assert_eq!(
        state.project().current_file().unwrap().draft,
        "synthetic unsaved draft"
    );
    assert_eq!(state.project().tab, Tab::Files);
    assert!(state.project().dock.visible);
    assert_eq!(state.project().terminal.sessions.len(), 2);
    assert!(state.closed_project_rows().is_empty());
}

#[test]
fn closing_linked_worktrees_is_one_project_and_never_reuses_retired_directories() {
    let mut state = workspace();
    let identity = state.projects[0].root.join(".git");
    let root = state.projects[0].root.clone();
    state.associate_repository(0, identity.clone(), Some(root.clone()));
    let linked = state.register_directory(std::env::temp_dir().join("tervia-synthetic-linked"));
    state.associate_repository(linked, identity, Some(root));
    state.switch_project(linked);
    assert_eq!(state.close_project(0), Some(vec![0, linked]));
    assert!(!state.project_visible(linked));
    assert_eq!(state.reopen_project(0), Some(linked));
    state.switch_project(1);
    state.retire_project(linked);
    state.close_project(0);
    state.reopen_project(0);
    assert!(!state.project_visible(linked));
    assert_eq!(state.active_project, 0);
}

#[test]
fn closing_all_projects_leaves_a_reopenable_catalog() {
    let mut state = workspace();
    for owner in [0, 1, 2] {
        state.close_project(owner);
    }
    assert!(state.project_rows().is_empty());
    assert_eq!(state.closed_project_rows(), vec![2, 1, 0]);
    assert_eq!(state.search_projects("", true).len(), 3);
    assert_eq!(state.reopen_project(1), Some(1));
    assert_eq!(state.project_rows(), vec![1]);
}

#[test]
fn explicitly_opening_a_closed_project_reuses_its_context() {
    let mut state = workspace();
    state.projects[0].tab = Tab::Files;
    let root = state.projects[0].root.clone();
    state.close_project(0);
    state.open_directory(root);
    assert_eq!(state.active_project, 0);
    assert!(state.project_visible(0));
    assert_eq!(state.project().tab, Tab::Files);
    assert_eq!(state.projects.len(), 3);
}

#[test]
fn recently_closed_search_uses_close_order_even_for_favorites() {
    let mut state = workspace();
    state.toggle_project_favorite(0);
    state.close_project(0);
    state.close_project(1);
    assert_eq!(state.search_projects("", true)[0].owner, 1);
}
