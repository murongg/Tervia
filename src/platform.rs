#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

impl Platform {
    pub const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::Mac
    } else if cfg!(target_os = "windows") {
        Self::Windows
    } else {
        Self::Linux
    };
    pub fn mono_font(self) -> &'static str {
        match self {
            Self::Mac => "Menlo",
            Self::Windows => "Consolas",
            Self::Linux => "DejaVu Sans Mono",
        }
    }
    pub fn ui_font(self) -> &'static str {
        match self {
            Self::Mac => ".SystemUIFont",
            Self::Windows => "Segoe UI",
            Self::Linux => "sans-serif",
        }
    }
    pub fn default_shell(self) -> &'static str {
        match self {
            Self::Mac => "/bin/zsh",
            Self::Windows => "powershell.exe",
            Self::Linux => "/bin/bash",
        }
    }
    pub fn shell_args(path: &str) -> &'static [&'static str] {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .trim_end_matches(".exe");
        match name {
            "powershell" | "pwsh" => &["-NoLogo", "-NoExit"],
            "cmd" => &["/K"],
            "bash" | "zsh" | "fish" | "sh" => &["-l", "-i"],
            _ => &[],
        }
    }
    pub fn clear_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-k"
        } else {
            "ctrl-shift-k"
        }
    }
    pub fn copy_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-c"
        } else {
            "ctrl-shift-c"
        }
    }
    pub fn paste_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-v"
        } else {
            "ctrl-shift-v"
        }
    }
    pub fn select_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-a"
        } else {
            "ctrl-shift-a"
        }
    }
    pub fn new_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-t"
        } else {
            "ctrl-shift-t"
        }
    }
    pub fn sidebar_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-b"
        } else {
            "ctrl-shift-b"
        }
    }
    pub fn quit_key(self) -> &'static str {
        if self == Self::Mac {
            "cmd-q"
        } else {
            "ctrl-shift-q"
        }
    }
    pub fn hint(self, text: &str) -> String {
        if self == Self::Mac {
            return text.to_owned();
        }
        text.replace("⌘K", "Ctrl+Shift+K")
            .replace("⌘T", "Ctrl+Shift+T")
            .replace("⌘B", "Ctrl+Shift+B")
            .replace("⌘C", "Ctrl+Shift+C")
            .replace("⌘V", "Ctrl+Shift+V")
            .replace("⌘⇧", "Ctrl+Shift+")
            .replace('⌘', "Ctrl+")
    }
}
