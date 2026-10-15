use std::{ops::Range, path::Path};

pub fn selection_context(path: &Path, source: &str, selection: Range<usize>) -> Option<String> {
    if selection.is_empty() {
        return None;
    }
    let code = source.get(selection.clone())?;
    let line = |offset| {
        1 + source.as_bytes()[..offset]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    };
    Some(code_context(
        path,
        line(selection.start),
        line(selection.end - 1),
        None,
        code,
    ))
}

pub fn code_context(
    path: &Path,
    start: usize,
    end: usize,
    version: Option<&str>,
    code: &str,
) -> String {
    let path = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let location = if start == end {
        format!("{path}:{start}")
    } else {
        format!("{path}:{start}-{end}")
    };
    let version = version
        .map(|version| format!(" ({version})"))
        .unwrap_or_default();
    // A selected Markdown fence must not close the clipboard's enclosing code block.
    let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(3.max(longest + 1));
    let newline = if code.ends_with('\n') { "" } else { "\n" };
    format!("{location}{version}\n{fence}\n{code}{newline}{fence}")
}
