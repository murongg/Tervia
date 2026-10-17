#[cfg(any(not(target_os = "windows"), test))]
use std::{fs, io::Write, path::Path};

#[cfg(any(not(target_os = "windows"), test))]
use crate::platform::Platform;

/// Read the OS registration, or change it and return the resulting state.
pub fn configure(enabled: Option<bool>) -> Result<bool, String> {
    configure_system(enabled).map_err(|error| format!("Autostart: {error}"))
}

#[cfg(not(target_os = "windows"))]
fn configure_system(enabled: Option<bool>) -> Result<bool, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = match Platform::CURRENT {
        Platform::Mac => dirs::home_dir()
            .ok_or("Cannot locate the home directory")?
            .join("Library/LaunchAgents"),
        _ => dirs::config_dir()
            .ok_or("Cannot locate the configuration directory")?
            .join("autostart"),
    };
    let registration = Registration::new(Platform::CURRENT, &executable, &directory)?;
    if let Some(enabled) = enabled {
        registration.set_enabled(enabled)?;
    }
    registration.is_enabled()
}

#[cfg(any(not(target_os = "windows"), test))]
struct Registration {
    path: std::path::PathBuf,
    source: String,
}

#[cfg(any(not(target_os = "windows"), test))]
impl Registration {
    fn new(platform: Platform, executable: &Path, directory: &Path) -> Result<Self, String> {
        let path = executable
            .to_str()
            .ok_or("The executable path is not UTF-8")?;
        if !path.starts_with('/') || path.chars().any(char::is_control) {
            return Err(
                "An absolute executable path without control characters is required".into(),
            );
        }
        let (name, source) = match platform {
            Platform::Mac => {
                let bundle = executable
                    .parent()
                    .filter(|parent| parent.file_name().is_some_and(|name| name == "MacOS"))
                    .and_then(Path::parent)
                    .filter(|parent| parent.file_name().is_some_and(|name| name == "Contents"))
                    .and_then(Path::parent)
                    .filter(|parent| {
                        parent
                            .extension()
                            .is_some_and(|extension| extension == "app")
                    });
                // Launch the bundle through Launch Services so Dock identity and activation match a manual launch.
                let arguments = if let Some(bundle) = bundle {
                    format!(
                        "<string>/usr/bin/open</string><string>{}</string>",
                        xml(&bundle.to_string_lossy())
                    )
                } else {
                    format!("<string>{}</string>", xml(path))
                };
                (
                    "dev.tervia.prototype.plist",
                    format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\"><dict>\n\
                     <key>Label</key><string>dev.tervia.prototype</string>\n\
                     <key>ProgramArguments</key><array>{arguments}</array>\n\
                     <key>RunAtLoad</key><true/>\n\
                     </dict></plist>\n"
                    ),
                )
            }
            Platform::Linux => {
                // Desktop Entry values and Exec arguments each have their own escaping pass.
                let command = path
                    .chars()
                    .fold(String::new(), |mut command, character| {
                        match character {
                            '%' => command.push_str("%%"),
                            '\\' | '\"' | '`' | '$' => {
                                command.push('\\');
                                command.push(character);
                            }
                            _ => command.push(character),
                        }
                        command
                    })
                    .replace('\\', "\\\\");
                (
                    "dev.tervia.prototype.desktop",
                    format!(
                        "[Desktop Entry]\nType=Application\nName=Tervia\nExec=\"{command}\"\nTerminal=false\nStartupNotify=false\n"
                    ),
                )
            }
            Platform::Windows => return Err("Windows uses a registry registration".into()),
        };
        Ok(Self {
            path: directory.join(name),
            source,
        })
    }

    fn is_enabled(&self) -> Result<bool, String> {
        match fs::read_to_string(&self.path) {
            Ok(source) => Ok(!source.lines().any(|line| {
                let line = line.trim();
                line.split_once('=').is_some_and(|(key, value)| {
                    matches!(key.trim(), "Hidden") && value.trim() == "true"
                        || key.trim() == "X-GNOME-Autostart-enabled" && value.trim() == "false"
                })
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        if !enabled {
            return match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            };
        }
        let directory = self.path.parent().ok_or("Missing autostart directory")?;
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let mut file =
            tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
        file.write_all(self.source.as_bytes())
            .map_err(|error| error.to_string())?;
        file.persist(&self.path)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(any(not(target_os = "windows"), test))]
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "windows")]
fn configure_system(enabled: Option<bool>) -> Result<bool, String> {
    windows::configure(enabled).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(any(target_os = "windows", test))]
fn windows_command(path: &str) -> String {
    format!("\"{path}\"")
}

#[cfg(any(target_os = "windows", test))]
fn windows_approved(value: &[u8]) -> bool {
    // Task Manager uses both 2/3 and 6/7; the low bit marks disabled entries.
    value.first().is_none_or(|state| state & 1 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use std::{fs, path::Path};

    #[test]
    fn windows_quotes_paths_and_respects_task_manager_disabled_states() {
        assert_eq!(
            windows_command(r"C:\Mock Apps\tervia.exe"),
            r#""C:\Mock Apps\tervia.exe""#
        );
        assert!(windows_approved(&[]));
        for state in [2, 6] {
            assert!(windows_approved(&[state, 0, 0, 0]));
        }
        for state in [3, 7] {
            assert!(!windows_approved(&[state, 0, 0, 0]));
        }
    }

    #[test]
    fn registration_round_trips_without_creating_files_until_enabled() {
        for platform in [Platform::Mac, Platform::Linux] {
            let root = tempfile::tempdir().unwrap();
            let directory = root.path().join("autostart");
            let registration = Registration::new(
                platform,
                Path::new("/mock apps/Tervia.app/Contents/MacOS/Tervia"),
                &directory,
            )
            .unwrap();
            assert!(!registration.is_enabled().unwrap());
            assert!(!directory.exists());
            registration.set_enabled(false).unwrap();
            registration.set_enabled(true).unwrap();
            assert!(registration.is_enabled().unwrap());
            registration.set_enabled(true).unwrap();
            registration.set_enabled(false).unwrap();
            assert!(!registration.is_enabled().unwrap());
        }
    }

    #[test]
    fn mac_login_launches_the_app_bundle_and_escapes_xml() {
        let registration = Registration::new(
            Platform::Mac,
            Path::new("/mock & apps/Tervia.app/Contents/MacOS/Tervia"),
            Path::new("/mock home/Library/LaunchAgents"),
        )
        .unwrap();
        assert!(
            registration
                .source
                .contains("<string>/usr/bin/open</string>")
        );
        assert!(
            registration
                .source
                .contains("<string>/mock &amp; apps/Tervia.app</string>")
        );
        assert!(registration.source.contains("<key>RunAtLoad</key><true/>"));
        assert!(!registration.source.contains("KeepAlive"));
        assert_eq!(
            registration.path.file_name().unwrap(),
            "dev.tervia.prototype.plist"
        );
    }

    #[test]
    fn mac_development_binary_is_launched_directly() {
        let registration = Registration::new(
            Platform::Mac,
            Path::new("/mock build/tervia"),
            Path::new("/mock autostart"),
        )
        .unwrap();
        assert!(
            registration
                .source
                .contains("<string>/mock build/tervia</string>")
        );
        assert!(!registration.source.contains("/usr/bin/open"));
    }

    #[test]
    fn linux_quotes_exec_paths_and_honors_external_disabling() {
        let root = tempfile::tempdir().unwrap();
        let registration = Registration::new(
            Platform::Linux,
            Path::new("/mock apps/100%/$special/\"tervia\""),
            root.path(),
        )
        .unwrap();
        assert!(
            registration
                .source
                .contains(r#"Exec="/mock apps/100%%/\\$special/\\"tervia\\"""#)
        );
        registration.set_enabled(true).unwrap();
        fs::write(&registration.path, "[Desktop Entry]\nHidden=true\n").unwrap();
        assert!(!registration.is_enabled().unwrap());
        registration.set_enabled(true).unwrap();
        assert!(registration.is_enabled().unwrap());
    }

    #[test]
    fn registration_reports_io_failures_and_preserves_other_entries() {
        let root = tempfile::tempdir().unwrap();
        let other = root.path().join("mock-other.desktop");
        fs::write(&other, "synthetic entry").unwrap();
        let registration =
            Registration::new(Platform::Linux, Path::new("/mock/tervia"), root.path()).unwrap();
        registration.set_enabled(true).unwrap();
        registration.set_enabled(false).unwrap();
        assert_eq!(fs::read_to_string(other).unwrap(), "synthetic entry");
        fs::create_dir(&registration.path).unwrap();
        assert!(registration.is_enabled().is_err());
        assert!(registration.set_enabled(false).is_err());
        assert!(registration.set_enabled(true).is_err());
    }

    #[test]
    fn invalid_executable_paths_are_rejected_before_registration() {
        for path in [
            "relative/tervia",
            "/mock/tervia\nInjected=true",
            "/mock/tervia\0",
        ] {
            assert!(
                Registration::new(
                    Platform::Linux,
                    Path::new(path),
                    Path::new("/mock autostart")
                )
                .is_err()
            );
        }
    }
}
