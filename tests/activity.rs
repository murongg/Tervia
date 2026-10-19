use serde_json::json;
use tervia::agent::{
    Agent,
    activity::{Event, Phase, Snapshot},
};

fn event(agent: Agent, name: &str) -> Event {
    Event::parse(
        agent,
        &json!({"session_id":"mock-session", "hook_event_name":name}),
    )
    .unwrap()
}

#[test]
fn official_events_distinguish_turns_permissions_and_recoverable_tool_errors() {
    for agent in [Agent::Claude, Agent::Codex] {
        for (name, phase) in [
            ("SessionStart", Phase::Ready),
            ("UserPromptSubmit", Phase::Working),
            ("PermissionRequest", Phase::Permission),
            ("PostToolUse", Phase::Working),
            ("Stop", Phase::Complete),
            ("SessionEnd", Phase::Exited),
        ] {
            assert_eq!(event(agent, name).phase, phase);
        }
        let question = json!({"session_id":"mock-session", "hook_event_name":"PreToolUse", "tool_name":"AskUserQuestion"});
        assert_eq!(Event::parse(agent, &question).unwrap().phase, Phase::Input);
    }
    assert_eq!(event(Agent::Codex, "Interrupt").phase, Phase::Interrupted);
    assert_eq!(event(Agent::Claude, "StopFailure").phase, Phase::Failed);
    assert_eq!(
        event(Agent::Claude, "PostToolUseFailure").phase,
        Phase::Working
    );
    assert!(
        Event::parse(
            Agent::Codex,
            &json!({"session_id":"mock-session", "hook_event_name":"StopFailure"})
        )
        .is_none()
    );
}

#[test]
fn subagents_unknown_events_and_content_are_never_treated_as_main_turn_completion() {
    for agent in [Agent::Claude, Agent::Codex] {
        for payload in [
            json!({"session_id":"mock-session", "hook_event_name":"SubagentStop"}),
            json!({"session_id":"mock-session", "hook_event_name":"Stop", "agent_id":"mock-child"}),
            json!({"session_id":"mock-session", "hook_event_name":"output", "message":"task complete"}),
            json!({"hook_event_name":"Stop"}),
        ] {
            assert!(Event::parse(agent, &payload).is_none());
        }
    }
    assert!(
        Event::parse(
            Agent::Gemini,
            &json!({"session_id":"mock-session", "hook_event_name":"Stop"})
        )
        .is_none()
    );
    let notice = |kind| json!({"session_id":"mock-session", "hook_event_name":"Notification", "notification_type":kind});
    assert_eq!(
        Event::parse(Agent::Claude, &notice("permission_prompt"))
            .unwrap()
            .phase,
        Phase::Permission
    );
    assert!(Event::parse(Agent::Claude, &notice("idle_prompt")).is_none());
    assert!(Event::parse(Agent::Claude, &notice("auth_success")).is_none());
}

#[test]
fn snapshots_keep_only_status_and_reject_other_sessions_until_a_new_start() {
    let payload = json!({"session_id":"mock-session", "hook_event_name":"UserPromptSubmit", "prompt":"synthetic-private-prompt", "transcript_path":"/mock/private.jsonl"});
    let working = Snapshot::advance(
        None,
        Event::parse(Agent::Claude, &payload).unwrap(),
        "mock-revision-1".into(),
    )
    .unwrap();
    let complete = Snapshot::advance(
        Some(&working),
        event(Agent::Claude, "Stop"),
        "mock-revision-2".into(),
    )
    .unwrap();
    assert_eq!(working.turn, complete.turn);
    assert_eq!(
        Snapshot::from_json(&complete.to_json()),
        Some(complete.clone())
    );
    let text = complete.to_json().to_string();
    assert!(!text.contains("synthetic-private-prompt") && !text.contains("private.jsonl"));
    let old = Event::parse(
        Agent::Claude,
        &json!({"session_id":"mock-other", "hook_event_name":"SessionEnd"}),
    )
    .unwrap();
    assert!(Snapshot::advance(Some(&complete), old, "mock-revision-3".into()).is_none());
    assert!(
        Phase::Permission.alerts()
            && Phase::Input.alerts()
            && Phase::Complete.alerts()
            && Phase::Failed.alerts()
    );
    assert!(!Phase::Working.alerts() && !Phase::Exited.alerts());
}

#[test]
fn delayed_codex_events_cannot_finish_a_newer_turn() {
    let payload =
        |name, turn| json!({"session_id":"mock-session", "hook_event_name":name, "turn_id":turn});
    let first = Snapshot::advance(
        None,
        Event::parse(Agent::Codex, &payload("UserPromptSubmit", "mock-turn-1")).unwrap(),
        "mock-1".into(),
    )
    .unwrap();
    let next = Snapshot::advance(
        Some(&first),
        Event::parse(Agent::Codex, &payload("UserPromptSubmit", "mock-turn-2")).unwrap(),
        "mock-2".into(),
    )
    .unwrap();
    assert!(
        Snapshot::advance(
            Some(&next),
            Event::parse(Agent::Codex, &payload("Stop", "mock-turn-1")).unwrap(),
            "mock-3".into()
        )
        .is_none()
    );
    assert_eq!(
        Snapshot::advance(
            Some(&next),
            Event::parse(Agent::Codex, &payload("Stop", "mock-turn-2")).unwrap(),
            "mock-4".into()
        )
        .unwrap()
        .phase,
        Phase::Complete
    );
}

#[test]
fn session_start_epoch_survives_coalesced_events_and_same_id_resume() {
    let old = Snapshot::advance(
        None,
        event(Agent::Claude, "UserPromptSubmit"),
        "mock-old-epoch".into(),
    )
    .unwrap();
    let resumed = Snapshot::advance(
        Some(&old),
        event(Agent::Claude, "SessionStart"),
        "mock-new-epoch".into(),
    )
    .unwrap();
    let working = Snapshot::advance(
        Some(&resumed),
        event(Agent::Claude, "UserPromptSubmit"),
        "mock-latest-event".into(),
    )
    .unwrap();
    assert_ne!(old.epoch, resumed.epoch);
    assert_eq!(working.epoch, resumed.epoch);
    assert_eq!(Snapshot::from_json(&working.to_json()), Some(working));
}

#[test]
fn atomic_recording_authenticates_the_owned_terminal_and_never_stores_hook_content() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("launch.json"),
        r#"{"agent_token":"mock-capability"}"#,
    )
    .unwrap();
    let payload = json!({"session_id":"mock-session", "hook_event_name":"Stop", "last_assistant_message":"synthetic-response-not-to-be-stored"});
    assert!(
        tervia::agent::activity::record(root.path(), "incorrect", Agent::Codex, &payload).is_err()
    );
    assert!(!root.path().join("agent.json").exists());
    assert!(
        tervia::agent::activity::record(root.path(), "mock-capability", Agent::Codex, &payload)
            .unwrap()
    );
    let result = tervia::agent::activity::read(root.path()).unwrap();
    assert_eq!(result.phase, Phase::Complete);
    assert!(
        !std::fs::read_to_string(root.path().join("agent.json"))
            .unwrap()
            .contains("synthetic-response")
    );
    std::fs::write(root.path().join("agent.json"), vec![b' '; 8193]).unwrap();
    assert!(tervia::agent::activity::read(root.path()).is_none());
}
