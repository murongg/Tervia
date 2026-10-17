#[derive(Default)]
pub struct Input {
    line: Option<String>,
    ready: bool,
    dirty: bool,
}

impl Input {
    pub fn prompt(&mut self) {
        self.ready = true;
        self.line = (!self.dirty).then(String::new);
    }

    pub fn started(&mut self) {
        self.ready = false;
        self.line = None;
    }

    pub fn send(&mut self, bytes: &[u8]) {
        self.dirty = !matches!(bytes, b"\r" | b"\n" | b"\x03");
        if !self.ready {
            return;
        }
        match bytes {
            b"\r" | b"\n" | b"\x03" => self.started(),
            b"\x7f" | b"\x08" => {
                if let Some(line) = &mut self.line {
                    // Shells can erase a whole grapheme; don't guess after a Unicode deletion.
                    if line.pop().is_some_and(|c| !c.is_ascii()) {
                        self.line = None;
                    }
                }
            }
            b"\x15" => {
                if let Some(line) = &mut self.line {
                    line.clear();
                }
            }
            b"\x0c" => {}
            bytes => {
                let bytes = bytes
                    .strip_prefix(b"\x1b[200~")
                    .and_then(|body| body.strip_suffix(b"\x1b[201~"))
                    .unwrap_or(bytes);
                match (self.line.as_mut(), std::str::from_utf8(bytes)) {
                    (Some(line), Ok(text))
                        if !text.chars().any(char::is_control)
                            && line.len() + text.len() <= 4096 =>
                    {
                        line.push_str(text)
                    }
                    // History navigation, shell completion and cursor edits change text we don't own.
                    // Stop offering suffixes until the next known empty prompt.
                    _ => self.line = None,
                }
            }
        }
    }

    pub fn prefix(&self) -> Option<&str> {
        self.line
            .as_deref()
            .filter(|line| line.chars().count() >= 2 && super::eligible(line))
    }
}

pub(super) const COMMON: &[&str] = &[
    "git status",
    "git stash",
    "git commit",
    "git diff",
    "git log",
    "git branch",
    "git switch",
    "git fetch",
    "git pull",
    "git push",
    "git add",
    "cargo build",
    "cargo test",
    "cargo check",
    "cargo run",
    "cargo clippy",
    "npm install",
    "npm run dev",
    "npm test",
    "npm run build",
    "pnpm install",
    "pnpm dev",
    "pnpm test",
    "pnpm build",
    "ls -la",
    "mkdir -p",
];
