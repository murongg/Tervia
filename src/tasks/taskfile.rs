use super::*;

pub(super) const FILES: &[&str] = &[
    "Taskfile.yml",
    "taskfile.yml",
    "Taskfile.yaml",
    "taskfile.yaml",
    "Taskfile.dist.yml",
    "taskfile.dist.yml",
    "Taskfile.dist.yaml",
    "taskfile.dist.yaml",
];

pub(super) fn task(source: &str, name: &str) -> Task {
    // Task treats arguments after `--` as CLI_ARGS for the default task, not as task names.
    Task {
        label: name.into(),
        source: source.into(),
        program: "task".into(),
        args: vec!["--taskfile".into(), source.into(), name.into()],
    }
}

fn literal(name: &str) -> bool {
    valid_goal(name) && !name.contains(['*', '?', '[', ']', '{', '}'])
}

pub(super) fn parse(
    source: &str,
    text: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Task>, String> {
    let mut budget = serde_saphyr::Budget::default();
    budget.max_depth = 32;
    budget.max_nodes = 25000;
    budget.max_events = 50000;
    budget.max_aliases = 128;
    budget.max_anchors = 128;
    budget.max_recorded_anchor_bytes = MAX_MANIFEST as usize;
    budget.max_total_scalar_bytes = 2 * MAX_MANIFEST as usize;
    let mut options = serde_saphyr::Options::default();
    options.budget = Some(budget);
    options.emit_comments = false;
    options.with_snippet = false;
    // Do not install file/property resolvers, even when native dependencies enable those parser features.
    let document: Value =
        serde_saphyr::from_str_with_options(text, options).map_err(|error| error.to_string())?;
    let version = &document["version"];
    if version.as_u64() != Some(3)
        && !version
            .as_str()
            .is_some_and(|version| version == "3" || version.starts_with("3."))
    {
        return Err("Expected a version 3 Taskfile".into());
    }
    if document.get("includes").is_some_and(|includes| {
        !includes.is_null()
            && includes
                .as_object()
                .is_none_or(|includes| !includes.is_empty())
    }) {
        warning(
            warnings,
            source,
            "Included Taskfiles are not expanded; only root task declarations are listed",
        );
    }
    let Some(tasks) = document.get("tasks") else {
        return Ok(Vec::new());
    };
    let tasks = tasks.as_object().ok_or("tasks must be a mapping")?;
    let mut names = BTreeSet::new();
    let mut aliases = Vec::new();
    for (name, definition) in tasks {
        if !literal(name) {
            warning(
                warnings,
                source,
                "Dynamic or invalid task names were skipped",
            );
            continue;
        }
        if !definition.is_object() && !definition.is_string() && !definition.is_array() {
            warning(
                warnings,
                source,
                "Some tasks have unsupported definitions and were skipped",
            );
            continue;
        }
        if definition.get("internal").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if definition
            .get("internal")
            .is_some_and(|value| !value.is_boolean())
        {
            warning(
                warnings,
                source,
                "Some tasks have invalid internal flags and were skipped",
            );
            continue;
        }
        // Required variables may already come from Taskfile defaults or shell startup.
        // Leave those checks to Task at execution time instead of hiding valid named tasks.
        names.insert(name.to_owned());
        if let Some(values) = definition.get("aliases") {
            if let Some(values) = values.as_array() {
                for alias in values {
                    if let Some(alias) = alias.as_str().filter(|alias| literal(alias)) {
                        aliases.push(alias.to_owned());
                    } else {
                        warning(
                            warnings,
                            source,
                            "Some task aliases are invalid and were skipped",
                        );
                    }
                }
            } else {
                warning(warnings, source, "Task aliases must be a list of names");
            }
        }
    }
    for alias in aliases {
        if tasks.contains_key(&alias) || !names.insert(alias) {
            warning(warnings, source, "Conflicting task aliases were skipped");
        }
    }
    Ok(names
        .iter()
        .take(MAX_TASKS + 1)
        .map(|name| task(source, name))
        .collect())
}
