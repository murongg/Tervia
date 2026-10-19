use super::*;
use std::collections::BTreeMap;

pub(super) fn parse(root: &Path, text: &str) -> Result<Vec<Task>, String> {
    let document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())?;
    if !document.contains_key("package") && !document.contains_key("workspace") {
        return Ok(Vec::new());
    }
    let make = |args: Vec<String>| Task {
        label: format!("cargo {}", args.join(" ")),
        source: "Cargo.toml".into(),
        program: "cargo".into(),
        args,
    };
    let mut tasks: Vec<_> = ["build", "check", "test"]
        .into_iter()
        .map(|command| make(vec![command.into()]))
        .collect();
    let mut bins = BTreeMap::new();
    if let Some(targets) = document
        .get("bin")
        .and_then(toml_edit::Item::as_array_of_tables)
    {
        for target in targets {
            if let Some(name) = target
                .get("name")
                .and_then(toml_edit::Item::as_str)
                .filter(|name| valid_text(name) && !name.starts_with('-'))
            {
                let features = target
                    .get("required-features")
                    .and_then(toml_edit::Item::as_array)
                    .map(|features| {
                        features
                            .iter()
                            .filter_map(|feature| feature.as_str().filter(|name| valid_text(name)))
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                bins.insert(name.to_owned(), features);
            }
        }
    }
    let package = document.get("package");
    let autobins = package
        .and_then(|package| package.get("autobins"))
        .and_then(toml_edit::Item::as_bool)
        .unwrap_or(true);
    if autobins
        && root.join("src/main.rs").is_file()
        && let Some(name) = package
            .and_then(|package| package.get("name"))
            .and_then(toml_edit::Item::as_str)
            .filter(|name| valid_text(name))
    {
        bins.entry(name.to_owned()).or_insert_with(Vec::new);
    }
    for (name, features) in bins {
        let mut args = vec!["run".into(), "--bin".into(), name];
        if !features.is_empty() {
            args.extend(["--features".into(), features.join(",")]);
        }
        tasks.push(make(args));
    }
    Ok(tasks)
}
