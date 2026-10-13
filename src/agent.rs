#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Gemini,
    Grok,
    Hermes,
    OpenCode,
    OpenClaw,
}

impl Agent {
    pub fn from_executable(path: &str) -> Option<Self> {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
        match parts.last().copied()? {
            "claude" | "claude.exe" => Some(Self::Claude),
            "codex" | "codex.exe" => Some(Self::Codex),
            "gemini" | "gemini.exe" => Some(Self::Gemini),
            "grok" | "grok.exe" | "grok-cli" => Some(Self::Grok),
            "hermes" | "hermes.exe" | "hermes-agent" => Some(Self::Hermes),
            "opencode" | "opencode.exe" => Some(Self::OpenCode),
            "openclaw" | "openclaw.exe" | "clawdbot" | "moltbot" => Some(Self::OpenClaw),
            version
                if parts.len() >= 3
                    && parts[parts.len() - 3..parts.len() - 1] == ["claude", "versions"]
                    && version.starts_with(|character: char| character.is_ascii_digit()) =>
            {
                Some(Self::Claude)
            }
            _ => None,
        }
    }

    pub fn is_launcher(path: &str) -> bool {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .trim_end_matches(".exe");
        matches!(name, "node" | "nodejs" | "bun" | "deno")
            || name.strip_prefix("python").is_some_and(|version| {
                version
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '.')
            })
    }

    pub fn from_argv(
        executable: &str,
        arguments: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Option<Self> {
        if let Some(agent) = Self::from_executable(executable) {
            return Some(agent);
        }
        if !Self::is_launcher(executable) {
            return None;
        }
        let mut arguments = arguments.into_iter().skip(1).take(8);
        while let Some(argument) = arguments.next() {
            let argument = argument.as_ref();
            if argument == "-m" && executable.to_ascii_lowercase().contains("python") {
                let module = arguments.next()?;
                let module = module.as_ref();
                return match module.split('.').next()? {
                    "hermes_cli" => Some(Self::Hermes),
                    "grok_cli" => Some(Self::Grok),
                    _ => None,
                };
            }
            if matches!(
                argument,
                "-u" | "-B" | "-O" | "-OO" | "--no-warnings" | "--enable-source-maps"
            ) {
                continue;
            }
            if argument.starts_with('-') {
                return None;
            }
            // The first entrypoint decides identity; later arguments may contain a prompt.
            return Self::from_script(argument);
        }
        None
    }

    fn from_script(path: &str) -> Option<Self> {
        if let Some(agent) = Self::from_executable(path) {
            return Some(agent);
        }
        let path = path.replace('\\', "/");
        let parts: Vec<_> = path.split('/').collect();
        let name = parts.last().copied()?;
        if ![".js", ".mjs", ".cjs", ".py"]
            .iter()
            .any(|extension| name.ends_with(extension))
        {
            return None;
        }
        if parts
            .windows(2)
            .any(|pair| pair == ["@anthropic-ai", "claude-code"])
        {
            return Some(Self::Claude);
        }
        if parts.windows(2).any(|pair| pair == ["@openai", "codex"]) {
            return Some(Self::Codex);
        }
        if parts
            .windows(2)
            .any(|pair| pair == ["@google", "gemini-cli"])
        {
            return Some(Self::Gemini);
        }
        if parts.contains(&"grok-cli") {
            return Some(Self::Grok);
        }
        if parts.contains(&"hermes_cli") {
            return Some(Self::Hermes);
        }
        if parts.contains(&"opencode-ai")
            || parts.windows(2).any(|pair| pair == ["@opencode-ai", "cli"])
        {
            return Some(Self::OpenCode);
        }
        if parts
            .iter()
            .any(|part| matches!(*part, "openclaw" | "clawdbot" | "moltbot"))
        {
            return Some(Self::OpenClaw);
        }
        None
    }

    pub fn from_foreground(paths: impl IntoIterator<Item = impl AsRef<str>>) -> Option<Self> {
        paths
            .into_iter()
            .find_map(|path| Self::from_executable(path.as_ref()))
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::Grok => "Grok",
            Self::Hermes => "Hermes",
            Self::OpenCode => "OpenCode",
            Self::OpenClaw => "OpenClaw",
        }
    }
}
