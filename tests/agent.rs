use tervia::agent::Agent;

#[test]
fn native_agents_and_node_wrappers_are_recognized_from_executable_paths() {
    for path in [
        "/mock/bin/claude",
        "/mock/share/claude/versions/9.9.9",
        "C:\\mock\\claude.exe",
    ] {
        assert_eq!(Agent::from_executable(path), Some(Agent::Claude));
    }
    for path in [
        "/mock/bin/codex",
        "/mock/node_modules/@openai/codex/vendor/mock/codex/codex",
        "C:\\mock\\codex.exe",
    ] {
        assert_eq!(Agent::from_executable(path), Some(Agent::Codex));
    }
    assert_eq!(
        Agent::from_foreground(["/mock/bin/node", "/mock/vendor/codex"]),
        Some(Agent::Codex)
    );
}

#[test]
fn similar_names_and_shell_output_cannot_claim_an_agent_identity() {
    for path in [
        "/mock/bin/zsh",
        "/mock/bin/claude-helper",
        "/mock/bin/codex-tool",
        "/mock/claude/readme",
        "/mock/share/versions/9.9.9",
    ] {
        assert_eq!(Agent::from_executable(path), None);
    }
    assert_eq!(Agent::from_foreground(["/mock/bin/zsh"]), None);
    assert_eq!(
        Agent::from_executable("/mock/bin/opencode"),
        Some(Agent::OpenCode)
    );
    assert_eq!(
        Agent::from_argv(
            "/mock/bin/node",
            ["node", "/mock/node_modules/openclaw/dist/entry.js"]
        ),
        Some(Agent::OpenClaw)
    );
}

#[test]
fn javascript_and_python_launchers_identify_the_entrypoint_not_prompt_arguments() {
    assert_eq!(
        Agent::from_argv(
            "/mock/Python.app/Contents/MacOS/Python",
            ["Python", "/mock/venv/bin/hermes"]
        ),
        Some(Agent::Hermes)
    );
    assert_eq!(
        Agent::from_argv(
            "/mock/bin/node",
            [
                "node",
                "/mock/node_modules/@google/gemini-cli/dist/index.js"
            ]
        ),
        Some(Agent::Gemini)
    );
    assert_eq!(
        Agent::from_argv(
            "/mock/bin/bun",
            ["bun", "/mock/node_modules/grok-cli/dist/index.js"]
        ),
        Some(Agent::Grok)
    );
    assert_eq!(
        Agent::from_argv("/mock/bin/python3", ["python3", "/mock/venv/bin/hermes"]),
        Some(Agent::Hermes)
    );
    assert_eq!(
        Agent::from_argv("/mock/bin/python3.13", ["python3", "-m", "hermes_cli.main"]),
        Some(Agent::Hermes)
    );
    assert_eq!(
        Agent::from_argv("/mock/bin/node", ["node", "-e", "gemini"]),
        None
    );
    assert_eq!(
        Agent::from_argv("/mock/bin/python3", ["python3", "-c", "hermes"]),
        None
    );
    assert_eq!(
        Agent::from_argv("/mock/bin/node", ["node", "/mock/other.js", "gemini"]),
        None
    );
}

#[test]
fn native_and_script_identity_do_not_consume_later_arguments() {
    let arguments = [
        "node",
        "/mock/node_modules/@google/gemini-cli/dist/index.js",
    ]
    .into_iter()
    .chain(std::iter::once_with(|| {
        panic!("must not inspect prompt arguments")
    }));
    assert_eq!(
        Agent::from_argv("/mock/bin/node", arguments),
        Some(Agent::Gemini)
    );
    assert_eq!(
        Agent::from_argv(
            "/mock/bin/grok",
            std::iter::once_with(|| -> &'static str {
                panic!("native program needs no arguments")
            })
        ),
        Some(Agent::Grok)
    );
}
