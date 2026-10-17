use tervia::state::{Screen, Tab, Workspace};

#[test]
fn welcome_and_local_terminal_do_not_register_a_project() {
    let mut state = Workspace::empty();
    assert_eq!(state.screen, Screen::Welcome);
    assert!(state.local_terminal.sessions.is_empty());
    state.show_local_terminal();
    assert_eq!(state.screen, Screen::Local);
    assert_eq!(state.terminals().sessions.len(), 1);
    assert!(state.projects.is_empty());
    assert!(state.project_rows().is_empty());
    assert_eq!(state.active_tab(), Tab::Terminal);
    state.set_tab(Tab::Git);
    assert_eq!(state.active_tab(), Tab::Terminal);
}

#[test]
fn local_and_project_sessions_keep_their_own_selection_and_split() {
    let mut state = Workspace::new("/synthetic/mock-project".into());
    state.set_tab(Tab::Files);
    state.new_session();
    state.show_local_terminal();
    state.new_session();
    state.new_session();
    state.toggle_split();
    state.focus_session(0);
    assert_eq!(state.command_session(), 0);
    state.switch_project(0);
    assert_eq!(state.screen, Screen::Project);
    assert_eq!(state.active_tab(), Tab::Files);
    assert_eq!(state.terminals().sessions.len(), 2);
    assert!(!state.terminals().split_terminal);
    assert_eq!(state.command_session(), 1);
    state.show_local_terminal();
    assert_eq!(state.terminals().sessions.len(), 3);
    assert!(state.terminals().split_terminal);
    assert_eq!(state.command_session(), 0);
}

#[test]
fn closing_last_local_session_keeps_an_empty_local_surface() {
    let mut state = Workspace::empty();
    state.show_local_terminal();
    state.close_session(0);
    assert!(state.terminals().sessions.is_empty());
    assert_eq!(state.command_session(), 0);
    assert_eq!(state.screen, Screen::Local);
    state.show_local_terminal();
    assert_eq!(state.terminals().sessions.len(), 1);
    assert_eq!(state.command_session(), 0);
}

#[test]
fn closing_a_project_keeps_local_sessions_and_the_current_screen() {
    let mut state = Workspace::new("/synthetic/mock-alpha".into());
    state.open_directory("/synthetic/mock-beta".into());
    state.show_local_terminal();
    state.new_session();
    state.close_project(1).unwrap();
    assert_eq!(state.screen, Screen::Local);
    assert_eq!(state.local_terminal.sessions.len(), 2);
    state.switch_project(0);
    state.close_project(0).unwrap();
    assert_eq!(state.screen, Screen::Local);
    assert_eq!(state.local_terminal.sessions.len(), 2);
    assert!(state.project_rows().is_empty());
}

#[test]
fn closing_the_last_project_without_local_sessions_returns_to_welcome() {
    let mut state = Workspace::new("/synthetic/mock-project".into());
    state.close_project(0).unwrap();
    assert_eq!(state.screen, Screen::Welcome);
    state.reopen_project(0).unwrap();
    assert_eq!(state.screen, Screen::Project);
    state.show_welcome();
    assert_eq!(state.screen, Screen::Welcome);
    assert_eq!(state.project_rows(), vec![0]);
    state.switch_project(0);
    assert_eq!(state.screen, Screen::Project);
}

#[test]
fn startup_restores_local_sessions_without_replacing_project_context() {
    use std::fs;
    use tervia::workspace::Store;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("mock-project");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    let mut state = store.load().unwrap();
    state.open_directory(root.clone());
    state.set_tab(Tab::Files);
    state.new_session();
    state.show_local_terminal();
    state.new_session();
    state.toggle_split();
    state.focus_session(0);
    state.local_terminal.sessions[0].name = "mock-local-shell".into();
    store.save(&state).unwrap();
    let mut restored = Store::new(path.clone()).load().unwrap();
    assert_eq!(restored.screen, Screen::Local);
    assert_eq!(restored.local_terminal.sessions.len(), 2);
    assert_eq!(restored.local_terminal.sessions[0].name, "mock-local-shell");
    assert_eq!(restored.local_terminal.active_session, 1);
    assert_eq!(restored.command_session(), 0);
    assert!(restored.local_terminal.split_terminal);
    assert_eq!(restored.projects.len(), 1);
    assert_eq!(restored.projects[0].root, root);
    assert_eq!(restored.projects[0].terminal.sessions.len(), 2);
    assert_eq!(restored.projects[0].tab, Tab::Files);
    restored.show_welcome();
    let mut store = Store::new(path.clone());
    store.load().unwrap();
    store.save(&restored).unwrap();
    assert_eq!(Store::new(path).load().unwrap().screen, Screen::Welcome);
}

#[test]
fn local_only_and_empty_local_sessions_survive_restart_without_a_project() {
    use tervia::workspace::Store;
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    let mut state = store.load().unwrap();
    state.show_local_terminal();
    store.save(&state).unwrap();
    let mut restored = Store::new(path.clone()).load().unwrap();
    assert_eq!(restored.screen, Screen::Local);
    assert!(restored.projects.is_empty());
    restored.close_session(0);
    store.save(&restored).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.screen, Screen::Local);
    assert_eq!(restored.local_terminal.sessions.len(), 1);
}

#[test]
fn invalid_local_recovery_is_rejected_and_never_overwritten() {
    use serde_json::json;
    use std::fs;
    use tervia::workspace::Store;
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("workspace.json");
    for local in [
        json!({"sessions": ["mock"], "active_session": 1}),
        json!({"sessions": vec!["mock"; 65]}),
        json!({"sessions": [null]}),
        json!({"sessions": ["mock"], "split_terminal": "invalid"}),
    ] {
        let saved = json!({"version": 2, "projects": [], "active": null,
            "startup": {"screen": "local", "local_terminal": local}})
        .to_string();
        fs::write(&path, &saved).unwrap();
        let mut store = Store::new(path.clone());
        assert!(store.load().is_err());
        assert!(store.save(&Workspace::empty()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    }
}

#[test]
fn a_missing_selected_project_returns_to_welcome_with_local_sessions_retained() {
    use std::fs;
    use tervia::workspace::Store;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("mock-project");
    fs::create_dir(&root).unwrap();
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    let mut state = store.load().unwrap();
    state.show_local_terminal();
    state.open_directory(root.canonicalize().unwrap());
    store.save(&state).unwrap();
    fs::remove_dir(&root).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.screen, Screen::Welcome);
    assert_eq!(restored.local_terminal.sessions.len(), 1);
    assert!(!restored.notice.is_empty());
}

#[test]
fn unavailable_last_project_is_not_silently_replaced_by_another_project() {
    use std::fs;
    use tervia::workspace::Store;
    let temporary = tempfile::tempdir().unwrap();
    let alpha = temporary.path().join("mock-alpha");
    let beta = temporary.path().join("mock-beta");
    fs::create_dir(&alpha).unwrap();
    fs::create_dir(&beta).unwrap();
    let path = temporary.path().join("workspace.json");
    let mut store = Store::new(path.clone());
    let mut state = store.load().unwrap();
    state.open_directory(alpha.canonicalize().unwrap());
    state.open_directory(beta.canonicalize().unwrap());
    store.save(&state).unwrap();
    fs::remove_dir(&beta).unwrap();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.screen, Screen::Welcome);
    assert_eq!(restored.projects.len(), 1);
    assert!(!restored.notice.is_empty());
}

#[test]
fn new_local_sessions_stop_at_the_recoverable_limit() {
    let mut state = Workspace::empty();
    state.show_local_terminal();
    for _ in 0..65 {
        state.new_session();
    }
    assert_eq!(state.local_terminal.sessions.len(), 64);
    assert_eq!(state.local_terminal.active_session, 63);
    assert!(!state.notice.is_empty());
}
