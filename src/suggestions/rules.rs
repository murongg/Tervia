use super::{Command, Shell, Source, Suggestion, eligible};
use regex::Regex;
use std::sync::OnceLock;

fn words(input: &str, shell: Shell) -> Option<Vec<String>> {
    // These initial rules operate on one literal command, never shell pipelines or substitutions.
    if input.contains([
        ';', '|', '&', '`', '$', '\n', '\r', '<', '>', '~', '*', '?', '[', ']', '{', '}', '(', ')',
    ]) {
        return None;
    }
    if shell == Shell::PowerShell {
        // POSIX shlex consumes Windows path separators. Leave quoted PowerShell input to the shell.
        return (!input.contains(['\'', '"']))
            .then(|| input.split_whitespace().map(str::to_owned).collect());
    }
    shlex::split(input)
}

fn join(parts: &[String], shell: Shell) -> String {
    parts
        .iter()
        .map(|part| {
            if !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_./:-=@".contains(c))
            {
                return part.clone();
            }
            match shell {
                Shell::PowerShell => format!("'{}'", part.replace('\'', "''")),
                _ => format!("'{}'", part.replace('\'', "'\\''")),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn corrections(command: &Command) -> Vec<Suggestion> {
    if matches!(command.exit_code, 0 | 130 | 141) || !eligible(&command.input) {
        return vec![];
    }
    let Some(mut parts) = words(&command.input, command.shell) else {
        return vec![];
    };
    let Some(executable) = parts.first().cloned() else {
        return vec![];
    };
    let output = command.output.to_ascii_lowercase();
    let mut candidates = vec![];
    match executable.as_str() {
        "git"
            if parts.get(1).is_some_and(|part| part == "push")
                && output.contains("no upstream branch") =>
        {
            static UPSTREAM: OnceLock<Regex> = OnceLock::new();
            let re = UPSTREAM.get_or_init(|| {
                Regex::new(
                    r"(?m)^\s*git push --set-upstream ([A-Za-z0-9_./-]+) ([A-Za-z0-9_./-]+)\s*$",
                )
                .unwrap()
            });
            if let Some(found) = re.captures(&command.output) {
                let remote = &found[1];
                let branch = &found[2];
                if !remote.starts_with('-') && !branch.starts_with('-') {
                    let mut corrected = vec![
                        "git".into(),
                        "push".into(),
                        "--set-upstream".into(),
                        remote.into(),
                        branch.into(),
                    ];
                    // Preserve safeguards such as --dry-run. Unknown options with separate
                    // values are deliberately left untouched instead of dropping their arguments.
                    let allowed = [
                        "--dry-run",
                        "-n",
                        "--porcelain",
                        "--verbose",
                        "-v",
                        "--quiet",
                        "-q",
                        "--atomic",
                        "--no-verify",
                        "--force",
                        "-f",
                        "--force-with-lease",
                        "--follow-tags",
                        "--tags",
                        "--all",
                        "--mirror",
                        "--delete",
                        "-d",
                        "-u",
                        "--set-upstream",
                    ];
                    let mut valid = true;
                    for part in parts.iter().skip(2) {
                        if part == remote
                            || part == branch
                            || part == "-u"
                            || part == "--set-upstream"
                        {
                            continue;
                        }
                        if allowed.contains(&part.as_str())
                            || part.starts_with("--force-with-lease=")
                        {
                            corrected.push(part.clone());
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    if valid {
                        candidates.push(join(&corrected, command.shell));
                    }
                }
            }
        }
        "git"
            if output.contains("conflict")
                || output.contains("would be overwritten")
                || output.contains("unmerged files") =>
        {
            candidates.push("git status".into())
        }
        "git"
            if parts.get(1).is_some_and(|part| part == "commit")
                && (output.contains("no changes added")
                    || output.contains("nothing added to commit")) =>
        {
            candidates.push("git diff".into())
        }
        "git"
            if parts.get(1).is_some_and(|part| part == "push")
                && (output.contains("non-fast-forward") || output.contains("fetch first")) =>
        {
            candidates.push("git fetch".into())
        }
        "git"
            if parts
                .get(1)
                .is_some_and(|part| part == "checkout" || part == "switch")
                && output.contains("pathspec") =>
        {
            candidates.push("git branch --all".into())
        }
        "mkdir"
            if command.shell != Shell::PowerShell
                && output.contains("no such file or directory")
                && parts.len() > 1
                && !parts.iter().any(|p| p == "-p" || p == "--parents") =>
        {
            parts.insert(1, "-p".into());
            candidates.push(join(&parts, command.shell));
        }
        "cp" if command.shell != Shell::PowerShell
            && (output.contains("is a directory")
                || output.contains("omitting directory")
                || output.contains("-r not specified"))
            && parts.len() > 2
            && !parts
                .iter()
                .any(|p| p == "-r" || p == "-R" || p == "--recursive") =>
        {
            parts.insert(1, "-R".into());
            candidates.push(join(&parts, command.shell));
        }
        _ if output.contains("command not found") || output.contains("is not recognized") => {
            let corrected = match executable.as_str() {
                "gti" | "gt" | "gi" => Some("git"),
                "npn" | "nmp" => Some("npm"),
                "pnmp" => Some("pnpm"),
                "carog" | "carg" => Some("cargo"),
                "dcoker" | "dokcer" => Some("docker"),
                "pyhton" => Some("python"),
                "pyhton3" => Some("python3"),
                _ => None,
            };
            if let Some(corrected) = corrected {
                parts[0] = corrected.into();
                candidates.push(join(&parts, command.shell));
            }
        }
        _ => {}
    }
    candidates
        .into_iter()
        .filter(|candidate| candidate != &command.input && eligible(candidate))
        .map(|command| Suggestion {
            command,
            source: Source::Correction,
        })
        .collect()
}
