//! Explicit lifecycle events are separate from process identity and terminal output.
use super::Agent;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Working,
    Permission,
    Input,
    Complete,
    Interrupted,
    Failed,
    Exited,
}

impl Phase {
    pub fn alerts(self) -> bool {
        matches!(
            self,
            Self::Permission | Self::Input | Self::Complete | Self::Failed
        )
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Ready => "agent_ready",
            Self::Working => "agent_working",
            Self::Permission => "agent_permission",
            Self::Input => "agent_input",
            Self::Complete => "agent_complete",
            Self::Interrupted => "agent_interrupted",
            Self::Failed => "agent_failed",
            Self::Exited => "agent_exited",
        }
    }
    fn parse(key: &str) -> Option<Self> {
        [
            Self::Ready,
            Self::Working,
            Self::Permission,
            Self::Input,
            Self::Complete,
            Self::Interrupted,
            Self::Failed,
            Self::Exited,
        ]
        .into_iter()
        .find(|phase| phase.key() == key)
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub agent: Agent,
    pub phase: Phase,
    session: String,
    turn: Option<String>,
    start: bool,
    new_turn: bool,
}

fn identifier(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
}

impl Event {
    pub fn parse(agent: Agent, value: &Value) -> Option<Self> {
        if !matches!(agent, Agent::Claude | Agent::Codex)
            || value
                .get("agent_id")
                .is_some_and(|id| !id.is_null() && id != "")
        {
            return None;
        }
        let name = value["hook_event_name"].as_str()?;
        let phase = match name {
            "SessionStart" => Phase::Ready,
            "UserPromptSubmit" | "PostToolUse" => Phase::Working,
            "PreToolUse" => match value["tool_name"].as_str()? {
                "AskUserQuestion" | "request_user_input" | "functions.request_user_input" => {
                    Phase::Input
                }
                _ => Phase::Working,
            },
            "PermissionRequest" => Phase::Permission,
            "Stop" => Phase::Complete,
            "SessionEnd" => Phase::Exited,
            "Interrupt" if agent == Agent::Codex => Phase::Interrupted,
            "StopFailure" if agent == Agent::Claude => Phase::Failed,
            // A failed tool can be retried by the model; it is not a failed response turn.
            "PostToolUseFailure" | "ElicitationResult" if agent == Agent::Claude => Phase::Working,
            "Elicitation" if agent == Agent::Claude => Phase::Input,
            "Notification" if agent == Agent::Claude => {
                match value["notification_type"].as_str()? {
                    "permission_prompt" => Phase::Permission,
                    "elicitation_dialog" => Phase::Input,
                    // idle_prompt is a delayed reminder after a finished response, not a new question.
                    "idle_prompt" => return None,
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(Self {
            agent,
            phase,
            session: identifier(&value["session_id"])?.into(),
            turn: identifier(&value["turn_id"]).map(str::to_owned),
            start: name == "SessionStart",
            new_turn: name == "UserPromptSubmit",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub agent: Agent,
    pub phase: Phase,
    pub session: String,
    pub epoch: String,
    pub turn: String,
    pub revision: String,
}

impl Snapshot {
    pub fn advance(previous: Option<&Self>, event: Event, revision: String) -> Option<Self> {
        if !event.start
            && previous.is_some_and(|old| old.session != event.session || old.agent != event.agent)
        {
            return None;
        }
        // Turn IDs fence late tool/stop events after a new prompt supersedes the old turn.
        if !event.start
            && !event.new_turn
            && event.turn.as_ref().is_some_and(|turn| {
                previous.is_some_and(|old| old.phase != Phase::Ready && &old.turn != turn)
            })
        {
            return None;
        }
        let turn = event.turn.unwrap_or_else(|| {
            if event.start || event.new_turn {
                revision.clone()
            } else {
                previous
                    .map(|old| old.turn.clone())
                    .unwrap_or_else(|| revision.clone())
            }
        });
        // Polling can skip Ready. Carry startup identity through every later event, including same-ID resumes.
        let epoch = if event.start {
            revision.clone()
        } else {
            previous
                .map(|old| old.epoch.clone())
                .unwrap_or_else(|| revision.clone())
        };
        Some(Self {
            agent: event.agent,
            phase: event.phase,
            session: event.session,
            epoch,
            turn,
            revision,
        })
    }
    pub fn to_json(&self) -> Value {
        json!({"version":1,"agent":self.agent.name(),"phase":self.phase.key(),"session":self.session,"epoch":self.epoch,"turn":self.turn,"revision":self.revision})
    }
    pub fn from_json(value: &Value) -> Option<Self> {
        if value["version"] != 1 {
            return None;
        }
        let agent = Agent::from_executable(value["agent"].as_str()?)?;
        if !matches!(agent, Agent::Claude | Agent::Codex) {
            return None;
        }
        let session = identifier(&value["session"])?.to_owned();
        let epoch = match value.get("epoch") {
            None => session.clone(),
            Some(epoch) => identifier(epoch)?.to_owned(),
        };
        Some(Self {
            agent,
            phase: Phase::parse(value["phase"].as_str()?)?,
            session,
            epoch,
            turn: identifier(&value["turn"])?.into(),
            revision: identifier(&value["revision"])?.into(),
        })
    }
    pub fn same_alert(&self, other: &Self) -> bool {
        self.agent == other.agent
            && self.session == other.session
            && self.epoch == other.epoch
            && self.turn == other.turn
            && self.phase == other.phase
    }
}

fn bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if !path.metadata()?.is_file() {
        return Err(io::Error::other("Agent state must be a regular file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("Oversized agent state"));
    }
    Ok(bytes)
}

pub fn read(directory: &Path) -> Option<Snapshot> {
    Snapshot::from_json(
        &serde_json::from_slice(&bounded(&directory.join("agent.json"), 8192).ok()?).ok()?,
    )
}

pub fn record(directory: &Path, token: &str, agent: Agent, payload: &Value) -> io::Result<bool> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(io::Error::other("Invalid agent event directory"));
    }
    let launch: Value =
        serde_json::from_slice(&bounded(&directory.join("launch.json"), 1024 * 1024)?)?;
    if token.is_empty() || launch["agent_token"] != token {
        return Err(io::Error::other("Invalid agent event capability"));
    }
    let Some(event) = Event::parse(agent, payload) else {
        return Ok(false);
    };
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(directory.join("agent.lock"))?;
    let deadline = Instant::now() + Duration::from_millis(250);
    while lock.try_lock().is_err() {
        if Instant::now() >= deadline {
            return Err(io::Error::other("Agent event writer is busy"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    let revision = format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        temporary.path().file_name().unwrap().to_string_lossy()
    );
    let Some(snapshot) = Snapshot::advance(read(directory).as_ref(), event, revision) else {
        return Ok(false);
    };
    // Publish only normalized state. Prompts, tool input and transcripts never enter this file.
    temporary.write_all(serde_json::to_string(&snapshot.to_json())?.as_bytes())?;
    temporary
        .persist(directory.join("agent.json"))
        .map_err(|error| error.error)?;
    Ok(true)
}

pub fn run_from_env(agent: Agent, input: impl Read) -> io::Result<bool> {
    let Some(directory) = std::env::var_os("TERVIA_AGENT_DIR") else {
        return Ok(false);
    };
    let Ok(token) = std::env::var("TERVIA_AGENT_TOKEN") else {
        return Ok(false);
    };
    let mut bytes = Vec::new();
    input.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Ok(false);
    }
    record(
        Path::new(&directory),
        &token,
        agent,
        &serde_json::from_slice(&bytes)?,
    )
}
