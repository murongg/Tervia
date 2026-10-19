use super::*;

pub(super) fn parse(
    root: &Path,
    text: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Task>, String> {
    let package: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let Some(scripts) = package.get("scripts") else {
        return Ok(Vec::new());
    };
    let scripts = scripts.as_object().ok_or("scripts must be an object")?;
    let manager = match package.get("packageManager") {
        Some(value) => {
            let manager = value
                .as_str()
                .ok_or("packageManager must be a string")?
                .split('@')
                .next()
                .unwrap_or_default();
            if !matches!(manager, "npm" | "pnpm" | "yarn" | "bun") {
                return Err("Unsupported packageManager; supported: npm, pnpm, yarn, bun".into());
            }
            manager
        }
        None => {
            let found: BTreeSet<_> = [
                ("package-lock.json", "npm"),
                ("npm-shrinkwrap.json", "npm"),
                ("pnpm-lock.yaml", "pnpm"),
                ("yarn.lock", "yarn"),
                ("bun.lock", "bun"),
                ("bun.lockb", "bun"),
            ]
            .into_iter()
            .filter(|(file, _)| root.join(file).is_file())
            .map(|(_, manager)| manager)
            .collect();
            if found.len() > 1 {
                return Err(
                    "Multiple package managers found; set packageManager in package.json".into(),
                );
            }
            found.into_iter().next().unwrap_or("npm")
        }
    };
    if scripts
        .iter()
        .any(|(name, value)| !valid_text(name) || name.starts_with('-') || !value.is_string())
    {
        warnings.push(
            "package.json: Some scripts have invalid names or non-string commands and were skipped"
                .into(),
        );
    }
    let mut tasks: Vec<_> = scripts
        .iter()
        .filter(|(name, value)| valid_text(name) && !name.starts_with('-') && value.is_string())
        .map(|(name, _)| Task {
            label: name.clone(),
            source: "package.json".into(),
            program: manager.into(),
            args: vec!["run".into(), name.clone()],
        })
        .collect();
    // Native dependencies enable serde_json/preserve_order; task order must not depend on feature unification.
    tasks.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(tasks)
}
