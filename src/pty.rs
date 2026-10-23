use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use crate::agent::Agent;
use async_channel::Receiver;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
mod integration;
mod task;
pub use integration::ShellIntegration;
pub use task::task_command_for;

pub enum PtyEvent {
    Data(Vec<u8>),
    Resized { columns: usize, rows: usize },
    ReplayComplete,
    Exited(String),
    Error(String),
}

type OwnedChild = Arc<Mutex<Option<Box<dyn Child + Send + Sync>>>>;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: OwnedChild,
    writer: Option<mpsc::Sender<Vec<u8>>>,
    identity: Arc<Mutex<Option<Agent>>>,
    _integration: Option<ShellIntegration>,
}

pub fn initial_directory(project: &Path, home: &Path) -> std::path::PathBuf {
    if project.is_dir() {
        project.to_path_buf()
    } else {
        home.to_path_buf()
    }
}

pub fn default_shell() -> std::ffi::OsString {
    crate::shell::Profile::Default.resolve()
}

pub fn shell_command(cwd: &Path) -> CommandBuilder {
    shell_command_for(&default_shell(), cwd)
}

pub fn ssh_command_for(cwd: &Path, host: &crate::ssh::Host) -> Result<CommandBuilder, String> {
    let mut command = CommandBuilder::new("ssh");
    command.args(host.arguments()?);
    #[cfg(windows)]
    let cwd = dunce::simplified(cwd);
    command.cwd(cwd);
    configure_terminal_environment(&mut command);
    Ok(command)
}

pub fn shell_command_for(shell: &std::ffi::OsStr, cwd: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(shell);
    command.args(crate::platform::Platform::shell_args(
        &shell.to_string_lossy(),
    ));
    // CMD rejects the verbatim prefix added by canonicalize. Only simplify paths
    // with an equivalent Win32 spelling, preserving long paths and special names.
    #[cfg(windows)]
    let cwd = dunce::simplified(cwd);
    command.cwd(cwd);
    configure_terminal_environment(&mut command);
    command
}

fn configure_terminal_environment(command: &mut CommandBuilder) {
    #[cfg(target_os = "macos")]
    if ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .all(|name| command.get_env(name).is_none_or(|value| value.is_empty()))
    {
        // Fish decodes PWD before startup files can establish a UTF-8 locale.
        command.env("LC_CTYPE", "en_US.UTF-8");
    }
    // A GUI launched by a text-only runner must not pass its output policy to
    // the interactive PTY. Shell startup can still opt into NO_COLOR explicitly.
    command.env_remove("NO_COLOR");
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "Tervia");
    command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
}

impl PtySession {
    pub fn spawn_shell(
        cwd: &Path,
        size: PtySize,
    ) -> Result<
        (
            Self,
            Receiver<PtyEvent>,
            Option<crate::terminal::shell::Tracker>,
        ),
        String,
    > {
        Self::spawn_shell_with(&default_shell(), cwd, size)
    }

    pub fn spawn_shell_with(
        shell: &std::ffi::OsStr,
        cwd: &Path,
        size: PtySize,
    ) -> Result<
        (
            Self,
            Receiver<PtyEvent>,
            Option<crate::terminal::shell::Tracker>,
        ),
        String,
    > {
        let mut command = shell_command_for(shell, cwd);
        let integration = ShellIntegration::configure(&mut command)?;
        let tracker = integration.as_ref().map(|integration| {
            crate::terminal::shell::Tracker::new(
                integration.token().into(),
                &shell.to_string_lossy(),
            )
        });
        let (mut session, receiver) = Self::spawn(command, size)?;
        session._integration = integration;
        Ok((session, receiver, tracker))
    }
    pub fn foreground_agent(&self) -> Option<Agent> {
        *self.identity.lock().unwrap()
    }

    pub fn process_id(&self) -> Option<u32> {
        self.child
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|child| child.process_id())
    }
    pub fn spawn(
        command: CommandBuilder,
        size: PtySize,
    ) -> Result<(Self, Receiver<PtyEvent>), String> {
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|error| error.to_string())?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| error.to_string())?;
        drop(pair.slave);
        let root = child.process_id().unwrap_or(0);
        let child = Arc::new(Mutex::new(Some(child)));
        let identity = Arc::new(Mutex::new(None));
        let identity_child = child.clone();
        let identity_value = identity.clone();
        #[cfg(unix)]
        let probe_fd = {
            use std::os::fd::FromRawFd;
            pair.master.as_raw_fd().and_then(|fd| {
                let fd = unsafe { libc::dup(fd) };
                (fd >= 0).then(|| unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
            })
        };
        thread::spawn(move || {
            // Process inspection can involve /proc or Win32 snapshots; keep it
            // outside the GPUI event loop so output and key input stay responsive.
            while root != 0 && identity_child.lock().unwrap().is_some() {
                #[cfg(unix)]
                let group = {
                    use std::os::fd::AsRawFd;
                    probe_fd.as_ref().and_then(|fd| {
                        let group = unsafe { libc::tcgetpgrp(fd.as_raw_fd()) };
                        (group > 1).then_some(group as u32)
                    })
                };
                #[cfg(not(unix))]
                let group = None;
                *identity_value.lock().unwrap() = crate::process::foreground(root, group);
                thread::sleep(Duration::from_millis(400));
            }
            *identity_value.lock().unwrap() = None;
        });
        let (sender, receiver) = async_channel::bounded(64);
        let (writer_sender, writer_receiver) = mpsc::channel::<Vec<u8>>();

        let read_sender = sender.clone();
        thread::spawn(move || {
            let mut bytes = [0u8; 8192];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => {
                        if read_sender
                            .send_blocking(PtyEvent::Data(bytes[..count].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    // Unix PTYs also signal a normal slave close with EIO.
                    Err(error) if error.raw_os_error() == Some(5) => break,
                    Err(error) => {
                        let _ = read_sender.send_blocking(PtyEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
        });

        let write_sender = sender.clone();
        thread::spawn(move || {
            while let Ok(bytes) = writer_receiver.recv() {
                if let Err(error) = writer.write_all(&bytes).and_then(|_| writer.flush()) {
                    let _ = write_sender.send_blocking(PtyEvent::Error(error.to_string()));
                    break;
                }
            }
        });

        let watched_child = child.clone();
        thread::spawn(move || {
            loop {
                let status = {
                    let mut child = watched_child.lock().unwrap();
                    let Some(process) = child.as_mut() else {
                        return;
                    };
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            child.take();
                            Some(Ok(status.to_string()))
                        }
                        Ok(None) => None,
                        Err(error) => {
                            child.take();
                            Some(Err(error.to_string()))
                        }
                    }
                };
                if let Some(status) = status {
                    let event = match status {
                        Ok(status) => PtyEvent::Exited(status),
                        Err(error) => PtyEvent::Error(error),
                    };
                    let _ = sender.send_blocking(event);
                    return;
                }
                thread::sleep(Duration::from_millis(30));
            }
        });

        Ok((
            Self {
                master: pair.master,
                child,
                writer: Some(writer_sender),
                identity,
                _integration: None,
            },
            receiver,
        ))
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .as_ref()
            .ok_or("Terminal session is closed")?
            .send(bytes.to_vec())
            .map_err(|error| error.to_string())
    }

    pub fn resize(
        &self,
        columns: usize,
        rows: usize,
        cell_width: u16,
        cell_height: u16,
    ) -> Result<(), String> {
        self.master
            .resize(PtySize {
                rows: rows.clamp(1, u16::MAX as usize) as u16,
                cols: columns.clamp(2, u16::MAX as usize) as u16,
                pixel_width: (columns as u32 * cell_width as u32).min(u16::MAX as u32) as u16,
                pixel_height: (rows as u32 * cell_height as u32).min(u16::MAX as u32) as u16,
            })
            .map_err(|error| error.to_string())
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.writer.take();
        // Taking the child under the same lock as try_wait prevents stale-PID signalling.
        if let Some(mut child) = self.child.lock().unwrap().take() {
            // Terminate our PTY's foreground job and session leader before the app can quit.
            // Background reaping alone is insufficient: app exit stops Rust cleanup threads.
            #[cfg(unix)]
            unsafe {
                let parent_group = libc::getpgrp();
                if let Some(group) = self
                    .master
                    .process_group_leader()
                    .filter(|group| *group > 1 && *group != parent_group)
                {
                    libc::kill(-group, libc::SIGKILL);
                }
                if let Some(pid) = child.process_id() {
                    if pid as i32 != parent_group {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            let _ = child.clone_killer().kill();
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_shells_default_to_utf8_when_locale_is_missing_or_empty() {
        for empty in [false, true] {
            let mut command = CommandBuilder::new("/synthetic-shell");
            command.env_clear();
            if empty {
                for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
                    command.env(name, "");
                }
            }
            command.env("LC_MESSAGES", "C");

            configure_terminal_environment(&mut command);

            assert_eq!(command.get_env("LC_CTYPE"), Some(OsStr::new("en_US.UTF-8")));
            for name in ["LC_ALL", "LANG"] {
                assert_eq!(command.get_env(name), empty.then(|| OsStr::new("")));
            }
            assert_eq!(command.get_env("LC_MESSAGES"), Some(OsStr::new("C")));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_shells_preserve_explicit_locale_settings() {
        for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
            for value in ["C", "en_US.UTF-8", "zh_CN.UTF-8"] {
                let mut command = CommandBuilder::new("/synthetic-shell");
                command.env_clear();
                command.env(name, value);

                configure_terminal_environment(&mut command);

                for other in ["LC_ALL", "LC_CTYPE", "LANG"] {
                    assert_eq!(
                        command.get_env(other),
                        (other == name).then(|| OsStr::new(value)),
                        "{name}={value}: {other} changed"
                    );
                }
            }
        }
    }

    #[test]
    fn interactive_terminal_does_not_inherit_the_launchers_no_color_policy() {
        let mut command = CommandBuilder::new("/synthetic-shell");
        command.env_clear();
        command.env("NO_COLOR", "1");
        command.env("TERM", "dumb");
        command.env("COLORTERM", "");
        command.env("SYNTHETIC_KEEP", "mock-value");

        configure_terminal_environment(&mut command);

        assert_eq!(command.get_env("NO_COLOR"), None);
        assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
        assert_eq!(command.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
        assert_eq!(command.get_env("TERM_PROGRAM"), Some(OsStr::new("Tervia")));
        assert_eq!(
            command.get_env("SYNTHETIC_KEEP"),
            Some(OsStr::new("mock-value"))
        );
    }
}
