//! Read-only shell archives have no trustworthy working-directory or exit-status metadata.
use super::{Shell, eligible};
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_COMMANDS: usize = 2000;

#[derive(Default)]
pub(super) struct Archive {
    snapshot: Option<Snapshot>,
}

struct Snapshot {
    path: PathBuf,
    shell: Shell,
    modified: SystemTime,
    length: u64,
    commands: Vec<String>,
}

impl Archive {
    pub fn read(&mut self, path: &Path, shell: Shell) -> io::Result<&[String]> {
        if !path.is_absolute() {
            return Err(io::Error::other("Shell history path must be absolute"));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // A misconfigured HISTFILE must not block the history worker on a FIFO.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut file = match options.open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.snapshot = None;
                return Ok(&[]);
            }
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::other("Shell history must be a regular file"));
        }
        let modified = metadata.modified().ok();
        if self.snapshot.as_ref().is_some_and(|cached| {
            cached.path == path
                && cached.shell == shell
                && Some(cached.modified) == modified
                && cached.length == metadata.len()
        }) {
            return Ok(&self.snapshot.as_ref().unwrap().commands);
        }
        let offset = metadata.len().saturating_sub(MAX_BYTES);
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES).read_to_end(&mut bytes)?;
        let (source, continued) = if offset > 0 {
            // Discard the partial first physical line, including split UTF-8 characters.
            bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or((&[][..], false), |end| {
                    let fragment = bytes[..end].strip_suffix(b"\r").unwrap_or(&bytes[..end]);
                    (&bytes[end + 1..], continues(fragment, shell))
                })
        } else {
            (bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes), false)
        };
        let commands = parse(source, shell, offset > 0, continued);
        self.snapshot = Some(Snapshot {
            path: path.into(),
            shell,
            modified: modified.unwrap_or(SystemTime::UNIX_EPOCH),
            length: metadata.len(),
            commands,
        });
        Ok(&self.snapshot.as_ref().unwrap().commands)
    }
}

fn parse(source: &[u8], shell: Shell, truncated: bool, continued: bool) -> Vec<String> {
    let mut commands = VecDeque::new();
    let mut push = |input: &str| {
        if eligible(input) {
            commands.push_back(input.to_owned());
            if commands.len() > MAX_COMMANDS {
                commands.pop_front();
            }
        }
    };
    let lines = || {
        source
            .split_inclusive(|byte| *byte == b'\n')
            .filter_map(|bytes| {
                // A writer may still be appending the final record. Never suggest a partial command.
                let line = bytes.strip_suffix(b"\n")?;
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                // Even an invalid encoded line can continue into the next physical line.
                Some((
                    std::str::from_utf8(line).unwrap_or("\0"),
                    continues(line, shell),
                ))
            })
    };
    let bash_timestamps = shell == Shell::Bash && lines().any(|(line, _)| timestamp(line));
    let mut bash_record = None::<String>;
    let mut discard = continued;
    for (line, continued) in lines() {
        if bash_timestamps {
            if timestamp(line) {
                if let Some(record) = bash_record.take() {
                    push(record.strip_suffix('\n').unwrap_or(&record));
                }
                bash_record = Some(String::new());
            } else if let Some(record) = &mut bash_record {
                if record.len() <= 4096 {
                    record.push_str(line);
                    record.push('\n');
                }
            } else if !truncated {
                push(line);
            }
            continue;
        }
        if shell == Shell::Fish {
            if let Some(command) = line.strip_prefix("- cmd: ").and_then(unescape_fish) {
                push(&command);
            }
            continue;
        }
        if shell == Shell::Other {
            continue;
        }
        if !discard && !continued {
            let input = if shell == Shell::Zsh {
                zsh_command(line)
            } else {
                line
            };
            push(input);
        }
        // Skip every physical line of a multi-line command, including its final line.
        discard = continued;
    }
    if source.ends_with(b"\n")
        && let Some(record) = bash_record
    {
        push(record.strip_suffix('\n').unwrap_or(&record));
    }
    commands.into_iter().collect()
}

fn timestamp(line: &str) -> bool {
    line.strip_prefix('#')
        .is_some_and(|stamp| !stamp.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit()))
}

fn continues(line: &[u8], shell: Shell) -> bool {
    match shell {
        // Bash saves literal backslash pairs unchanged; only an unpaired slash continues.
        Shell::Bash => line.iter().rev().take_while(|byte| **byte == b'\\').count() % 2 == 1,
        Shell::PowerShell => line.ends_with(b"`"),
        _ => line.ends_with(b"\\"),
    }
}

fn zsh_command(line: &str) -> &str {
    let command = if let Some((stamp, command)) =
        line.strip_prefix(": ").and_then(|s| s.split_once(';'))
        && stamp.split_once(':').is_some_and(|(start, duration)| {
            !start.is_empty()
                && !duration.is_empty()
                && start
                    .bytes()
                    .chain(duration.bytes())
                    .all(|b| b.is_ascii_digit())
        }) {
        command
    } else {
        line.strip_prefix("\\:").map_or(line, |_| &line[1..])
    };
    // Zsh adds one space to disambiguate a command ending in literal backslashes.
    if command.ends_with(' ') && command.trim_end_matches(' ').ends_with('\\') {
        &command[..command.len() - 1]
    } else {
        command
    }
}

fn unescape_fish(input: &str) -> Option<String> {
    let mut command = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            command.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => command.push('\\'),
            // Escaped newlines mark a multi-line command, which completion deliberately excludes.
            'n' => return None,
            _ => return None,
        }
    }
    Some(command)
}
