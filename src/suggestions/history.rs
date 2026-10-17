use super::{Command, Shell, Source, Suggestion, eligible};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::PathBuf,
};

const MAX_ENTRIES: usize = 2000;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Default)]
pub struct History {
    entries: Vec<Entry>,
    tails: HashMap<String, u64>,
    next_id: u64,
}

struct Entry {
    id: u64,
    previous: Option<u64>,
    session: String,
    input: String,
    cwd: PathBuf,
    shell: Shell,
    exit_code: i32,
}

fn rank<'a>(inputs: impl Iterator<Item = (&'a str, u64)>) -> Vec<&'a str> {
    let mut counts: HashMap<&str, (usize, u64)> = HashMap::new();
    for (command, recent) in inputs {
        let rank = counts.entry(command).or_default();
        rank.0 += 1;
        rank.1 = rank.1.max(recent);
    }
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by(|(a, x), (b, y)| y.cmp(x).then_with(|| a.cmp(b)));
    ranked.into_iter().map(|(command, _)| command).collect()
}

impl History {
    pub fn complete(&self, prefix: &str, cwd: &std::path::Path, shell: Shell) -> Vec<Suggestion> {
        self.complete_with(prefix, cwd, shell, &[])
    }

    fn complete_with(
        &self,
        prefix: &str,
        cwd: &std::path::Path,
        shell: Shell,
        imported: &[String],
    ) -> Vec<Suggestion> {
        if !eligible(prefix) || prefix.chars().count() < 2 {
            return Vec::new();
        }
        let matches = |input: &str| input.starts_with(prefix) && input != prefix;
        let local = rank(
            self.entries
                .iter()
                .filter(|entry| {
                    entry.cwd == cwd
                        && entry.shell == shell
                        && entry.exit_code == 0
                        && matches(&entry.input)
                })
                .map(|entry| (entry.input.as_str(), entry.id)),
        );
        let imported = rank(
            imported
                .iter()
                .enumerate()
                .filter(|(_, command)| matches(command))
                .map(|(index, command)| (command.as_str(), index as u64)),
        );
        // Archives have no cwd or exit code. Keep them below known local successes.
        let history = local
            .into_iter()
            .chain(imported)
            .map(|command| (command, Source::History));
        let common = super::completion::COMMON
            .iter()
            .copied()
            .filter(|command| matches(command))
            .map(|command| (command, Source::Common));
        let mut suggestions: Vec<Suggestion> = Vec::new();
        for (command, source) in history.chain(common) {
            if !suggestions.iter().any(|s| s.command == command) {
                suggestions.push(Suggestion {
                    command: command.into(),
                    source,
                });
                if suggestions.len() == 6 {
                    break;
                }
            }
        }
        suggestions
    }

    pub fn record(&mut self, session: &str, command: &Command) {
        if !eligible(&command.input) || !command.cwd.is_absolute() {
            // A hidden command breaks the sequence; never connect commands across it.
            self.tails.remove(session);
            return;
        }
        self.next_id += 1;
        let previous = self.tails.insert(session.to_owned(), self.next_id);
        self.entries.push(Entry {
            id: self.next_id,
            previous,
            session: session.into(),
            input: command.input.clone(),
            cwd: command.cwd.clone(),
            shell: command.shell,
            exit_code: command.exit_code,
        });
        if self.entries.len() > MAX_ENTRIES {
            self.entries.drain(..self.entries.len() - MAX_ENTRIES);
        }
        if self.tails.len() > MAX_ENTRIES {
            let oldest = self.entries.first().map_or(0, |entry| entry.id);
            self.tails.retain(|_, id| *id >= oldest);
        }
    }

    pub fn predict(&self, command: &Command) -> Option<Suggestion> {
        if !eligible(&command.input) {
            return None;
        }
        let entries: HashMap<_, _> = self.entries.iter().map(|entry| (entry.id, entry)).collect();
        let mut counts: HashMap<&str, (usize, u64)> = HashMap::new();
        let mut samples = 0;
        for next in self.entries.iter().rev() {
            let Some(previous) = next.previous.and_then(|id| entries.get(&id)) else {
                continue;
            };
            if previous.input != command.input
                || previous.cwd != command.cwd
                || previous.shell != command.shell
                || previous.exit_code != command.exit_code
                || next.exit_code != 0
            {
                continue;
            }
            let count = counts.entry(&next.input).or_default();
            count.0 += 1;
            count.1 = count.1.max(next.id);
            samples += 1;
            if samples == 25 {
                break;
            }
        }
        let (input, (count, _)) = counts
            .into_iter()
            .max_by(|(a, x), (b, y)| x.cmp(y).then_with(|| b.cmp(a)))?;
        // Require two repetitions and at least half the samples, rather than one accidental pair.
        (count >= 2 && count * 2 >= samples).then(|| Suggestion {
            command: input.into(),
            source: Source::History,
        })
    }

    pub fn encode(&self) -> Result<String, String> {
        let entries: Vec<_> = self.entries.iter().map(|entry| json!({"id":entry.id,"previous":entry.previous,"session":entry.session,"input":entry.input,"cwd":entry.cwd,"shell":entry.shell.name(),"exit_code":entry.exit_code})).collect();
        serde_json::to_string(&json!({"version":1,"entries":entries}))
            .map_err(|error| error.to_string())
    }

    pub fn decode(source: &str) -> Result<Self, String> {
        let invalid = || "Invalid command history".to_owned();
        if source.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid());
        }
        let value: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
        if value["version"].as_u64() != Some(1) {
            return Err(invalid());
        }
        let values = value["entries"].as_array().ok_or_else(invalid)?;
        if values.len() > MAX_ENTRIES {
            return Err(invalid());
        }
        let mut history = Self::default();
        let mut sessions = HashMap::new();
        for value in values {
            let id = value["id"]
                .as_u64()
                .filter(|id| *id > history.next_id && *id < u64::MAX)
                .ok_or_else(invalid)?;
            let session = value["session"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or_else(invalid)?;
            let input = value["input"]
                .as_str()
                .filter(|s| eligible(s))
                .ok_or_else(invalid)?;
            let cwd = PathBuf::from(
                value["cwd"]
                    .as_str()
                    .filter(|s| s.len() <= 16384)
                    .ok_or_else(invalid)?,
            );
            let exit_code = value["exit_code"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(invalid)?;
            let shell = match value["shell"].as_str() {
                Some("bash") => Shell::Bash,
                Some("zsh") => Shell::Zsh,
                Some("fish") => Shell::Fish,
                Some("powershell") => Shell::PowerShell,
                Some("other") => Shell::Other,
                _ => return Err(invalid()),
            };
            let previous = if value["previous"].is_null() {
                None
            } else {
                Some(
                    value["previous"]
                        .as_u64()
                        .filter(|previous| *previous < id)
                        .ok_or_else(invalid)?,
                )
            };
            if !cwd.is_absolute()
                || previous
                    .and_then(|previous| sessions.get(&previous))
                    .is_some_and(|s| *s != session)
            {
                return Err(invalid());
            }
            sessions.insert(id, session);
            history.entries.push(Entry {
                id,
                previous,
                session: session.into(),
                input: input.into(),
                cwd,
                shell,
                exit_code,
            });
            history.next_id = id;
        }
        // Restored commands are prediction samples, never the tail of a newly opened shell.
        Ok(history)
    }
}

/// Owned by the history worker so filesystem IO never runs on the terminal's UI thread.
pub struct Store {
    path: Option<PathBuf>,
    history: Option<History>,
    archive: super::shell::Archive,
}

impl Store {
    pub fn complete_from(
        &mut self,
        prefix: &str,
        cwd: &std::path::Path,
        shell: Shell,
        source: Option<&std::path::Path>,
    ) -> Result<Vec<Suggestion>, String> {
        self.load()?;
        let imported = match source {
            Some(path) => self
                .archive
                .read(path, shell)
                .map_err(|error| error.to_string())?,
            None => &[],
        };
        Ok(self
            .history
            .as_ref()
            .unwrap()
            .complete_with(prefix, cwd, shell, imported))
    }

    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            history: None,
            archive: super::shell::Archive::default(),
        }
    }

    pub fn record(
        &mut self,
        session: &str,
        command: &Command,
    ) -> Result<Option<Suggestion>, String> {
        self.load()?;
        let history = self.history.as_mut().unwrap();
        history.record(session, command);
        let prediction = history.predict(command);
        if let Some(path) = &self.path {
            let directory = path
                .parent()
                .ok_or("Command history has no parent directory")?;
            fs::create_dir_all(directory).map_err(|error| error.to_string())?;
            let mut file =
                tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
            file.write_all(history.encode()?.as_bytes())
                .map_err(|error| error.to_string())?;
            file.as_file()
                .sync_all()
                .map_err(|error| error.to_string())?;
            file.persist(path).map_err(|error| error.to_string())?;
        }
        Ok(prediction)
    }

    pub fn complete(
        &mut self,
        prefix: &str,
        cwd: &std::path::Path,
        shell: Shell,
    ) -> Result<Vec<Suggestion>, String> {
        self.load()?;
        Ok(self.history.as_ref().unwrap().complete(prefix, cwd, shell))
    }

    fn load(&mut self) -> Result<(), String> {
        if self.history.is_none() {
            self.history = Some(match &self.path {
                Some(path) => match fs::metadata(path) {
                    Ok(metadata) if metadata.len() > MAX_FILE_BYTES => {
                        return Err("Command history is too large".into());
                    }
                    Ok(_) => History::decode(
                        &fs::read_to_string(path).map_err(|error| error.to_string())?,
                    )?,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => History::default(),
                    Err(error) => return Err(error.to_string()),
                },
                None => History::default(),
            });
        }
        Ok(())
    }
}
