use serde_json::{Value, json};
use tervia::agent::{Agent, hooks};

#[test]
fn integration_merges_and_removes_only_its_own_hooks_idempotently() {
    for agent in [Agent::Claude, Agent::Codex] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        let original = json!({"custom":"mock-kept", "hooks":{"Stop":[{"matcher":"mock-preserved", "hooks":[]}, {"hooks":[{"type":"command","command":"echo mock-existing-hook"}]}]}});
        std::fs::write(&path, original.to_string()).unwrap();
        hooks::configure(
            &path,
            std::path::Path::new("/mock/app with spaces/tervia"),
            agent,
            true,
        )
        .unwrap();
        let installed = std::fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&installed).unwrap();
        assert_eq!(value["custom"], "mock-kept");
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 3);
        assert!(installed.contains("mock-existing-hook"));
        assert!(installed.contains("--tervia-agent-hook"));
        assert!(value["hooks"].get("SubagentStop").is_none());
        hooks::configure(
            &path,
            std::path::Path::new("/mock/app with spaces/tervia"),
            agent,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), installed);
        hooks::configure(&path, std::path::Path::new("/mock/new/tervia"), agent, true).unwrap();
        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(updated["hooks"]["Stop"].as_array().unwrap().len(), 3);
        hooks::configure(
            &path,
            std::path::Path::new("/mock/new/tervia"),
            agent,
            false,
        )
        .unwrap();
        let restored: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(restored, original);
    }
}

#[test]
fn malformed_hook_configuration_is_not_overwritten_and_missing_files_can_be_created() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mock-user/hooks.json");
    let executable = std::path::Path::new("/mock/tervia");
    hooks::configure(&path, executable, Agent::Codex, false).unwrap();
    assert!(!path.exists());
    hooks::configure(&path, executable, Agent::Codex, true).unwrap();
    for invalid in [
        "invalid-json",
        r#"{"hooks":{"Stop":"mock-invalid"}}"#,
        r#"{"hooks":[]}"#,
    ] {
        std::fs::write(&path, invalid).unwrap();
        assert!(hooks::configure(&path, executable, Agent::Codex, true).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
    }
    assert!(hooks::configure(&path, executable, Agent::Gemini, true).is_err());
}

#[cfg(unix)]
#[test]
fn hook_configuration_preserves_dotfile_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("mock-source.json");
    let link = root.path().join("mock-settings.json");
    std::fs::write(&target, "{}").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    hooks::configure(
        &link,
        std::path::Path::new("/mock/tervia"),
        Agent::Claude,
        true,
    )
    .unwrap();
    assert!(link.is_symlink());
    assert!(
        std::fs::read_to_string(target)
            .unwrap()
            .contains("--tervia-agent-hook")
    );
}
