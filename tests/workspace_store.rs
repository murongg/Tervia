use std::{fs, path::PathBuf};
use tervia::{state::Workspace, workspace::Store};

#[test]
fn window_maximization_survives_workspace_saves_and_restarts() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    for maximized in [true, false] {
        fs::write(
            &path,
            serde_json::json!({
                "version": 1,
                "projects": [],
                "active": null,
                "window_maximized": maximized,
            })
            .to_string(),
        )
        .unwrap();
        for _ in 0..2 {
            let mut store = Store::new(path.clone());
            let mut state = store.load().unwrap();
            state.sidebar_expanded = !state.sidebar_expanded;
            store.save(&state).unwrap();
            let saved: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(saved["window_maximized"], maximized);
        }
    }
}

#[test]
fn catalog_round_trip_retains_favorites_order_recent_directories_and_closed_drafts() {
    use tervia::state::{File, Tab};
    let temporary = tempfile::tempdir().unwrap();
    let alpha = directory(&temporary, "mock-alpha");
    let beta = directory(&temporary, "mock-beta");
    let gamma = directory(&temporary, "mock-gamma");
    fs::create_dir(alpha.join("mock-web")).unwrap();
    let child = alpha.join("mock-web");
    let mut state = Workspace::new(alpha.clone());
    state.open_directory(beta.clone());
    state.open_directory(gamma.clone());
    state.move_project(2, 0);
    state.toggle_project_favorite(1);
    state.switch_project(1);
    let index = state.register_directory(child.clone());
    state.attach_directory(0, index);
    state.switch_project(index);
    state.project_mut().tab = Tab::Files;
    state.project_mut().files.push(File {
        path: "mock.rs".into(),
        disk_path: "mock.rs".into(),
        language: "rust".into(),
        saved: "base".into(),
        draft: "closed synthetic draft".into(),
    });
    state.project_mut().open_file(0);
    state.close_project(0);
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let mut restored = Store::new(path.clone()).load().unwrap();
    let rows: Vec<_> = restored
        .project_rows()
        .iter()
        .map(|index| restored.project_root(*index).clone())
        .collect();
    assert_eq!(rows, vec![beta.clone(), gamma]);
    assert!(restored.project_favorite(restored.project_rows()[0]));
    let closed = restored.closed_project_rows()[0];
    assert_eq!(restored.project_root(closed), &alpha);
    let target = restored.reopen_project(closed).unwrap();
    assert_eq!(restored.projects[target].root, child);
    assert_eq!(
        restored.project().current_file().unwrap().draft,
        "closed synthetic draft"
    );
    assert_eq!(restored.project().tab, Tab::Files);
    let previous = restored.previous_project().unwrap();
    assert_eq!(restored.projects[previous].root, beta);
    for owner in restored.project_rows() {
        restored.close_project(owner);
    }
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&restored).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert!(
        restored.project_rows().is_empty(),
        "Recovery must not reopen closed projects"
    );
    assert_eq!(restored.closed_project_rows().len(), 3);
}

#[test]
fn malformed_catalog_preserves_the_existing_recovery_file() {
    let temporary = tempfile::tempdir().unwrap();
    let root = directory(&temporary, "mock-project");
    let state = Workspace::new(root);
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let mut saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    saved["catalog"] = serde_json::json!({"favorites": true});
    let source = serde_json::to_string(&saved).unwrap();
    fs::write(&path, &source).unwrap();
    let mut store = Store::new(path.clone());
    assert!(store.load().is_err());
    assert!(store.save(&state).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn closed_linked_worktrees_stay_one_closed_project_after_restart_before_git_discovery() {
    use tervia::state::File;
    let temporary = tempfile::tempdir().unwrap();
    let main = directory(&temporary, "mock-main");
    let linked = directory(&temporary, "mock-linked");
    let mut state = Workspace::new(main.clone());
    state.associate_repository(0, main.join(".git"), Some(main.clone()));
    state.projects[0].files.push(File {
        path: "mock.rs".into(),
        disk_path: "mock.rs".into(),
        language: "rust".into(),
        saved: "base".into(),
        draft: "synthetic main draft".into(),
    });
    state.projects[0].open_file(0);
    let worktree = state.open_directory(linked.clone());
    state.associate_repository(worktree, main.join(".git"), None);
    state.close_project(0);
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let mut restored = Store::new(path).load().unwrap();
    assert!(restored.project_rows().is_empty());
    assert_eq!(restored.closed_project_rows().len(), 1);
    let owner = restored.closed_project_rows()[0];
    let target = restored.reopen_project(owner).unwrap();
    assert_eq!(restored.projects[target].root, linked);
    assert_eq!(restored.project_rows().len(), 1);
    let main = restored
        .projects
        .iter()
        .find(|project| project.root == main)
        .unwrap();
    assert_eq!(main.current_file().unwrap().draft, "synthetic main draft");
}

#[test]
fn reopened_project_retains_new_worktrees_and_its_latest_directory_on_restart() {
    let temporary = tempfile::tempdir().unwrap();
    let main = directory(&temporary, "mock-main");
    let linked = directory(&temporary, "mock-linked");
    let newer = directory(&temporary, "mock-newer");
    let identity = main.join(".git");
    let mut state = Workspace::new(main.clone());
    state.associate_repository(0, identity.clone(), Some(main));
    let linked_index = state.open_directory(linked);
    state.associate_repository(linked_index, identity.clone(), None);
    state.close_project(0);
    state.reopen_project(0);
    let newer_index = state.open_directory(newer.clone());
    state.associate_repository(newer_index, identity, None);
    assert_eq!(state.project_target(0), newer_index);
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.project_rows().len(), 1);
    assert_eq!(
        restored.projects[restored.project_target(restored.project_rows()[0])].root,
        newer
    );
}

#[test]
fn recovery_keeps_tabs_layout_drafts_and_reading_positions_without_overwriting_disk() {
    use tervia::state::{File, ReadingPosition, Tab};
    let temporary = tempfile::tempdir().unwrap();
    let root = directory(&temporary, "mock-recovery");
    let path = temporary.path().join("workspace.json");
    fs::write(root.join("mock.rs"), "base\n").unwrap();
    fs::write(root.join("clean.rs"), "clean\n").unwrap();
    let mut state = Workspace::new(root.clone());
    state.new_session();
    state.new_session();
    let project = state.project_mut();
    project.tab = Tab::Files;
    project.dock.visible = true;
    project.dock.size = 260;
    project.files = vec![
        File {
            path: "mock.rs".into(),
            disk_path: "mock.rs".into(),
            language: "rust".into(),
            saved: "base\n".into(),
            draft: "unsaved synthetic\n".into(),
        },
        File {
            path: "clean.rs".into(),
            disk_path: "clean.rs".into(),
            language: "rust".into(),
            saved: "clean\n".into(),
            draft: "clean\n".into(),
        },
    ];
    project.open_files = vec![1, 0];
    project.active_file = 0;
    project.positions.insert(
        "mock.rs".into(),
        ReadingPosition {
            line: 4,
            column: 3,
            scroll_x: 0.0,
            scroll_y: -80.0,
            source: true,
        },
    );
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    fs::write(root.join("mock.rs"), "external synthetic\n").unwrap();
    let restored = Store::new(path.clone()).load().unwrap();
    let project = restored.project();
    assert_eq!(project.tab, Tab::Files);
    assert_eq!(project.sessions.len(), 3);
    assert_eq!(project.active_session, 2);
    assert!(project.dock.visible);
    assert_eq!(project.dock.size, 260);
    let file = project
        .files
        .iter()
        .find(|file| file.disk_path == std::path::Path::new("mock.rs"))
        .unwrap();
    assert_eq!(file.draft, "unsaved synthetic\n");
    assert_eq!(file.saved, "base\n");
    assert_eq!(project.files[project.active_file].path, "mock.rs");
    assert_eq!(project.positions[std::path::Path::new("mock.rs")].line, 4);
    assert!(project.restoring.contains(std::path::Path::new("clean.rs")));
    assert!(tervia::files::write(&root, &file.disk_path, &file.draft, &file.saved).is_err());
    assert_eq!(
        fs::read_to_string(root.join("mock.rs")).unwrap(),
        "external synthetic\n"
    );
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["version"], 2);
}

#[test]
fn unavailable_directories_cannot_cause_recovery_drafts_to_be_discarded_on_save() {
    use tervia::state::File;
    let temporary = tempfile::tempdir().unwrap();
    let root = directory(&temporary, "mock-unavailable");
    let path = temporary.path().join("workspace.json");
    let mut state = Workspace::new(root.clone());
    state.project_mut().files.push(File {
        path: "mock.rs".into(),
        disk_path: "mock.rs".into(),
        language: "rust".into(),
        saved: "base".into(),
        draft: "synthetic recovery draft".into(),
    });
    state.project_mut().open_files.push(0);
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    fs::remove_dir(&root).unwrap();
    let mut store = Store::new(path.clone());
    let mut restored = store.load().unwrap();
    restored.open_directory(directory(&temporary, "mock-other"));
    store.save(&restored).unwrap();
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("synthetic recovery draft")
    );
    fs::create_dir(&root).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert!(restored.projects.iter().any(|project| {
        project
            .files
            .iter()
            .any(|file| file.draft == "synthetic recovery draft")
    }));
}

#[test]
fn aliased_recovery_directories_are_rejected_instead_of_overwriting_a_draft() {
    let temporary = tempfile::tempdir().unwrap();
    let root = directory(&temporary, "mock-alias");
    fs::create_dir(root.join("sub")).unwrap();
    let path = temporary.path().join("workspace.json");
    let context = |root: PathBuf, draft: &str| {
        serde_json::json!({
            "root": root, "sessions": ["shell"], "files": [{"path": "mock.rs", "saved": "base", "draft": draft}]
        })
    };
    let source = serde_json::json!({
        "version": 2, "projects": [{"root": root, "directory": root}], "active": root,
        "contexts": [context(root.clone(), "synthetic first draft"), context(root.join("sub/.."), "synthetic second draft")]
    }).to_string();
    fs::write(&path, &source).unwrap();
    let mut store = Store::new(path.clone());
    assert!(store.load().is_err());
    assert!(store.save(&Workspace::new(root)).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

fn directory(root: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = root.path().join(name);
    fs::create_dir(&path).unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn project_order_selection_and_sidebar_survive_a_new_store_instance() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("state/workspace.json");
    let alpha = directory(&root, "mock-alpha");
    let beta = directory(&root, "mock-beta");
    let mut store = Store::new(path.clone());
    assert!(store.load().unwrap().projects.is_empty());
    assert!(!path.exists());
    let mut state = Workspace::new(alpha.clone());
    state.open_directory(beta.clone());
    state.switch_project(0);
    state.sidebar_expanded = true;
    store.save(&state).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(
        restored
            .projects
            .iter()
            .map(|project| &project.root)
            .collect::<Vec<_>>(),
        vec![&alpha, &beta]
    );
    assert_eq!(restored.project().root, alpha);
    assert!(restored.sidebar_expanded);
    assert_eq!(restored.project().sessions.len(), 1);
    assert!(restored.project().files.is_empty());
}

#[test]
fn missing_directories_are_skipped_and_selection_falls_back_to_an_available_project() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let alpha = directory(&root, "mock-alpha");
    let beta = directory(&root, "mock-beta");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    let mut state = Workspace::new(alpha.clone());
    state.open_directory(beta.clone());
    store.save(&state).unwrap();
    fs::remove_dir(&beta).unwrap();
    let restored = Store::new(path.clone()).load().unwrap();
    assert_eq!(restored.projects.len(), 1);
    assert_eq!(restored.project().root, alpha);
    fs::remove_dir(&alpha).unwrap();
    assert!(Store::new(path).load().unwrap().projects.is_empty());
}

#[test]
fn folder_history_restores_one_parent_its_selected_child_and_its_folded_state() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    let parent = directory(&temporary, "mock-folder");
    let child = parent.join("mock-child");
    fs::create_dir(&child).unwrap();
    let mut state = Workspace::new(parent.clone());
    let child_index = state.register_directory(child.clone());
    state.group_folder(0, &[child_index]);
    state.switch_project(child_index);
    state.toggle_folder(0);
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["projects"].as_array().unwrap().len(), 1);
    assert_eq!(saved["projects"][0]["root"], serde_json::json!(parent));
    assert_eq!(saved["projects"][0]["directory"], serde_json::json!(child));
    let mut restored_store = Store::new(path.clone());
    let mut restored = restored_store.load().unwrap();
    assert_eq!(restored.project_rows(), vec![0]);
    assert_eq!(restored.project().root, child);
    assert_eq!(restored.folder(restored.active_project), Some(0));
    assert!(!restored.folder_expanded(0));
    restored.sidebar_expanded = true;
    restored_store.save(&restored).unwrap();
    fs::remove_dir(&child).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.project_rows(), vec![0]);
    assert_eq!(restored.project().root, parent);
}

#[test]
fn explicitly_added_working_directories_survive_restart_without_scanned_candidates() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    let parent = directory(&temporary, "mock-project");
    let alpha = parent.join("mock-alpha");
    let beta = parent.join("mock-beta");
    let scanned = parent.join("mock-scanned");
    for child in [&alpha, &beta, &scanned] {
        fs::create_dir(child).unwrap();
    }
    let mut state = Workspace::new(parent.clone());
    for child in [&alpha, &beta] {
        let index = state.register_directory(child.clone());
        state.attach_directory(0, index);
    }
    let candidate = state.register_directory(scanned.clone());
    state.group_folder(0, &[candidate]);
    state.switch_project(0);
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        saved["projects"][0]["directories"],
        serde_json::json!([parent, alpha, beta])
    );
    let restored = Store::new(path.clone()).load().unwrap();
    assert_eq!(restored.project_rows(), vec![0]);
    assert!(
        restored
            .projects
            .iter()
            .any(|project| project.root == alpha)
    );
    assert!(restored.projects.iter().any(|project| project.root == beta));
    assert!(
        !restored
            .projects
            .iter()
            .any(|project| project.root == scanned)
    );
    fs::remove_dir(&beta).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.project().root, parent);
    assert!(
        restored
            .projects
            .iter()
            .any(|project| project.root == alpha)
    );
    assert!(!restored.projects.iter().any(|project| project.root == beta));
}

#[test]
fn legacy_folder_history_keeps_its_selected_directory_available_after_returning_to_root() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    let parent = directory(&temporary, "mock-project");
    let child = parent.join("mock-child");
    fs::create_dir(&child).unwrap();
    fs::write(
        &path,
        serde_json::json!({
            "version": 1,
            "projects": [{"root": parent, "directory": child, "folder": true}],
            "active": child
        })
        .to_string(),
    )
    .unwrap();
    let mut restored = Store::new(path).load().unwrap();
    let child = restored.active_project;
    restored.switch_project(0);
    assert_eq!(restored.working_directories(0), vec![0, child]);
}

#[test]
fn worktree_groups_remember_the_last_directory_and_fall_back_when_it_is_removed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let main = directory(&root, "mock-main");
    let linked = directory(&root, "mock-linked");
    let retired = directory(&root, "mock-retired");
    let other = directory(&root, "mock-other");
    let mut state = Workspace::new(main.clone());
    state.associate_repository(0, main.join(".git"), Some(main.clone()));
    let index = state.open_directory(linked.clone());
    state.associate_repository(index, main.join(".git"), None);
    let index = state.register_directory(retired);
    state.retire_project(index);
    state.open_directory(other.clone());
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let restored = Store::new(path.clone()).load().unwrap();
    assert_eq!(restored.projects.len(), 2);
    assert_eq!(restored.projects[0].root, linked);
    assert_eq!(restored.project().root, other);
    let mut restored = restored;
    restored.associate_repository(0, main.join(".git"), Some(main.clone()));
    restored.sidebar_expanded = true;
    let mut restarted = Store::new(path.clone());
    restarted.load().unwrap();
    restarted.save(&restored).unwrap();
    fs::remove_dir(&linked).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.projects[0].root, main);
    assert_eq!(restored.project().root, other);
}

#[test]
fn restored_worktree_fallback_survives_saving_before_git_metadata_loads() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let main = directory(&root, "mock-main");
    let linked = directory(&root, "mock-linked");
    let mut state = Workspace::new(main.clone());
    state.associate_repository(0, main.join(".git"), Some(main.clone()));
    let index = state.open_directory(linked.clone());
    state.associate_repository(index, main.join(".git"), None);
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&state).unwrap();
    let mut restarted = Store::new(path.clone());
    let mut restored = restarted.load().unwrap();
    restored.sidebar_expanded = true;
    restarted.save(&restored).unwrap();
    fs::remove_dir(&linked).unwrap();
    assert_eq!(Store::new(path).load().unwrap().project().root, main);
}

#[test]
fn invalid_state_is_reported_and_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    for source in [
        "{invalid",
        r#"{"version": 99, "projects": []}"#,
        r#"{"version": 1, "projects": [{"root": "relative", "directory": "relative"}]}"#,
        r#"{"version": 1, "projects": "invalid"}"#,
        r#"{"version": 1, "projects": [], "window_maximized": "invalid"}"#,
    ] {
        fs::write(&path, source).unwrap();
        let mut store = Store::new(path.clone());
        assert!(store.load().err().unwrap().contains("workspace.json"));
        assert!(store.save(&Workspace::empty()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn concurrent_instances_cannot_replace_each_others_project_lists() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let alpha = directory(&root, "mock-alpha");
    let beta = directory(&root, "mock-beta");
    let mut first = Store::new(path.clone());
    let mut second = Store::new(path.clone());
    first.load().unwrap();
    second.load().unwrap();
    first.save(&Workspace::new(alpha.clone())).unwrap();
    assert!(second.save(&Workspace::new(beta)).is_err());
    assert_eq!(Store::new(path).load().unwrap().project().root, alpha);
}

#[test]
fn duplicate_directory_aliases_restore_one_project_and_keep_the_selection() {
    let root = tempfile::tempdir().unwrap();
    let alpha = directory(&root, "mock-alpha");
    let alias = alpha.join(".");
    let path = root.path().join("workspace.json");
    fs::write(
        &path,
        serde_json::json!({
            "version": 1,
            "projects": [
                { "root": alpha, "directory": alpha },
                { "root": alias, "directory": alias }
            ],
            "active": alias
        })
        .to_string(),
    )
    .unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.projects.len(), 1);
    assert_eq!(restored.project().root, alpha);
}

#[test]
fn save_failures_preserve_the_previous_state_file() {
    let root = tempfile::tempdir().unwrap();
    let project = directory(&root, "mock-alpha");
    let blocked = root.path().join("blocked");
    fs::write(&blocked, "synthetic file").unwrap();
    let mut blocked_store = Store::new(blocked.join("workspace.json"));
    // Windows can report a blocked parent as NotFound, deferring the error until saving.
    let _ = blocked_store.load();
    assert!(
        blocked_store
            .save(&Workspace::new(project.clone()))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&blocked).unwrap(), "synthetic file");
    let path = root.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    fs::write(&path, "synthetic external change").unwrap();
    assert!(store.save(&Workspace::new(project)).is_err());
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "synthetic external change"
    );
}

#[cfg(unix)]
#[test]
fn updating_history_preserves_its_symlink_and_updates_the_target() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("mock-history.json");
    let link = root.path().join("workspace.json");
    let alpha = directory(&root, "mock-alpha");
    let beta = directory(&root, "mock-beta");
    let mut original = Store::new(target.clone());
    original.load().unwrap();
    original.save(&Workspace::new(alpha)).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut store = Store::new(link.clone());
    let mut state = store.load().unwrap();
    state.open_directory(beta.clone());
    store.save(&state).unwrap();
    assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    assert_eq!(Store::new(target).load().unwrap().project().root, beta);
}
