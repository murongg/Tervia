use super::*;
use std::collections::BTreeMap;

pub(super) fn is_source(name: &str) -> bool {
    name.eq_ignore_ascii_case("justfile") || name.eq_ignore_ascii_case(".justfile")
}

pub(super) fn select(files: &BTreeSet<String>) -> Result<Option<String>, String> {
    let candidates: Vec<_> = files.iter().filter(|name| is_source(name)).collect();
    match candidates.as_slice() {
        [] => Ok(None),
        [source] => Ok(Some((*source).clone())),
        _ => {
            Err("Multiple justfiles found; keep one justfile or .justfile in this directory".into())
        }
    }
}

pub(super) fn task(source: &str, name: &str) -> Task {
    Task {
        label: name.into(),
        source: source.into(),
        program: "just".into(),
        args: vec!["--justfile".into(), source.into(), "--".into(), name.into()],
    }
}

fn identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && characters.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        && valid_goal(name)
}

#[derive(Default)]
struct Attributes {
    private: bool,
    parameters: bool,
    excluded: bool,
}

pub(super) fn parse(
    source: &str,
    text: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Task>, String> {
    let mut recipes = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut attributes = Attributes::default();
    for (line, colon) in statements(text)? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1]
                .split(['(', ':', ','])
                .next()
                .unwrap_or_default()
                .trim();
            match name {
                "private" => attributes.private = true,
                "arg" => attributes.parameters = true,
                "windows" => attributes.excluded |= !cfg!(windows),
                "unix" => attributes.excluded |= !cfg!(unix),
                "linux" => attributes.excluded |= !cfg!(target_os = "linux"),
                "macos" => attributes.excluded |= !cfg!(target_os = "macos"),
                _ => {}
            }
            continue;
        }
        if line.starts_with("import ")
            || line.starts_with("import? ")
            || line.starts_with("mod ")
            || line.starts_with("mod? ")
        {
            warning(
                warnings,
                source,
                "Imports and modules are not expanded; only recipes in this file are listed",
            );
            attributes = Attributes::default();
            continue;
        }
        if line.starts_with("set ")
            || line.starts_with("export ")
            || line.starts_with("unexport ")
            || line
                .split_once(":=")
                .is_some_and(|(name, _)| identifier(name.trim()))
        {
            attributes = Attributes::default();
            continue;
        }
        if let Some(alias) = line.strip_prefix("alias ") {
            if let Some((name, target)) = alias.split_once(":=") {
                let (name, target) = (name.trim(), target.trim());
                if identifier(name)
                    && identifier(target)
                    && !attributes.private
                    && !attributes.excluded
                    && !name.starts_with('_')
                {
                    if aliases.insert(name.to_owned(), target.to_owned()).is_some() {
                        return Err("Duplicate recipe alias".into());
                    }
                } else if !identifier(name) || !identifier(target) {
                    warning(
                        warnings,
                        source,
                        "Some aliases require unresolved or imported recipes and were skipped",
                    );
                }
            }
            attributes = Attributes::default();
            continue;
        }
        let Some(colon) = colon else {
            warning(
                warnings,
                source,
                "Some declarations could not be recognized; task discovery may be incomplete",
            );
            attributes = Attributes::default();
            continue;
        };
        let header = line[..colon].trim().trim_start_matches('@');
        let end = header.find(char::is_whitespace).unwrap_or(header.len());
        let name = &header[..end];
        if !identifier(name) {
            warning(
                warnings,
                source,
                "Some recipe declarations could not be recognized",
            );
            attributes = Attributes::default();
            continue;
        }
        if attributes.excluded {
            // Platform-specific definitions may intentionally share a name; filter before checking uniqueness.
            attributes = Attributes::default();
            continue;
        }
        let required = required_parameters(header[end..].trim());
        let needs_arguments = required.unwrap_or(true) || attributes.parameters;
        let hidden = attributes.private || name.starts_with('_');
        if needs_arguments && !hidden {
            warning(
                warnings,
                source,
                "Recipes with required or unsupported parameters were skipped; invoke them with explicit arguments in the terminal",
            );
        }
        if recipes
            .insert(name.to_owned(), (needs_arguments, hidden))
            .is_some()
        {
            return Err("Duplicate recipe name".into());
        }
        attributes = Attributes::default();
    }
    let mut names: BTreeSet<_> = recipes
        .iter()
        .filter(|(_, (required, hidden))| !required && !hidden)
        .map(|(name, _)| name.clone())
        .collect();
    let mut resolved = BTreeMap::new();
    for alias in aliases.keys() {
        if !recipes.contains_key(alias) && runnable_alias(alias, &recipes, &aliases, &mut resolved)
        {
            names.insert(alias.clone());
        } else {
            warning(
                warnings,
                source,
                "Aliases with unresolved, conflicting or required-argument targets were skipped",
            );
        }
    }
    Ok(names
        .iter()
        .take(MAX_TASKS + 1)
        .map(|name| task(source, name))
        .collect())
}

fn runnable_alias(
    alias: &str,
    recipes: &BTreeMap<String, (bool, bool)>,
    aliases: &BTreeMap<String, String>,
    resolved: &mut BTreeMap<String, bool>,
) -> bool {
    let mut current = alias;
    let mut path = BTreeSet::new();
    let runnable = loop {
        if let Some((required, _)) = recipes.get(current) {
            break !required;
        }
        if let Some(runnable) = resolved.get(current) {
            break *runnable;
        }
        if !path.insert(current.to_owned()) {
            break false;
        }
        let Some(target) = aliases.get(current) else {
            break false;
        };
        current = target;
    };
    // Resolve a shared suffix once: large alias chains must not turn discovery into a quadratic scan.
    for name in path {
        resolved.insert(name, runnable);
    }
    runnable
}

fn required_parameters(parameters: &str) -> Option<bool> {
    let words = parameter_words(parameters)?;
    let mut required = false;
    let mut index = 0;
    while index < words.len() {
        let word = words[index];
        let (parameter, has_default) = if let Some((name, value)) = word.split_once('=') {
            if value.is_empty() {
                words.get(index + 1)?;
                index += 1;
            }
            (name, true)
        } else if let Some(value) = words.get(index + 1).and_then(|word| word.strip_prefix('=')) {
            index += 1;
            if value.is_empty() {
                words.get(index + 1)?;
                index += 1;
            }
            (word, true)
        } else {
            (word, false)
        };
        let name = parameter.trim_start_matches(['$', '+', '*']);
        if !identifier(name) {
            return None;
        }
        required |= !has_default && !parameter.starts_with('*');
        index += 1;
    }
    Some(required)
}

fn parameter_words(text: &str) -> Option<Vec<&str>> {
    let mut words = Vec::new();
    let mut start = None;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for (index, ch) in text.char_indices() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' && delimiter != '\'' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        if ch.is_whitespace() && depth == 0 {
            if let Some(start) = start.take() {
                words.push(&text[start..index]);
            }
            continue;
        }
        start.get_or_insert(index);
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    if quote.is_some() || depth > 0 {
        return None;
    }
    if let Some(start) = start {
        words.push(&text[start..]);
    }
    Some(words)
}

/// Keep multiline values opaque: a column-zero `name:` inside a string is not a recipe.
fn statements(text: &str) -> Result<Vec<(String, Option<usize>)>, String> {
    let mut result = Vec::new();
    let mut pending = String::new();
    let mut quote: Option<(u8, bool)> = None;
    let mut depth = 0usize;
    let mut colon = None;
    for line in text.lines() {
        if pending.is_empty() && line.starts_with(char::is_whitespace) {
            continue;
        }
        let bytes = line.as_bytes();
        let mut index = 0;
        let mut end = bytes.len();
        while index < bytes.len() {
            let byte = bytes[index];
            if let Some((delimiter, triple)) = quote {
                if byte == b'\\' && delimiter != b'\'' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if byte == delimiter
                    && (!triple || bytes.get(index..index + 3) == Some(&[delimiter; 3]))
                {
                    quote = None;
                    index += if triple { 3 } else { 1 };
                } else {
                    index += 1;
                }
                continue;
            }
            match byte {
                b'#' => {
                    end = index;
                    break;
                }
                b'\'' | b'"' | b'`' => {
                    let triple = byte != b'`' && bytes.get(index..index + 3) == Some(&[byte; 3]);
                    quote = Some((byte, triple));
                    index += if triple { 3 } else { 1 };
                    continue;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b':' if depth == 0
                    && colon.is_none()
                    && bytes.get(index + 1) != Some(&b'=')
                    && bytes.get(index.wrapping_sub(1)) != Some(&b':') =>
                {
                    colon = Some(pending.len() + index)
                }
                _ => {}
            }
            index += 1;
        }
        pending.push_str(&line[..end]);
        if quote.is_some() || depth > 0 {
            pending.push('\n');
            continue;
        }
        if pending.ends_with('\\') {
            pending.pop();
            pending.push(' ');
            continue;
        }
        result.push((std::mem::take(&mut pending), colon.take()));
    }
    if quote.is_some() || depth > 0 || !pending.is_empty() {
        return Err("Unfinished string, expression or continuation in justfile".into());
    }
    Ok(result)
}
