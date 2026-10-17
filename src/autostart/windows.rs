use std::io;
use windows_registry::CURRENT_USER;

const NAME: &str = "Tervia";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

pub(super) fn configure(enabled: Option<bool>) -> io::Result<bool> {
    if let Some(enabled) = enabled {
        if enabled {
            let executable = std::env::current_exe()?;
            let path = executable.to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The executable path is not UTF-8",
                )
            })?;
            CURRENT_USER
                .create(RUN)?
                .set_string(NAME, super::windows_command(path))?;
            // Clear only our override so enabling works after a Task Manager disable.
            remove_value(APPROVED)?;
        } else {
            remove_value(RUN)?;
        }
    }
    let registered =
        optional(CURRENT_USER.open(RUN).and_then(|key| key.get_string(NAME)))?.is_some();
    if !registered {
        return Ok(false);
    }
    let approved = optional(
        CURRENT_USER
            .open(APPROVED)
            .and_then(|key| key.get_value(NAME)),
    )?;
    Ok(approved
        .as_ref()
        .is_none_or(|value| super::windows_approved(value)))
}

fn remove_value(path: &str) -> io::Result<()> {
    optional(
        CURRENT_USER
            .options()
            .write()
            .open(path)
            .and_then(|key| key.remove_value(NAME)),
    )?;
    Ok(())
}

fn optional<T>(result: windows_registry::Result<T>) -> io::Result<Option<T>> {
    match result.map_err(io::Error::from) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
