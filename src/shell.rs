use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{locale::Language, platform::Platform};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    #[default]
    Default,
    PowerShell,
    WindowsPowerShell,
    Cmd,
}

impl Profile {
    pub const WINDOWS: [Self; 3] = [Self::PowerShell, Self::WindowsPowerShell, Self::Cmd];

    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::PowerShell => "pwsh",
            Self::WindowsPowerShell => "powershell",
            Self::Cmd => "cmd",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        std::iter::once(Self::Default)
            .chain(Self::WINDOWS)
            .find(|profile| profile.id() == id)
    }

    pub fn label(self, language: Language) -> &'static str {
        match self {
            Self::Default => language.text("shell_default"),
            Self::PowerShell => "PowerShell 7",
            Self::WindowsPowerShell => "Windows PowerShell",
            Self::Cmd => language.text("shell_cmd"),
        }
    }

    fn executable(self) -> &'static str {
        match self {
            Self::Default => Platform::CURRENT.default_shell(),
            Self::PowerShell => "pwsh.exe",
            Self::WindowsPowerShell => "powershell.exe",
            Self::Cmd => "cmd.exe",
        }
    }

    fn locate(
        self,
        paths: &[PathBuf],
        system: Option<&Path>,
        programs: Option<&Path>,
    ) -> Option<PathBuf> {
        let installed = match self {
            Self::WindowsPowerShell => {
                system.map(|root| root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
            }
            Self::Cmd => system.map(|root| root.join("System32/cmd.exe")),
            Self::PowerShell => programs.map(|root| root.join("PowerShell/7/pwsh.exe")),
            Self::Default => None,
        };
        installed.filter(|path| path.is_file()).or_else(|| {
            paths
                .iter()
                .map(|path| path.join(self.executable()))
                .find(|path| path.is_file())
        })
    }

    fn installed(self) -> Option<PathBuf> {
        let paths: Vec<_> = std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default();
        let system = std::env::var_os("SystemRoot").map(PathBuf::from);
        let programs = std::env::var_os("ProgramFiles").map(PathBuf::from);
        self.locate(&paths, system.as_deref(), programs.as_deref())
    }

    pub fn available() -> Vec<Self> {
        std::iter::once(Self::Default)
            .chain(
                Self::WINDOWS
                    .into_iter()
                    .filter(|profile| profile.installed().is_some()),
            )
            .collect()
    }

    pub fn resolve(self) -> OsString {
        if self == Self::Default {
            return std::env::var_os("SHELL")
                .filter(|shell| !shell.is_empty())
                .unwrap_or_else(|| Platform::CURRENT.default_shell().into());
        }
        // An unavailable explicit selection must fail to launch instead of running a different shell.
        self.installed()
            .map(PathBuf::into_os_string)
            .unwrap_or_else(|| self.executable().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_uses_installed_shells_including_paths_with_spaces() {
        let root = tempfile::tempdir().unwrap();
        let system = root.path().join("synthetic Windows");
        let programs = root.path().join("synthetic Programs");
        let tools = root.path().join("synthetic tools");
        std::fs::create_dir_all(&tools).unwrap();
        let paths = vec![tools.clone()];
        assert!(
            Profile::PowerShell
                .locate(&paths, Some(&system), Some(&programs))
                .is_none()
        );
        for (profile, path) in [
            (Profile::PowerShell, programs.join("PowerShell/7/pwsh.exe")),
            (
                Profile::WindowsPowerShell,
                system.join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            ),
            (Profile::Cmd, system.join("System32/cmd.exe")),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"synthetic executable").unwrap();
            assert_eq!(
                profile.locate(&paths, Some(&system), Some(&programs)),
                Some(path)
            );
        }
        let portable = tools.join("pwsh.exe");
        std::fs::write(&portable, b"synthetic executable").unwrap();
        assert_eq!(
            Profile::PowerShell.locate(&paths, None, None),
            Some(portable)
        );
    }
}
