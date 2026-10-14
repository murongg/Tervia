use std::{
    ffi::OsString,
    fmt, io,
    path::{Path, PathBuf},
    process::ExitStatus,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    MissingExecutable,
    Start,
    Io,
    Timeout,
    Command,
    Message,
}

#[derive(Clone)]
pub struct Error {
    kind: ErrorKind,
    process: Option<Box<Process>>,
    message: String,
    guidance: Option<String>,
}

#[derive(Clone)]
struct Process {
    directory: PathBuf,
    arguments: Vec<OsString>,
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Explicit operation guidance intended to remain visible without opening diagnostics.
    pub fn guidance(&self) -> Option<&str> {
        self.guidance.as_deref()
    }

    pub fn directory(&self) -> Option<&Path> {
        self.process
            .as_ref()
            .map(|process| process.directory.as_path())
    }

    pub fn arguments(&self) -> &[OsString] {
        self.process
            .as_ref()
            .map_or(&[], |process| process.arguments.as_slice())
    }

    pub fn status(&self) -> Option<ExitStatus> {
        self.process.as_ref().and_then(|process| process.status)
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.status().and_then(|status| status.code())
    }

    pub fn stdout(&self) -> &[u8] {
        self.process
            .as_ref()
            .map_or(&[], |process| process.stdout.as_slice())
    }

    pub fn stderr(&self) -> &[u8] {
        self.process
            .as_ref()
            .map_or(&[], |process| process.stderr.as_slice())
    }

    pub fn details(&self) -> String {
        self.to_string()
    }

    pub(super) fn with_guidance(mut self, guidance: &str) -> Self {
        self.guidance = Some(guidance.to_owned());
        self
    }

    pub(super) fn context(mut self, context: &str) -> Self {
        self.message = if self.message.is_empty() {
            context.to_owned()
        } else {
            format!("{context}: {}", self.message)
        };
        self
    }

    pub(super) fn with_process(
        mut self,
        root: &Path,
        args: &[OsString],
        status: Option<ExitStatus>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> Self {
        self.process = Some(Box::new(Process {
            directory: root.to_path_buf(),
            arguments: args.to_vec(),
            status,
            stdout,
            stderr,
        }));
        self
    }

    pub(super) fn start(root: &Path, args: &[OsString], error: io::Error) -> Self {
        let kind = if error.kind() == io::ErrorKind::NotFound {
            ErrorKind::MissingExecutable
        } else {
            ErrorKind::Start
        };
        Self::process(
            kind,
            root,
            args,
            None,
            Vec::new(),
            Vec::new(),
            format!("Cannot start Git: {error}"),
        )
    }

    pub(super) fn process(
        kind: ErrorKind,
        root: &Path,
        args: &[OsString],
        status: Option<ExitStatus>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        message: String,
    ) -> Self {
        Self {
            kind,
            process: None,
            message,
            guidance: None,
        }
        .with_process(root, args, status, stdout, stderr)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            kind: ErrorKind::Message,
            process: None,
            message,
            guidance: None,
        }
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            ..Self::from(error.to_string())
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(guidance) = self.guidance() {
            formatter.write_str(guidance)?;
            if self.process.is_some() || !self.message.is_empty() {
                formatter.write_str(": ")?;
            }
        }
        let Some(directory) = self.directory() else {
            return formatter.write_str(&self.message);
        };
        // Arguments can contain credentials, commit bodies, or pathspecs. Keep
        // them available to callers, but never render their values in diagnostics.
        let subcommand = self
            .arguments()
            .first()
            .map(|arg| arg.to_string_lossy())
            .unwrap_or_default();
        write!(formatter, "git {subcommand} in {}", directory.display())?;
        if let Some(code) = self.exit_code() {
            write!(formatter, " (exit code: {code})")?;
        } else if let Some(status) = self.status() {
            write!(formatter, " ({status})")?;
        }
        if !self.message.is_empty() {
            write!(formatter, ": {}", self.message)?;
        }
        for (label, bytes) in [("stderr", self.stderr()), ("stdout", self.stdout())] {
            if !bytes.is_empty() {
                write!(formatter, "\n{label}:\n{}", String::from_utf8_lossy(bytes))?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Error")
            .field("kind", &self.kind)
            .field("details", &self.details())
            .finish()
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_validation_messages_keep_their_guidance() {
        let message = "The diff changed. Refresh it before staging this hunk.";
        let error = Error::from(message);
        assert_eq!(error.kind(), ErrorKind::Message);
        assert_eq!(error.message(), message);
        assert_eq!(error.to_string(), message);
        assert_eq!(error.clone().details(), message);
    }

    #[test]
    fn explicit_guidance_is_separate_from_diagnostic_context() {
        let diagnostic = Error::from(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "synthetic IO cause",
        ))
        .context("Cannot inspect synthetic metadata");
        assert_eq!(diagnostic.guidance(), None);
        let error = diagnostic
            .clone()
            .with_guidance("Synthetic operation already completed");
        assert_eq!(error.kind(), ErrorKind::Io);
        assert_eq!(error.message(), diagnostic.message());
        assert_eq!(
            error.guidance(),
            Some("Synthetic operation already completed")
        );
        assert_eq!(error.clone().guidance(), error.guidance());
        assert_eq!(
            error.details(),
            "Synthetic operation already completed: Cannot inspect synthetic metadata: synthetic IO cause"
        );
    }

    #[test]
    fn explicit_guidance_preserves_process_metadata_and_captured_output() {
        let original = Error::process(
            ErrorKind::Command,
            Path::new("synthetic-repository"),
            &["stash".into(), "synthetic-argument".into()],
            None,
            b"synthetic stdout".to_vec(),
            b"synthetic stderr".to_vec(),
            "synthetic cause".into(),
        );
        let error = original
            .clone()
            .with_guidance("Synthetic apply already completed");
        assert_eq!(error.kind(), original.kind());
        assert_eq!(error.message(), original.message());
        assert_eq!(error.directory(), original.directory());
        assert_eq!(error.arguments(), original.arguments());
        assert_eq!(error.status(), original.status());
        assert_eq!(error.stdout(), original.stdout());
        assert_eq!(error.stderr(), original.stderr());
        let details = error.details();
        assert!(details.contains("Synthetic apply already completed"));
        assert!(details.contains(&original.details()));
    }

    #[test]
    fn diagnostics_do_not_render_argument_values() {
        let args = vec![
            "commit".into(),
            "-m".into(),
            "synthetic private body".into(),
            "https://mock:secret@example.test/repo".into(),
        ];
        let error = Error::start(
            Path::new("mock-project"),
            &args,
            io::ErrorKind::PermissionDenied.into(),
        );
        assert_eq!(error.arguments(), args);
        for detail in [error.to_string(), format!("{error:?}")] {
            assert!(detail.contains("git commit"));
            assert!(!detail.contains("private body"));
            assert!(!detail.contains("secret"));
        }
    }
}
