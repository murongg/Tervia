use super::*;
use crate::tasks::Task;
use std::ffi::OsStr;

pub fn task_command_for(shell: &OsStr, cwd: &Path, task: &Task) -> Result<CommandBuilder, String> {
    Task::from_json(&task.to_json())?;
    let name = shell
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let name = name
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".exe");
    let words = std::iter::once(task.program.as_str()).chain(task.args.iter().map(String::as_str));
    let mut command = super::shell_command_for(shell, cwd);
    command.get_argv_mut().truncate(1);
    match name {
        "bash" | "zsh" | "fish" | "sh" => {
            // Only the discovered program and argument names enter the shell. Script bodies stay in the manifest.
            let line = words.map(|word| format!("'{}'", word.replace('\'', "'\\''"))).collect::<Vec<_>>().join(" ");
            command.args(["-l", "-i", "-c", &line]);
        }
        "pwsh" | "powershell" => {
            let line = words.map(|word| format!("'{}'", word.replace('\'', "''"))).collect::<Vec<_>>().join(" ");
            command.args(["-NoLogo", "-Command", &format!("$ErrorActionPreference = 'Stop'; $PSNativeCommandUseErrorActionPreference = $false; $global:LASTEXITCODE = 0; & {line}; exit $LASTEXITCODE")]);
        }
        "cmd" => {
            let words: Vec<_> = words.collect();
            // CMD expands these characters even inside quotes; do not silently change a script's identity.
            if words.iter().any(|word| word.contains(['%', '!', '"'])) {
                return Err("This task name cannot be passed safely to CMD. Select PowerShell in Settings and retry.".into());
            }
            let line = words.iter().map(|word| format!("\"{word}\"")).collect::<Vec<_>>().join(" ");
            command.args(["/D", "/S", "/C", &format!("\"{line}\"")]);
        }
        _ => return Err("Project tasks support Bash, Zsh, Fish, sh, PowerShell and CMD. Select a supported shell and retry.".into()),
    }
    Ok(command)
}
