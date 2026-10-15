use super::Error;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub(crate) fn ignored(directory: &Path, paths: &[PathBuf]) -> Result<BTreeSet<PathBuf>, Error> {
    if paths.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path.as_os_str().as_encoded_bytes());
        input.push(0);
    }
    // Keep the real index so tracked files matching an ignore pattern stay ordinary entries.
    let output = match super::command::run_input(
        directory,
        &super::arguments(&["check-ignore", "--stdin", "-z"]),
        Some(input),
    ) {
        Ok(output) => output,
        Err(error) if error.exit_code() == Some(1) => return Ok(BTreeSet::new()),
        Err(error) => return Err(error),
    };
    output
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(path)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStatus {
    pub path: PathBuf,
    pub original: Option<PathBuf>,
    pub index: char,
    pub worktree: char,
    pub conflicted: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub unborn: bool,
    pub files: Vec<FileStatus>,
    pub operation: Option<String>,
}

pub(super) fn path(bytes: &[u8]) -> Result<PathBuf, Error> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(bytes.to_vec()).into())
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes.to_vec())
            .map(PathBuf::from)
            .map_err(|_| "Git returned an unsupported file name encoding".into())
    }
}

pub(super) fn parse(bytes: &[u8]) -> Result<Status, Error> {
    let mut status = Status::default();
    let mut records = bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty());
    while let Some(record) = records.next() {
        if record.starts_with(b"# ") {
            let text = String::from_utf8_lossy(record);
            if let Some(branch) = text.strip_prefix("# branch.head ") {
                status.branch = branch.into();
            }
            if let Some(upstream) = text.strip_prefix("# branch.upstream ") {
                status.upstream = Some(upstream.into());
            }
            if text == "# branch.oid (initial)" {
                status.unborn = true;
            }
            if let Some(counts) = text.strip_prefix("# branch.ab ") {
                let mut counts = counts.split_whitespace();
                status.ahead = counts
                    .next()
                    .and_then(|value| value.trim_start_matches('+').parse().ok())
                    .unwrap_or(0);
                status.behind = counts
                    .next()
                    .and_then(|value| value.trim_start_matches('-').parse().ok())
                    .unwrap_or(0);
            }
            continue;
        }
        if record.starts_with(b"? ") {
            status.files.push(FileStatus {
                path: path(&record[2..])?,
                original: None,
                index: '.',
                worktree: '?',
                conflicted: false,
            });
            continue;
        }
        let fields = match record[0] {
            b'1' => 9,
            b'2' => 10,
            b'u' => 11,
            _ => return Err("Unsupported Git status record".into()),
        };
        let parts: Vec<_> = record.splitn(fields, |byte| *byte == b' ').collect();
        if parts.len() != fields || parts[1].len() != 2 {
            return Err("Invalid Git status record".into());
        }
        let original = if record[0] == b'2' {
            Some(path(records.next().ok_or("Missing original rename path")?)?)
        } else {
            None
        };
        status.files.push(FileStatus {
            path: path(parts[fields - 1])?,
            original,
            index: parts[1][0] as char,
            worktree: parts[1][1] as char,
            conflicted: record[0] == b'u',
        });
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_status_keeps_the_original_parser_message() {
        let error = parse(b"synthetic invalid record\0").unwrap_err();
        assert_eq!(error.kind(), super::super::ErrorKind::Message);
        assert_eq!(error.message(), "Unsupported Git status record");
        assert_eq!(error.details(), "Unsupported Git status record");
    }
}
