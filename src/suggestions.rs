//! Local command completion uses typed prefixes and device-local history.
use std::path::PathBuf;

mod completion;
mod history;
mod shell;
pub use completion::Input;
mod rules;
pub use history::{History, Store};
pub use rules::corrections;
#[cfg(feature = "pty")]
mod worker;
#[cfg(feature = "pty")]
pub use worker::Worker;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Other,
}

impl Shell {
    pub fn from_executable(path: &str) -> Self {
        match path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase()
            .trim_end_matches(".exe")
        {
            "bash" => Self::Bash,
            "zsh" => Self::Zsh,
            "fish" => Self::Fish,
            "powershell" | "pwsh" => Self::PowerShell,
            _ => Self::Other,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub input: String,
    pub output: String,
    pub cwd: PathBuf,
    pub exit_code: i32,
    pub shell: Shell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Correction,
    History,
    Common,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub command: String,
    pub source: Source,
}

pub fn eligible(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= 4096
        && !input.starts_with(char::is_whitespace)
        && !input.chars().any(char::is_control)
}
