use super::*;
use crate::state::{File, ReadingPosition, Session, Tab, Terminals};

pub(super) fn snapshot(
    workspace: &Workspace,
    orphans: &[Value],
) -> Result<Vec<Value>, serde_json::Error> {
    let mut contexts = Vec::new();
    for (index, project) in workspace.projects.iter().enumerate() {
        if !workspace.project_available(index) {
            continue;
        }
        if project.terminal.sessions.len() > 64
            || project.open_files.len() > 512
            || project.terminal.sessions.iter().any(|session| {
                session.name.len() > 1024
                    || session
                        .custom_name
                        .as_ref()
                        .is_some_and(|name| !Session::valid_custom_name(name))
                    || session
                        .connection
                        .as_ref()
                        .is_some_and(|id| !Session::valid_connection(id))
                    || session.scroll_offset > 10000
                    || session
                        .task
                        .as_ref()
                        .is_some_and(|task| crate::tasks::Task::from_json(&task.to_json()).is_err())
                    || (session.task.is_some() && session.connection.is_none())
            })
        {
            return Err(serde_json::Error::io(io::Error::new(
                io::ErrorKind::InvalidData,
                "Recovery supports 64 terminal tabs and 512 open files per directory",
            )));
        }
        if project.open_files.is_empty()
            && project.terminal.sessions.len() == 1
            // A lone renamed tab still needs a recovery context in the default Git layout.
            && project.terminal.sessions[0].custom_name.is_none()
            && project.terminal.sessions[0].connection.is_none()
            && project.terminal.sessions[0].task.is_none()
            && project.terminal.sessions[0].scroll_offset == 0
            && !project.dock.visible
            && project.tab == Tab::Git
            && project.positions.is_empty()
        {
            continue;
        }
        let mut files = Vec::new();
        for index in &project.open_files {
            let Some(file) = project.files.get(*index) else {
                continue;
            };
            let position = project
                .positions
                .get(&file.disk_path)
                .copied()
                .unwrap_or_default();
            if file.saved.len() > 64 * 1024 * 1024
                || file.draft.len() > 64 * 1024 * 1024
                || !position.scroll_x.is_finite()
                || !position.scroll_y.is_finite()
            {
                return Err(serde_json::Error::io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Recovery draft or position exceeds its supported range",
                )));
            }
            let mut saved = json!({
                "path": serde_json::to_value(&file.disk_path)?, "language": file.language,
                "position": { "line": position.line, "column": position.column,
                    "scroll_x": position.scroll_x, "scroll_y": position.scroll_y, "source": position.source }
            });
            if file.dirty() {
                saved["saved"] = json!(file.saved);
                saved["draft"] = json!(file.draft);
            }
            files.push(saved);
        }
        contexts.push(json!({
            "root": serde_json::to_value(&project.root)?,
            "owner": serde_json::to_value(workspace.folder(index).map(|owner| &workspace.projects[owner].root).unwrap_or(&project.root))?,
            "tab": match project.tab { Tab::Terminal => "terminal", Tab::Git => "git", Tab::Files => "files" },
            "dock": { "visible": project.dock.visible, "size": project.dock.size },
            "sessions": project.terminal.sessions.iter().map(|session| &session.name).collect::<Vec<_>>(),
            "custom_names": project.terminal.sessions.iter().map(|session| &session.custom_name).collect::<Vec<_>>(),
            "connections": project.terminal.sessions.iter().map(|session| &session.connection).collect::<Vec<_>>(),
            "scroll_offsets": project.terminal.sessions.iter().map(|session| session.scroll_offset).collect::<Vec<_>>(),
            "tasks": project.terminal.sessions.iter().map(|session| session.task.as_ref().map(crate::tasks::Task::to_json)).collect::<Vec<_>>(),
            "active_session": project.terminal.active_session, "focused_session": project.terminal.focused_session,
            "split_terminal": project.terminal.split_terminal, "files": files,
            "active_file": project.current_file().map(|file| serde_json::to_value(&file.disk_path)).transpose()?
        }));
    }
    if contexts.len() + orphans.len() > 4096 {
        return Err(serde_json::Error::io(io::Error::new(
            io::ErrorKind::InvalidData,
            "Too many recovery directories",
        )));
    }
    // Unavailable directories must retain both drafts and ownership of still-running terminal hosts.
    for orphan in orphans {
        if !contexts
            .iter()
            .any(|context| context["root"] == orphan["root"])
        {
            contexts.push(orphan.clone());
        }
    }
    Ok(contexts)
}

pub(super) fn restore(saved: &Value, workspace: &mut Workspace) -> Result<Vec<Value>, String> {
    let Some(contexts) = saved.get("contexts") else {
        return Ok(Vec::new());
    };
    let contexts = contexts.as_array().ok_or("contexts must be an array")?;
    if contexts.len() > 4096 {
        return Err("Too many recovery directories".into());
    }
    let mut orphans = Vec::new();
    let mut roots = std::collections::BTreeSet::new();
    for context in contexts {
        let root = directory(context.get("root").ok_or("Recovery root is missing")?)?;
        let root = if root.is_dir() {
            root.canonicalize().map_err(|error| error.to_string())?
        } else {
            root
        };
        if !roots.insert(root.clone()) {
            return Err("Duplicate recovery directory".into());
        }
        let tab = match context.get("tab").and_then(Value::as_str).unwrap_or("git") {
            "terminal" => Tab::Terminal,
            "git" => Tab::Git,
            "files" => Tab::Files,
            _ => return Err("Invalid recovery module".into()),
        };
        let terminal = terminals(context)?;
        let dock = context.get("dock");
        let visible = boolean(dock.and_then(|dock| dock.get("visible")), false)?;
        let size = number(dock.and_then(|dock| dock.get("size")), 220, 800)?.max(120) as u32;
        let stored_files = context
            .get("files")
            .and_then(Value::as_array)
            .ok_or("Recovery files must be an array")?;
        if stored_files.len() > 512 {
            return Err("Too many recovery files".into());
        }
        let mut files = Vec::new();
        let mut positions = std::collections::BTreeMap::new();
        let mut restoring = std::collections::BTreeSet::new();
        for saved in stored_files {
            let path = relative(saved.get("path").ok_or("Recovery file path is missing")?)?;
            if positions.contains_key(&path) {
                return Err("Duplicate recovery file".into());
            }
            let language = saved
                .get("language")
                .and_then(Value::as_str)
                .filter(|language| language.len() <= 128)
                .unwrap_or("text");
            let (base, draft) = match (saved.get("saved"), saved.get("draft")) {
                (None, None) => ("", ""),
                (Some(base), Some(draft)) => (source(base)?, source(draft)?),
                _ => return Err("Incomplete recovery draft".into()),
            };
            let position = saved.get("position");
            positions.insert(
                path.clone(),
                ReadingPosition {
                    line: number(
                        position.and_then(|position| position.get("line")),
                        0,
                        u32::MAX as u64,
                    )? as u32,
                    column: number(
                        position.and_then(|position| position.get("column")),
                        0,
                        u32::MAX as u64,
                    )? as u32,
                    scroll_x: offset(position.and_then(|position| position.get("scroll_x")))?,
                    scroll_y: offset(position.and_then(|position| position.get("scroll_y")))?,
                    source: boolean(position.and_then(|position| position.get("source")), false)?,
                },
            );
            if base == draft {
                restoring.insert(path.clone());
            }
            files.push(File {
                path: path.to_string_lossy().into_owned(),
                disk_path: path,
                language: language.into(),
                saved: base.into(),
                draft: draft.into(),
            });
        }
        if !root.is_dir() {
            if files.iter().any(File::dirty)
                || terminal
                    .sessions
                    .iter()
                    .any(|session| session.connection.is_some())
            {
                orphans.push(context.clone());
            }
            continue;
        }
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let index = workspace.register_directory(root);
        if let Some(owner) = context.get("owner") {
            let owner = directory(owner)?;
            if owner.is_dir() {
                let owner = owner.canonicalize().map_err(|error| error.to_string())?;
                if workspace.projects[index].root != owner
                    && workspace.projects[index].root.starts_with(&owner)
                {
                    let parent = workspace.register_directory(owner);
                    workspace.attach_directory(parent, index);
                }
            }
        }
        let active_file = context
            .get("active_file")
            .filter(|file| !file.is_null())
            .map(relative)
            .transpose()?;
        let project = &mut workspace.projects[index];
        project.tab = tab;
        project.dock = crate::state::Dock { visible, size };
        project.terminal = terminal;
        project.active_file = active_file
            .and_then(|path| files.iter().position(|file| file.disk_path == path))
            .unwrap_or(0);
        project.open_files = (0..files.len()).collect();
        project.files = files;
        project.positions = positions;
        project.restoring = restoring;
    }
    Ok(orphans)
}

pub(super) fn terminals(context: &Value) -> Result<Terminals, String> {
    let sessions = context
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or("Recovery sessions must be an array")?;
    if sessions.len() > 64 {
        return Err("Invalid recovery terminal count".into());
    }
    // Optional parallel aliases keep the existing sessions array readable by older workspaces.
    let custom_names = context
        .get("custom_names")
        .map(|names| {
            names
                .as_array()
                .filter(|names| names.len() == sessions.len())
                .ok_or("Invalid terminal custom names")
        })
        .transpose()?;
    let parallel = |key: &str| -> Result<Option<&Vec<Value>>, String> {
        context
            .get(key)
            .map(|values| {
                values
                    .as_array()
                    .filter(|values| values.len() == sessions.len())
                    .ok_or_else(|| format!("Invalid terminal {key}"))
            })
            .transpose()
    };
    let connections = parallel("connections")?;
    let offsets = parallel("scroll_offsets")?;
    let tasks = parallel("tasks")?;
    let mut identities = std::collections::BTreeSet::new();
    let sessions = sessions
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let name = name
                .as_str()
                .filter(|name| name.len() <= 1024)
                .ok_or("Invalid terminal name")?;
            let custom_name = custom_names
                .map(|names| &names[index])
                .filter(|name| !name.is_null())
                .map(|name| {
                    name.as_str()
                        .filter(|name| Session::valid_custom_name(name))
                        .map(str::trim)
                        .ok_or("Invalid terminal custom name")
                })
                .transpose()?
                .filter(|name| !name.is_empty())
                .map(str::to_owned);
            Ok(Session {
                name: name.into(),
                custom_name,
                connection: connections
                    .map(|values| &values[index])
                    .filter(|id| !id.is_null())
                    .map(|id| {
                        id.as_str()
                            .filter(|id| {
                                Session::valid_connection(id) && identities.insert((*id).to_owned())
                            })
                            .map(str::to_owned)
                            .ok_or("Invalid or duplicate terminal connection")
                    })
                    .transpose()?,
                scroll_offset: number(offsets.map(|values| &values[index]), 0, 10000)? as usize,
                task: tasks
                    .map(|values| &values[index])
                    .filter(|task| !task.is_null())
                    .map(crate::tasks::Task::from_json)
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    // Recovery may reconnect a task, but must never turn missing host metadata into permission to execute it again.
    if sessions
        .iter()
        .any(|session| session.task.is_some() && session.connection.is_none())
    {
        return Err("A recovered task must have a terminal connection".into());
    }
    let active = number(context.get("active_session"), 0, 63)? as usize;
    let focused = number(context.get("focused_session"), active as u64, 63)? as usize;
    if active > sessions.len().saturating_sub(1) || focused > sessions.len().saturating_sub(1) {
        return Err("Invalid active terminal".into());
    }
    let split = boolean(context.get("split_terminal"), false)?;
    Ok(Terminals {
        active_session: active,
        focused_session: focused,
        split_terminal: split && sessions.len() > 1,
        sessions,
    })
}

fn source(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|source| source.len() <= 64 * 1024 * 1024)
        .ok_or_else(|| "Invalid or oversized recovery draft".into())
}
fn relative(value: &Value) -> Result<PathBuf, String> {
    let path = PathBuf::from(
        value
            .as_str()
            .ok_or("Recovery file path must be a string")?,
    );
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("Recovery file is outside its directory".into());
    }
    Ok(path)
}
fn number(value: Option<&Value>, default: u64, maximum: u64) -> Result<u64, String> {
    value
        .map(|value| {
            value
                .as_u64()
                .filter(|value| *value <= maximum)
                .ok_or_else(|| "Invalid recovery number".into())
        })
        .unwrap_or(Ok(default))
}
fn boolean(value: Option<&Value>, default: bool) -> Result<bool, String> {
    value
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| "Invalid recovery boolean".into())
        })
        .unwrap_or(Ok(default))
}
fn offset(value: Option<&Value>) -> Result<f32, String> {
    value
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && value.abs() <= 10_000_000.0)
                .map(|value| value as f32)
                .ok_or_else(|| "Invalid reading position".into())
        })
        .unwrap_or(Ok(0.0))
}
