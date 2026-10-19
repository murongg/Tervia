//! Managed hook configuration, installed only through an explicit settings action.
use super::Agent;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const FLAG: &str = "--tervia-agent-hook";

fn provider(agent: Agent) -> io::Result<&'static str> {
    match agent {
        Agent::Claude => Ok("claude"),
        Agent::Codex => Ok("codex"),
        _ => Err(io::Error::other("This agent has no lifecycle integration")),
    }
}

pub fn path(agent: Agent) -> io::Result<PathBuf> {
    let (override_key, directory, filename) = match agent {
        Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude", "settings.json"),
        Agent::Codex => ("CODEX_HOME", ".codex", "hooks.json"),
        _ => return Err(io::Error::other("Unsupported agent integration")),
    };
    let root = std::env::var_os(override_key)
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(directory)))
        .ok_or_else(|| io::Error::other("Cannot find the agent configuration directory"))?;
    if !root.is_absolute() {
        return Err(io::Error::other(
            "Agent configuration directory must be absolute",
        ));
    }
    Ok(root.join(filename))
}

fn events(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "PostToolUseFailure",
            "Notification",
            "Elicitation",
            "ElicitationResult",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ],
        Agent::Codex => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "Stop",
            "Interrupt",
            "SessionEnd",
        ],
        _ => &[],
    }
}

fn managed(value: &Value, agent: &str) -> bool {
    value["type"] == "command"
        && value["command"]
            .as_str()
            .and_then(shlex::split)
            .is_some_and(|words| words.len() == 3 && words[1] == FLAG && words[2] == agent)
}

pub fn configure(path: &Path, executable: &Path, agent: Agent, enabled: bool) -> io::Result<()> {
    let agent_id = provider(agent)?;
    let original = match fs::File::open(path) {
        Ok(file) => {
            if !file.metadata()?.is_file() {
                return Err(io::Error::other(
                    "Agent configuration must be a regular file",
                ));
            }
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(io::Error::other("Agent configuration exceeds 1 MiB"));
            }
            Some(bytes)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if !enabled && original.is_none() {
        return Ok(());
    }
    let mut value: Value = match &original {
        Some(bytes) => serde_json::from_slice(bytes)?,
        None => json!({}),
    };
    let object = value
        .as_object_mut()
        .ok_or_else(|| io::Error::other("Agent settings must be an object"))?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| io::Error::other("Agent hooks must be an object"))?;
    let executable = executable
        .to_str()
        .filter(|path| !path.chars().any(char::is_control))
        .ok_or_else(|| io::Error::other("Invalid application path"))?;
    // Claude uses Bash for command hooks on Windows too; forward slashes work in both clients.
    let executable = if cfg!(windows) {
        executable.replace('\\', "/")
    } else {
        executable.to_owned()
    };
    let command =
        shlex::try_join([executable.as_str(), FLAG, agent_id]).map_err(io::Error::other)?;
    for event in events(agent) {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| io::Error::other(format!("hooks.{event} must be an array")))?;
        let mut preserved = Vec::new();
        for mut group in std::mem::take(groups) {
            let handlers = group
                .get_mut("hooks")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    io::Error::other(format!("hooks.{event} must contain hook arrays"))
                })?;
            let owned = handlers.iter().any(|handler| managed(handler, agent_id));
            handlers.retain(|handler| !managed(handler, agent_id));
            if !owned || !handlers.is_empty() {
                preserved.push(group);
            }
        }
        *groups = preserved;
        if enabled {
            let mut handler = json!({"type":"command", "command":command, "timeout":1});
            if cfg!(windows) && agent == Agent::Codex {
                handler["commandWindows"] = json!(format!(
                    "& '{}' {FLAG} {agent_id}",
                    executable.replace('\'', "''")
                ));
            }
            groups.push(json!({"hooks":[handler]}));
        }
        if groups.is_empty() {
            hooks.remove(*event);
        }
    }
    if hooks.is_empty() {
        object.remove("hooks");
    }
    let updated = serde_json::to_vec_pretty(&value)?;
    if original.as_ref() == Some(&updated) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let target = if original.is_some() {
        fs::canonicalize(path)?
    } else {
        path.to_owned()
    };
    let mut temporary = tempfile::NamedTempFile::new_in(
        target
            .parent()
            .ok_or_else(|| io::Error::other("Agent settings need a parent directory"))?,
    )?;
    if original.is_some() {
        temporary
            .as_file()
            .set_permissions(fs::metadata(&target)?.permissions())?;
    }
    temporary.write_all(&updated)?;
    temporary.as_file().sync_all()?;
    // Preserve concurrent edits just as Tervia's own preferences do.
    if fs::read(path).ok() != original {
        return Err(io::Error::other(
            "Agent settings changed; retry the integration action",
        ));
    }
    if original.is_some() {
        temporary.persist(target).map_err(|error| error.error)?;
    } else {
        temporary
            .persist_noclobber(target)
            .map_err(|error| error.error)?;
    }
    Ok(())
}

/// Hooks always return an empty successful decision, including outside Tervia.
pub fn dispatch() -> bool {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(FLAG)) {
        return false;
    }
    if let Some(agent) = arguments
        .next()
        .and_then(|name| name.to_str().and_then(Agent::from_executable))
    {
        let _ = super::activity::run_from_env(agent, std::io::stdin().lock());
    }
    // Stop requires valid JSON; printing errors or lifecycle data could alter the agent's context.
    println!("{{}}");
    true
}
