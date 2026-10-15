use std::{fs, path::PathBuf};
use tervia::{state::Workspace, workspace::Store};

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
