//! Project task discovery reads manifests only; execution always requires an explicit launch.
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs::File, io::Read, path::Path};

mod cargo;
mod just;
mod make;
mod package;
mod taskfile;

const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_TASKS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub label: String,
    pub source: String,
    pub program: String,
    pub args: Vec<String>,
}

impl Task {
    pub fn display(&self) -> String {
        shlex::try_join(
            std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str)),
        )
        .unwrap_or_default()
    }

    pub fn to_json(&self) -> Value {
        json!({"label": self.label, "source": self.source, "program": self.program, "args": self.args})
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        let field = |key: &str| {
            value[key]
                .as_str()
                .filter(|text| valid_text(text))
                .map(str::to_owned)
                .ok_or_else(|| format!("Invalid task {key}"))
        };
        let task = Self {
            label: field("label")?,
            source: field("source")?,
            program: field("program")?,
            args: value["args"]
                .as_array()
                .filter(|args| !args.is_empty() && args.len() <= 16)
                .ok_or("Invalid task arguments")?
                .iter()
                .map(|arg| {
                    arg.as_str()
                        .filter(|arg| valid_text(arg))
                        .map(str::to_owned)
                        .ok_or("Invalid task argument")
                })
                .collect::<Result<_, _>>()?,
        };
        let supported = match task.program.as_str() {
            "npm" | "pnpm" | "yarn" | "bun" => task.source == "package.json",
            "cargo" => task.source == "Cargo.toml",
            "make" => {
                make::FILES.contains(&task.source.as_str())
                    && valid_goal(&task.label)
                    && make::task(&task.source, &task.label) == task
            }
            "just" => {
                just::is_source(&task.source)
                    && valid_goal(&task.label)
                    && just::task(&task.source, &task.label) == task
            }
            "task" => {
                taskfile::FILES.contains(&task.source.as_str())
                    && valid_goal(&task.label)
                    && taskfile::task(&task.source, &task.label) == task
            }
            _ => false,
        };
        if !supported {
            return Err("Unsupported project task".into());
        }
        Ok(task)
    }
}

fn valid_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= 1024 && !text.chars().any(char::is_control)
}

fn valid_goal(name: &str) -> bool {
    valid_text(name) && !name.starts_with('-') && !name.contains('=')
}

fn warning(warnings: &mut Vec<String>, source: &str, message: &str) {
    let message = format!("{source}: {message}");
    if !warnings.contains(&message) {
        warnings.push(message);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub tasks: Vec<Task>,
    pub warnings: Vec<String>,
}

pub fn discover(root: &Path) -> Catalog {
    let mut catalog = Catalog::default();
    for source in ["package.json", "Cargo.toml"] {
        let result = (|| {
            let Some(text) = manifest(&root.join(source))? else {
                return Ok(Vec::new());
            };
            match source {
                "package.json" => package::parse(root, &text, &mut catalog.warnings),
                _ => cargo::parse(root, &text),
            }
        })();
        match result {
            Ok(tasks) => catalog.tasks.extend(tasks),
            Err(error) => catalog.warnings.push(format!("{source}: {error}")),
        }
    }
    let files = source_files(root, &mut catalog.warnings);
    for (kind, selected) in [
        ("Makefile", Ok(select(&files, make::FILES))),
        ("justfile", just::select(&files)),
        ("Taskfile", Ok(select(&files, taskfile::FILES))),
    ] {
        let source = match selected {
            Ok(Some(source)) => source,
            Ok(None) => continue,
            Err(error) => {
                warning(&mut catalog.warnings, kind, &error);
                continue;
            }
        };
        let result = (|| {
            let Some(text) = manifest(&root.join(&source))? else {
                return Err("Task source disappeared; refresh to rediscover tasks".into());
            };
            match kind {
                "Makefile" => make::parse(&source, &text, &mut catalog.warnings),
                "justfile" => just::parse(&source, &text, &mut catalog.warnings),
                _ => taskfile::parse(&source, &text, &mut catalog.warnings),
            }
        })();
        match result {
            Ok(tasks) => catalog.tasks.extend(tasks),
            Err(error) => warning(&mut catalog.warnings, &source, &error),
        }
    }
    if catalog.tasks.len() > MAX_TASKS {
        catalog.tasks.truncate(MAX_TASKS);
        catalog
            .warnings
            .push(format!("Only the first {MAX_TASKS} tasks are shown"));
    }
    catalog
}

fn manifest(path: &Path) -> Result<Option<String>, String> {
    // A byte limit does not prevent opening a FIFO from blocking task discovery.
    match path.metadata() {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err("Task source must be a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut text = String::new();
    file.take(MAX_MANIFEST + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > MAX_MANIFEST {
        return Err("Manifest exceeds 1 MiB".into());
    }
    Ok(Some(text))
}

fn select(files: &BTreeSet<String>, candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|name| files.contains(**name))
        .map(|name| (*name).to_owned())
}

fn source_files(root: &Path, warnings: &mut Vec<String>) -> BTreeSet<String> {
    let entries = match root.read_dir() {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return BTreeSet::new(),
        Err(error) => {
            warning(warnings, "Task discovery", &error.to_string());
            return BTreeSet::new();
        }
    };
    let mut files = BTreeSet::new();
    for (index, entry) in entries.enumerate() {
        if index == 8192 {
            warning(
                warnings,
                "Task discovery",
                "Directory scan stopped after 8192 entries; some task files may be missing",
            );
            break;
        }
        match entry {
            Ok(entry) => {
                if let Some(name) = entry.file_name().to_str() {
                    files.insert(name.to_owned());
                }
            }
            Err(error) => warning(warnings, "Task discovery", &error.to_string()),
        }
    }
    files
}
