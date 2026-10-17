use crate::suggestions::Shell;
use portable_pty::CommandBuilder;
use std::fs;

/// Temporary startup wrappers live as long as the PTY; user-owned RC files are only sourced.
pub struct ShellIntegration {
    directory: tempfile::TempDir,
    token: String,
}

impl ShellIntegration {
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn configure(command: &mut CommandBuilder) -> Result<Option<Self>, String> {
        let shell = command
            .get_argv()
            .first()
            .map(|s| Shell::from_executable(&s.to_string_lossy()))
            .unwrap_or(Shell::Other);
        if shell == Shell::Other {
            return Ok(None);
        }
        let directory = tempfile::Builder::new()
            .prefix("tervia-shell-")
            .rand_bytes(24)
            .tempdir()
            .map_err(|error| error.to_string())?;
        let token = directory
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let integration = Self { directory, token };
        let root = integration.directory.path();
        let write = |name: &str, source: &str| {
            fs::write(root.join(name), source).map_err(|error| error.to_string())
        };
        command.env("TERVIA_SHELL_TOKEN", integration.token());
        command.env("TERVIA_SHELL_DIR", root);
        match shell {
            Shell::Zsh => {
                let original = command.get_env("ZDOTDIR").map(|value| value.to_os_string());
                command.env(
                    "TERVIA_USER_ZDOTDIR_SET",
                    if original.is_some() { "1" } else { "0" },
                );
                if let Some(original) = original {
                    command.env("TERVIA_USER_ZDOTDIR", original);
                }
                for file in [".zshenv", ".zprofile", ".zshrc"] {
                    let restore = "if [[ $TERVIA_USER_ZDOTDIR_SET == 1 ]]; then export ZDOTDIR=\"$TERVIA_USER_ZDOTDIR\"; else unset ZDOTDIR; fi\n";
                    // A system zshrc can derive HISTFILE from our temporary ZDOTDIR.
                    // Repair that injected default before the user's RC reads or overrides it.
                    let history = if file == ".zshrc" {
                        r#"if [[ ${HISTFILE-} == "$TERVIA_SHELL_DIR/"* ]]; then
    HISTFILE="${ZDOTDIR:-$HOME}/${HISTFILE#"$TERVIA_SHELL_DIR/"}"
fi
"#
                    } else {
                        ""
                    };
                    let source = format!(
                        "{restore}{history}[[ -r \"${{ZDOTDIR:-$HOME}}/{file}\" ]] && source \"${{ZDOTDIR:-$HOME}}/{file}\"\n"
                    );
                    let tail = if file == ".zshrc" {
                        "source \"$TERVIA_SHELL_DIR/integration.zsh\"\n"
                    } else {
                        "export TERVIA_USER_ZDOTDIR_SET=${+ZDOTDIR}\nexport TERVIA_USER_ZDOTDIR=${ZDOTDIR:-$HOME}\nexport ZDOTDIR=\"$TERVIA_SHELL_DIR\"\n"
                    };
                    write(file, &format!("{source}{tail}"))?;
                }
                write(
                    "integration.zsh",
                    include_str!("../../assets/shell/integration.zsh"),
                )?;
                command.env("ZDOTDIR", root);
            }
            Shell::Bash => {
                write(
                    "integration.bash",
                    include_str!("../../assets/shell/integration.bash"),
                )?;
                command.env("TERVIA_SHELL_INIT", root.join("integration.bash"));
                let original = command
                    .get_env("PROMPT_COMMAND")
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // --rcfile would disable real login-shell semantics. Bootstrap on the first
                // prompt instead; a profile replacing PROMPT_COMMAND keeps its normal behavior.
                let bootstrap = r#"if [[ -z ${_tervia_bootstrapped-} ]]; then _tervia_bootstrapped=1; builtin source "$TERVIA_SHELL_INIT"; _tervia_prompt_ready; fi"#;
                command.env("PROMPT_COMMAND", format!("{original}\n{bootstrap}"));
            }
            Shell::Fish => {
                write(
                    "integration.fish",
                    include_str!("../../assets/shell/integration.fish"),
                )?;
                command.arg("--init-command");
                command.arg("source \"$TERVIA_SHELL_DIR/integration.fish\"");
            }
            Shell::PowerShell => {
                write(
                    "integration.ps1",
                    include_str!("../../assets/shell/integration.ps1"),
                )?;
                command.env("TERVIA_SHELL_INIT", root.join("integration.ps1"));
                command.args(["-Command", ". $env:TERVIA_SHELL_INIT"]);
            }
            Shell::Other => unreachable!(),
        }
        Ok(Some(integration))
    }
}
