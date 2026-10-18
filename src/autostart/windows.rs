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
    match result {
        Ok(value) => Ok(Some(value)),
        // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND) loses its NotFound kind when converted to io::Error.
        Err(error) if error.code().0 == 0x8007_0002_u32 as i32 => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_registry_entries_are_absent() {
        let error = io::Error::from_raw_os_error(2).into();
        assert_eq!(optional::<()>(Err(error)).unwrap(), None);
    }

    #[test]
    fn present_registry_values_and_other_errors_are_preserved() {
        assert_eq!(
            optional(Ok("synthetic command")).unwrap(),
            Some("synthetic command")
        );
        for code in [5, 13] {
            let result: windows_registry::Result<()> =
                Err(io::Error::from_raw_os_error(code).into());
            let original = result.as_ref().unwrap_err().code().0;
            assert_eq!(optional(result).unwrap_err().raw_os_error(), Some(original));
        }
    }

    #[test]
    fn missing_keys_and_values_are_absent_in_windows_registry() {
        let root = tempfile::tempdir().unwrap();
        let path = format!(
            r"Software\Tervia-tests-{}",
            root.path().file_name().unwrap().to_str().unwrap()
        );
        let key = CURRENT_USER.create(&path).unwrap();
        let result = (|| -> io::Result<()> {
            assert!(optional(key.open("missing-key"))?.is_none());
            assert!(optional(key.get_string("missing-value"))?.is_none());
            assert!(optional(key.remove_value("missing-value"))?.is_none());
            key.set_string("synthetic-value", "synthetic command")?;
            assert_eq!(
                optional(key.get_string("synthetic-value"))?,
                Some("synthetic command".into())
            );
            Ok(())
        })();
        drop(key);
        CURRENT_USER.remove_tree(&path).unwrap();
        result.unwrap();
    }
}
