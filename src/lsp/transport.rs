use super::{Server, process, tools::Service};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_MESSAGE: usize = 16 * 1024 * 1024;

pub(super) struct Connection {
    child: Child,
    outgoing: mpsc::SyncSender<Value>,
    incoming: mpsc::Receiver<Result<Value, String>>,
    deadline: Instant,
    stop: Arc<AtomicBool>,
}
impl Connection {
    pub(super) fn start(
        server: &Server,
        root: &Path,
        deadline: Instant,
        stop: Arc<AtomicBool>,
        paths: &[std::path::PathBuf],
        service: Option<Service>,
    ) -> Result<Self, String> {
        #[cfg(windows)]
        let program = windows_program(
            &server.program,
            &std::env::var_os("PATH")
                .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
                .unwrap_or_default(),
        );
        #[cfg(not(windows))]
        let program = &server.program;
        let mut command = Command::new(program);
        command
            .args(&server.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        process::configure(&mut command);
        if service == Some(Service::TypeScript) {
            command.env(
                "NODE_OPTIONS",
                process::node_options(std::env::var_os("NODE_OPTIONS")),
            );
        }
        if !paths.is_empty() {
            process::with_paths(&mut command, paths)?;
        }
        let mut child = command.spawn().map_err(|error| {
            format!(
                "Cannot start {}: {error}. Install it and make it available on PATH.",
                server.program
            )
        })?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (outgoing, writes) = mpsc::sync_channel::<Value>(8);
        let (reads, incoming) = mpsc::sync_channel(64);
        let errors = reads.clone();
        let connection = Self {
            child,
            outgoing,
            incoming,
            deadline,
            stop,
        };
        thread::Builder::new()
            .name("tervia-lsp-write".into())
            .spawn(move || {
                for value in writes {
                    let result = (|| {
                        let body = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
                        if body.len() > MAX_MESSAGE {
                            return Err("Language server message exceeds 16 MiB".to_owned());
                        }
                        write!(stdin, "Content-Length: {}\r\n\r\n", body.len())
                            .and_then(|_| stdin.write_all(&body))
                            .and_then(|_| stdin.flush())
                            .map_err(|error| error.to_string())
                    })();
                    if let Err(error) = result {
                        let _ = errors.send(Err(error));
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        thread::Builder::new()
            .name("tervia-lsp-read".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let message = read(&mut reader).map_err(|error| error.to_string());
                    let failed = message.is_err();
                    if reads.send(message).is_err() || failed {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(connection)
    }

    pub(super) fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
    }

    pub(super) fn send(&self, mut message: Value) -> Result<(), String> {
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return Err("Language server stopped".into());
            }
            if Instant::now() >= self.deadline {
                return Err("Language server request timed out".into());
            }
            match self.outgoing.try_send(message) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err("Language server disconnected".into());
                }
                Err(mpsc::TrySendError::Full(pending)) => {
                    // A burst of open buffers can outpace the pipe writer. Apply
                    // bounded backpressure without blocking the app's UI thread.
                    message = pending;
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }

    pub(super) fn receive(&self, deadline: Instant, stop: &AtomicBool) -> Result<Value, String> {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("Language server stopped".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Language server request timed out".into());
            }
            match self
                .incoming
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(message) => return message,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err("Language server disconnected".into()),
            }
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        // Servers may launch child processes (notably Node language services).
        // Terminate the owned group before reaping so close/retry leaves no worker.
        process::terminate(&mut self.child);
    }
}

#[cfg(any(windows, test))]
fn windows_program(program: &str, paths: &[std::path::PathBuf]) -> std::path::PathBuf {
    let path = Path::new(program);
    if path.components().count() == 1 && path.extension().is_none() {
        // npm installs both an extensionless Unix shim and a .cmd launcher.
        // Windows Command resolves .exe implicitly, but needs the .cmd suffix.
        for directory in paths {
            for extension in ["exe", "cmd", "bat", "com"] {
                let candidate = directory.join(format!("{program}.{extension}"));
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    path.into()
}

fn read(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut length = None;
    let mut total = 0;
    loop {
        let mut line = String::new();
        let bytes = reader
            .take(8193)
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        total += bytes;
        if bytes == 0 {
            return Err("Language server closed stdout".into());
        }
        if total > 8192 {
            return Err("Language server header exceeds 8 KiB".into());
        }
        if line == "\r\n" {
            break;
        }
        let (name, value) = line
            .trim_end()
            .split_once(':')
            .ok_or("Invalid language server header")?;
        if name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err("Duplicate Content-Length".into());
            }
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Invalid Content-Length")?,
            );
        }
    }
    let length = length
        .filter(|length| *length <= MAX_MESSAGE)
        .ok_or("Invalid or oversized language server message")?;
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_bounded_and_read_exactly_one_message() {
        let mut valid = std::io::Cursor::new(b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}Content-Length: 4\r\n\r\nnull");
        assert_eq!(read(&mut valid).unwrap(), serde_json::json!({}));
        assert_eq!(read(&mut valid).unwrap(), Value::Null);
        for invalid in [
            "Content-Length: 16777217\r\n\r\n",
            "Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "Content-Length: -1\r\n\r\n",
            "Content-Length: 4\r\n\r\n{}",
            "Content-Type: application/json\r\n\r\n{}",
        ] {
            assert!(read(&mut std::io::Cursor::new(invalid)).is_err());
        }
        let header = format!("X: {}\r\n\r\n", "x".repeat(8192));
        assert!(read(&mut std::io::Cursor::new(header)).is_err());
    }

    #[test]
    fn windows_npm_shims_are_resolved_without_requiring_exe_files() {
        let root = tempfile::tempdir().unwrap();
        let paths = vec![root.path().to_path_buf()];
        std::fs::write(root.path().join("synthetic-server"), "Unix npm shim").unwrap();
        std::fs::write(root.path().join("synthetic-server.cmd"), "Windows npm shim").unwrap();
        assert_eq!(
            windows_program("synthetic-server", &paths),
            root.path().join("synthetic-server.cmd")
        );
        std::fs::write(
            root.path().join("synthetic-server.exe"),
            "synthetic executable",
        )
        .unwrap();
        assert_eq!(
            windows_program("synthetic-server", &paths),
            root.path().join("synthetic-server.exe")
        );
        assert_eq!(
            windows_program("synthetic-missing-server", &paths),
            std::path::PathBuf::from("synthetic-missing-server")
        );
    }
}
