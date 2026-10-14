use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::state::Workspace;

/// App-owned project history, stored separately from the user-editable configuration.
pub struct Store {
    path: PathBuf,
    source: Option<String>,
    saved: Option<Value>,
    fallbacks: BTreeMap<PathBuf, PathBuf>,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            source: None,
            saved: None,
            fallbacks: BTreeMap::new(),
        }
    }

    pub fn load(&mut self) -> Result<Workspace, String> {
        self.saved = None;
        self.source = None;
        self.fallbacks.clear();
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => Some(source),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(self.error(error)),
        };
        let (workspace, fallbacks) = match &source {
            Some(source) => restore(source).map_err(|error| self.error(error))?,
            None => (Workspace::empty(), BTreeMap::new()),
        };
        self.saved = Some(snapshot(&workspace, &fallbacks).map_err(|error| self.error(error))?);
        self.fallbacks = fallbacks;
        self.source = source;
        Ok(workspace)
    }

    pub fn save(&mut self, workspace: &Workspace) -> Result<(), String> {
        if self.saved.is_none() {
            return Err(self.error("Workspace was not loaded; fix the file and restart Tervia"));
        }
        let snapshot = snapshot(workspace, &self.fallbacks).map_err(|error| self.error(error))?;
        // UI notifications also cover terminal output and Git polling; unchanged history needs no IO.
        if self.saved.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        if self.current()? != self.source {
            return Err(
                self.error("Workspace changed on disk; restart Tervia before saving projects")
            );
        }
        let target = if self.source.is_some() {
            fs::canonicalize(&self.path).map_err(|error| self.error(error))?
        } else {
            self.path.clone()
        };
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|error| self.error(error))?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| self.error(error))?;
        if self.source.is_some() {
            temporary
                .as_file()
                .set_permissions(
                    fs::metadata(&target)
                        .map_err(|error| self.error(error))?
                        .permissions(),
                )
                .map_err(|error| self.error(error))?;
        }
        let updated = serde_json::to_string_pretty(&snapshot).map_err(|error| self.error(error))?;
        temporary
            .write_all(updated.as_bytes())
            .map_err(|error| self.error(error))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| self.error(error))?;
        if self.current()? != self.source {
            return Err(
                self.error("Workspace changed on disk; restart Tervia before saving projects")
            );
        }
        if self.source.is_none() {
            // Two first launches must not silently replace each other's initial project lists.
            temporary
                .persist_noclobber(&target)
                .map_err(|error| self.error(error))?;
        } else {
            temporary
                .persist(&target)
                .map_err(|error| self.error(error))?;
        }
        self.source = Some(updated);
        self.saved = Some(snapshot);
        Ok(())
    }

    fn current(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.path) {
            Ok(source) => Ok(Some(source)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(self.error(error)),
        }
    }

    fn error(&self, error: impl std::fmt::Display) -> String {
        format!("{}: {error}", self.path.display())
    }
}

fn snapshot(
    workspace: &Workspace,
    fallbacks: &BTreeMap<PathBuf, PathBuf>,
) -> Result<Value, serde_json::Error> {
    let projects = workspace
        .project_rows()
        .into_iter()
        .map(|index| {
            let project = &workspace.projects[index];
            // Startup Git discovery is asynchronous; early UI saves must retain the main-directory fallback.
            let root = if project.repository.is_none() {
                fallbacks
                    .get(&project.root)
                    .unwrap_or(workspace.project_root(index))
            } else {
                workspace.project_root(index)
            };
            let root = serde_json::to_value(root)?;
            let directory =
                serde_json::to_value(&workspace.projects[workspace.project_target(index)].root)?;
            let mut saved = json!({ "root": root, "directory": directory });
            if workspace.folder(index) == Some(index) {
                saved["folder"] = json!(true);
                saved["children_expanded"] = json!(workspace.folder_expanded(index));
            }
            Ok(saved)
        })
        .collect::<Result<Vec<Value>, serde_json::Error>>()?;
    let active = workspace
        .projects
        .get(workspace.active_project)
        .map(|project| &project.root);
    Ok(json!({
        "version": 1,
        "projects": projects,
        "active": serde_json::to_value(active)?,
        "sidebar_expanded": workspace.sidebar_expanded,
    }))
}

fn restore(source: &str) -> Result<(Workspace, BTreeMap<PathBuf, PathBuf>), String> {
    let saved: Value =
        serde_json::from_str(source).map_err(|error| format!("Invalid workspace JSON: {error}"))?;
    if saved.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported workspace version".into());
    }
    let projects = saved
        .get("projects")
        .and_then(Value::as_array)
        .ok_or("projects must be an array")?;
    let active = saved
        .get("active")
        .filter(|value| !value.is_null())
        .map(directory)
        .transpose()?;
    let mut workspace = Workspace::empty();
    workspace.sidebar_expanded = match saved.get("sidebar_expanded") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or("sidebar_expanded must be a boolean")?,
    };
    let mut selected = 0;
    let mut fallbacks = BTreeMap::new();
    for project in projects {
        let root = directory(project.get("root").ok_or("Project root is missing")?)?;
        let target = directory(
            project
                .get("directory")
                .ok_or("Project directory is missing")?,
        )?;
        let folder = project
            .get("folder")
            .map(|value| value.as_bool().ok_or("folder must be a boolean"))
            .transpose()?
            .unwrap_or(false);
        if folder {
            let parent = if root.is_dir() { &root } else { &target };
            if !parent.is_dir() {
                continue;
            }
            let parent = fs::canonicalize(parent).map_err(|error| error.to_string())?;
            let parent_index = workspace.register_directory(parent);
            let index = if target.is_dir() {
                workspace.register_directory(
                    fs::canonicalize(&target).map_err(|error| error.to_string())?,
                )
            } else {
                parent_index
            };
            workspace.group_folder(parent_index, &[index]);
            let expanded = project
                .get("children_expanded")
                .map(|value| value.as_bool().ok_or("children_expanded must be a boolean"))
                .transpose()?
                .unwrap_or(true);
            if !expanded {
                workspace.toggle_folder(parent_index);
            }
            workspace.switch_project(index);
            if active.as_ref() == Some(&target) || active.as_ref() == Some(&root) {
                selected = index;
            }
            continue;
        }
        let available = if target.is_dir() { &target } else { &root };
        if !available.is_dir() {
            continue;
        }
        let available = fs::canonicalize(available).map_err(|error| error.to_string())?;
        let index = workspace.register_directory(available.clone());
        if active.as_ref() == Some(&target) || active.as_ref() == Some(&root) {
            selected = index;
        }
        fallbacks.entry(available).or_insert(root);
    }
    workspace.switch_project(selected);
    Ok((workspace, fallbacks))
}

fn directory(value: &Value) -> Result<PathBuf, String> {
    let path = PathBuf::from(value.as_str().ok_or("Project directory must be a string")?);
    if !path.is_absolute() {
        return Err("Project directory must be an absolute path".into());
    }
    Ok(path)
}
