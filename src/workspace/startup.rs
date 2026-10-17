use super::*;
use crate::state::Screen;

pub(super) fn snapshot(workspace: &Workspace) -> Result<Value, serde_json::Error> {
    let terminal = &workspace.local_terminal;
    let saved = json!({
        "sessions": terminal.sessions.iter().map(|session| &session.name).collect::<Vec<_>>(),
        "custom_names": terminal.sessions.iter().map(|session| &session.custom_name).collect::<Vec<_>>(),
        "active_session": terminal.active_session,
        "focused_session": terminal.focused_session,
        "split_terminal": terminal.split_terminal,
    });
    recovery::terminals(&saved, true).map_err(|error| {
        serde_json::Error::io(io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    Ok(json!({
        "screen": match workspace.screen {
            Screen::Welcome => "welcome", Screen::Local => "local", Screen::Project => "project",
        },
        "local_terminal": saved,
    }))
}

pub(super) fn restore(saved: &Value, workspace: &mut Workspace) -> Result<(), String> {
    let Some(startup) = saved.get("startup") else {
        return Ok(());
    };
    workspace.local_terminal = recovery::terminals(
        startup
            .get("local_terminal")
            .ok_or("Local terminal recovery is missing")?,
        true,
    )?;
    match startup.get("screen").and_then(Value::as_str) {
        Some("welcome") => workspace.screen = Screen::Welcome,
        Some("local") => workspace.show_local_terminal(),
        Some("project") => {
            let active = saved
                .get("active")
                .filter(|value| !value.is_null())
                .map(directory)
                .transpose()?;
            let available = active.as_ref().is_some_and(|path| path.is_dir())
                || active.as_ref().is_some_and(|active| {
                    // An unavailable linked worktree may still restore through its saved main directory.
                    saved
                        .get("projects")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .any(|project| {
                            project
                                .get("directory")
                                .and_then(|value| directory(value).ok())
                                .as_ref()
                                == Some(active)
                                && project
                                    .get("root")
                                    .and_then(|value| directory(value).ok())
                                    .is_some_and(|root| root.is_dir())
                        })
                });
            if !workspace.has_project() || !available {
                workspace.screen = Screen::Welcome;
                workspace.notice = "startup_project_unavailable".into();
            }
        }
        _ => return Err("Invalid startup screen".into()),
    }
    Ok(())
}
