use super::*;

pub(super) const FILES: &[&str] = &["GNUmakefile", "makefile", "Makefile"];

pub(super) fn task(source: &str, name: &str) -> Task {
    Task {
        label: name.into(),
        source: source.into(),
        program: "make".into(),
        args: vec!["-f".into(), source.into(), "--".into(), name.into()],
    }
}

fn literal(name: &str) -> bool {
    valid_goal(name)
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|ch| ch.is_whitespace() || "$%\\#;:*?[]{}()|&".contains(ch))
}

pub(super) fn parse(
    source: &str,
    text: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Task>, String> {
    // `make -p` still evaluates $(shell ...); discovery must scan declarations without invoking Make.
    let mut names = BTreeSet::new();
    let mut logical = String::new();
    let mut definitions = 0usize;
    let mut conditionals = 0usize;
    let mut prefix = '\t';
    for physical in text.lines() {
        logical.push_str(physical);
        let slashes = physical
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count();
        if slashes % 2 == 1 {
            logical.pop();
            logical.push(' ');
            continue;
        }
        let raw = std::mem::take(&mut logical);
        if raw.starts_with(prefix) {
            continue;
        }
        let line = raw.split('#').next().unwrap_or_default().trim();
        let first = line.split_whitespace().next().unwrap_or_default();
        if first == "define"
            || line.starts_with("override define ")
            || line.starts_with("export define ")
        {
            definitions += 1;
            warning(
                warnings,
                source,
                "Macro-generated and dynamic targets are not expanded during discovery",
            );
            continue;
        }
        if first == "endef" {
            definitions = definitions.saturating_sub(1);
            continue;
        }
        if definitions > 0 {
            continue;
        }
        match first {
            "ifdef" | "ifndef" | "ifeq" | "ifneq" => {
                conditionals += 1;
                warning(
                    warnings,
                    source,
                    "Conditional targets are not evaluated during discovery",
                );
                continue;
            }
            "endif" => {
                conditionals = conditionals.saturating_sub(1);
                continue;
            }
            "else" => continue,
            _ => {}
        }
        if conditionals > 0 {
            continue;
        }
        if matches!(first, "include" | "-include" | "sinclude") {
            warning(
                warnings,
                source,
                "Included makefiles are not expanded; only targets in this file are listed",
            );
            continue;
        }
        if let Some(value) = line
            .strip_prefix(".RECIPEPREFIX")
            .and_then(|rest| rest.split_once('=').map(|(_, value)| value.trim()))
        {
            if value.contains('$') {
                warning(
                    warnings,
                    source,
                    "Dynamic recipe prefixes are unsupported; later targets were skipped",
                );
                break;
            }
            prefix = value.chars().next().unwrap_or('\t');
            continue;
        }
        let Some((left, right)) = line.split_once(':') else {
            continue;
        };
        let left = left.trim();
        if left.contains('=')
            || right.starts_with('=')
            || right.starts_with(":=")
            || right.starts_with("::=")
        {
            continue;
        }
        if left == ".PHONY" {
            for name in right.split_whitespace() {
                if literal(name) {
                    names.insert(name.to_owned());
                } else {
                    warning(
                        warnings,
                        source,
                        "Some dynamic or escaped targets could not be discovered",
                    );
                }
            }
            continue;
        }
        if left.starts_with('.') {
            continue;
        }
        // A target-specific assignment changes variables; it does not itself declare a runnable rule.
        let prerequisite = right
            .trim_start_matches(':')
            .split(';')
            .next()
            .unwrap_or_default();
        if prerequisite
            .split_whitespace()
            .take(2)
            .any(|word| word.contains('='))
        {
            continue;
        }
        for name in left.split_whitespace().filter(|name| *name != "&") {
            if literal(name) {
                names.insert(name.to_owned());
            } else {
                warning(
                    warnings,
                    source,
                    "Some dynamic or escaped targets could not be discovered",
                );
            }
        }
    }
    if definitions > 0 || conditionals > 0 || !logical.is_empty() {
        warning(
            warnings,
            source,
            "Unfinished macro, conditional or continuation; task discovery may be incomplete",
        );
    }
    Ok(names
        .iter()
        .take(MAX_TASKS + 1)
        .map(|name| task(source, name))
        .collect())
}
