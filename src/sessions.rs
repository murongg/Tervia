//! Live PTYs belong to detached hosts; GUI handles only attach and detach.
mod client;
mod host;
mod journal;
#[cfg(target_os = "macos")]
mod macos;
mod runtime;

use crate::{pty::PtyEvent, terminal::shell::Tracker};
pub use client::Connection;
use portable_pty::PtySize;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// Live hosts survive app upgrades; changing this requires client support for the previous wire format.
const VERSION: u64 = 1;
const MAX_PACKET: u64 = 1024 * 1024;
type Opened = (
    Connection,
    async_channel::Receiver<PtyEvent>,
    Option<Tracker>,
);

#[derive(Clone)]
pub struct Target {
    root: PathBuf,
    id: String,
    executable: PathBuf,
    environment: Vec<(OsString, OsString)>,
    colors: Vec<[u8; 3]>,
    shell: Option<PathBuf>,
}

impl Target {
    pub fn new(root: PathBuf, id: String, executable: PathBuf) -> Self {
        Self {
            root,
            id,
            executable,
            environment: Vec::new(),
            colors: Vec::new(),
            shell: None,
        }
    }

    pub fn with_env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.push((name.into(), value.into()));
        self
    }

    pub fn with_colors(mut self, colors: Vec<[u8; 3]>) -> Self {
        self.colors = colors;
        self
    }

    pub fn with_shell(mut self, shell: impl Into<PathBuf>) -> Self {
        self.shell = Some(shell.into());
        self
    }

    pub fn shell(&self) -> PathBuf {
        self.shell
            .clone()
            .unwrap_or_else(|| crate::pty::default_shell().into())
    }

    pub fn directory(&self) -> PathBuf {
        self.root.join(&self.id)
    }

    pub fn has_exited(&self) -> bool {
        client::has_exited(&self.directory(), crate::process::has_exited)
    }

    pub fn identifier() -> Result<String, String> {
        random_id().map_err(|e| e.to_string())
    }

    pub fn open(&self, cwd: &Path, size: PtySize) -> Result<Opened, String> {
        self.check()?;
        runtime::private_directory(&self.root).map_err(|e| e.to_string())?;
        match fs::create_dir(self.directory()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return self.reconnect(),
            Err(e) => return Err(e.to_string()),
        }
        let launch = (|| -> io::Result<()> {
            runtime::private_directory(&self.directory())?;
            runtime::write_private(
                &self.directory().join("launch.json"),
                &json!({
                    "version": VERSION, "cwd": cwd, "cols": size.cols, "rows": size.rows,
                    "token": random_id()?, "colors": self.colors,
                    "shell": self.shell,
                }),
            )?;
            runtime::spawn(
                &self.root,
                &self.executable,
                &self.directory(),
                &self.environment,
            )
        })();
        if let Err(error) = launch {
            let _ = fs::remove_dir_all(self.directory());
            return Err(error.to_string());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.directory().join("endpoint.json").exists() {
            if let Ok(message) = fs::read_to_string(self.directory().join("error")) {
                return Err(message);
            }
            if Instant::now() >= deadline {
                return Err("Terminal host did not become ready".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.reconnect()
    }

    pub fn reconnect(&self) -> Result<Opened, String> {
        self.check()?;
        client::connect(&self.directory(),&self.colors).map_err(|error|
            format!("Cannot reconnect to the saved terminal: {error}. Open a new terminal to start a new shell."))
    }

    pub fn terminate(&self) -> Result<(), String> {
        self.check()?;
        if !self.directory().exists() {
            return Ok(());
        }
        client::terminate(&self.directory()).map_err(|e| e.to_string())
    }

    fn check(&self) -> Result<(), String> {
        if !crate::state::Session::valid_connection(&self.id) {
            return Err("Invalid terminal identity".into());
        }
        Ok(())
    }
}

pub fn is_host_argument(argument: &std::ffi::OsStr) -> bool {
    matches!(
        argument.to_str(),
        Some("--terminal-host" | "--terminal-launcher" | "--terminal-detached-host")
    )
}

pub fn run_from_args() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().ok_or("Missing terminal host mode")?;
    let path = args
        .next()
        .map(PathBuf::from)
        .ok_or("Missing terminal directory")?;
    if !path.is_absolute() {
        return Err("Invalid terminal host arguments".into());
    }
    let result = (|| -> io::Result<()> {
        #[cfg(target_os = "macos")]
        if mode == "--terminal-detached-host" {
            let parent = args
                .next()
                .and_then(|value| value.to_str()?.parse().ok())
                .ok_or_else(|| io::Error::other("Missing terminal launcher PID"))?;
            if args.next().is_some() {
                return Err(io::Error::other("Unexpected terminal host arguments"));
            }
            macos::wait_for_launcher(parent)?;
            return host::run(&path);
        }
        if args.next().is_some() {
            return Err(io::Error::other("Unexpected terminal host arguments"));
        }
        #[cfg(target_os = "macos")]
        if mode == "--terminal-launcher" {
            return macos::launch(&path);
        }
        if mode != "--terminal-host" {
            return Err(io::Error::other("Invalid terminal host mode"));
        }
        host::run(&path)
    })();
    if let Err(error) = result {
        let _ = fs::write(path.join("error"), error.to_string());
        return Err(error.to_string());
    }
    Ok(())
}

fn random_id() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn field<'a>(value: &'a Value, name: &str) -> io::Result<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::other(format!("Missing {name}")))
}
