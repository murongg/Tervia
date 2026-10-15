use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLink {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

impl FileLink {
    pub fn resolve(&self, root: &Path) -> Result<PathBuf, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let path = root
            .join(&self.path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("File points outside the project".into());
        }
        if !path.is_file() {
            return Err("Path is not a file".into());
        }
        Ok(path.strip_prefix(root).unwrap().to_owned())
    }
}

pub fn file_link(text: &str, offset: usize) -> Option<FileLink> {
    if offset >= text.len() {
        return None;
    }
    let delimiter = |c: char| c.is_whitespace() || "`\"'<>[](){},;".contains(c);
    let mut chars = text.char_indices().peekable();
    while let Some((start, first)) = chars.next() {
        if first.is_whitespace() || "<>[](){},;".contains(first) {
            continue;
        }
        let quote = "`\"'".contains(first).then_some(first);
        // A quoted path may contain spaces, with its location suffix inside or after the quote.
        let content_start = start + if quote.is_some() { first.len_utf8() } else { 0 };
        let mut content_end = text.len();
        let mut end = text.len();
        while let Some(&(ix, c)) = chars.peek() {
            if quote == Some(c) {
                content_end = ix;
                chars.next();
                end = ix + c.len_utf8();
                while let Some(&(ix, c)) = chars.peek() {
                    if c != ':' && !c.is_ascii_digit() {
                        break;
                    }
                    chars.next();
                    end = ix + c.len_utf8();
                }
                break;
            }
            if quote.is_none() && delimiter(c) {
                content_end = ix;
                end = ix;
                break;
            }
            chars.next();
        }
        if !(start..end).contains(&offset) {
            continue;
        }
        let mut reference = text[content_start..content_end].to_owned();
        if quote.is_some() && content_end < end {
            reference.push_str(&text[content_end + 1..end]);
        }
        return parse(&reference);
    }
    None
}

fn parse(reference: &str) -> Option<FileLink> {
    let reference = reference.trim_end_matches([':', '.', '!', '?']);
    if reference.contains("://") || !reference.chars().any(char::is_alphabetic) {
        return None;
    }
    let mut path = reference;
    let mut numbers = Vec::new();
    for _ in 0..2 {
        let Some((prefix, suffix)) = path.rsplit_once(':') else {
            break;
        };
        if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            let value = suffix.parse::<usize>().ok().filter(|value| *value > 0)?;
            numbers.push(value);
            path = prefix;
        } else {
            break;
        }
    }
    if !path.contains(['/', '\\', '.']) && !matches!(path, "Makefile" | "Dockerfile" | "LICENSE") {
        return None;
    }
    let (line, column) = match numbers.as_slice() {
        [] => (1, 1),
        [line] => (*line, 1),
        [column, line] => (*line, *column),
        _ => unreachable!(),
    };
    Some(FileLink {
        path: path.into(),
        line,
        column,
    })
}
