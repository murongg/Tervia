mod catalog;
#[cfg(test)]
mod tests;

use super::{Server, process};
use crate::platform::Platform;
use base64::Engine as _;
use self_update::{ArchiveKind, Compression, Extract};
use sha2::{Digest, Sha256, Sha512};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Rust,
    TypeScript,
    Python,
    Go,
}
impl Service {
    pub fn from_server(server: &Server) -> Option<Self> {
        [Self::Rust, Self::TypeScript, Self::Python, Self::Go]
            .into_iter()
            .find(|service| Server::for_language(service.language()).as_ref() == Some(server))
    }
    fn language(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Go => "go",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::TypeScript => "JavaScript / TypeScript",
            Self::Python => "Python",
            Self::Go => "Go",
        }
    }
    fn version(self) -> &'static str {
        match self {
            Self::Rust => "2026-10-19",
            Self::TypeScript => "6.0.2-ts6.0.3",
            Self::Python => "1.1.414",
            Self::Go => "0.24.0",
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Rust => "rust-analyzer",
            Self::TypeScript => "typescript",
            Self::Python => "pyright",
            Self::Go => "gopls",
        }
    }
    fn files(self, target: Target) -> Vec<&'static str> {
        match self {
            Self::Rust => vec![if target.platform == Platform::Windows {
                "rust-analyzer.exe"
            } else {
                "rust-analyzer"
            }],
            Self::TypeScript => vec![
                "node_modules/typescript-language-server/lib/cli.mjs",
                "node_modules/typescript/lib/tsserver.js",
            ],
            Self::Python => vec!["node_modules/pyright/langserver.index.js"],
            Self::Go => vec![if target.platform == Platform::Windows {
                "bin/gopls.exe"
            } else {
                "bin/gopls"
            }],
        }
    }
    fn runtime(self) -> Option<Runtime> {
        match self {
            Self::Rust => None,
            Self::TypeScript => Some(Runtime {
                name: "Node.js",
                program: "node",
                minimum: "22.22.2",
                url: "https://nodejs.org/en/download",
            }),
            Self::Python => Some(Runtime {
                name: "Node.js",
                program: "node",
                minimum: "14.0.0",
                url: "https://nodejs.org/en/download",
            }),
            Self::Go => Some(Runtime {
                name: "Go",
                program: "go",
                minimum: "1.27.0",
                url: "https://go.dev/dl/",
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub name: &'static str,
    program: &'static str,
    pub minimum: &'static str,
    pub url: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Checking,
    Waiting,
    Downloading { received: u64, total: Option<u64> },
    Installing,
    Runtime(Runtime),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub service: Service,
    pub stage: Stage,
}
pub type Progress = Arc<Mutex<Option<Status>>>;

#[derive(Clone, Debug)]
pub(super) struct Launch {
    pub server: Server,
    pub paths: Vec<PathBuf>,
    pub provision: Provision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Provision {
    Custom,
    System,
    Managed,
}
#[derive(Clone, Copy, Debug)]
pub struct Target {
    platform: Platform,
    arch: &'static str,
}
impl Target {
    pub fn new(platform: Platform, arch: &str) -> Result<Self, String> {
        let arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return Err(format!("No language service distribution for {arch}")),
        };
        Ok(Self { platform, arch })
    }
    pub fn current() -> Result<Self, String> {
        Self::new(Platform::CURRENT, std::env::consts::ARCH)
    }
    fn key(self) -> String {
        format!(
            "{}-{}",
            match self.platform {
                Platform::Mac => "macos",
                Platform::Windows => "windows",
                Platform::Linux => "linux",
            },
            self.arch
        )
    }
}

enum Hash {
    Sha256(String),
    Sha512(String),
}
enum Layout {
    Binary { kind: ArchiveKind, file: String },
    Npm { directory: &'static str },
}
struct Package {
    url: String,
    digest: Hash,
    layout: Layout,
}
trait Source: Send + Sync {
    fn packages(&self, service: Service, target: Target) -> Result<Vec<Package>, String>;
    fn fetch(&self, package: &Package, path: &Path, context: &Context<'_>) -> Result<(), String>;
}
struct Official;
impl Source for Official {
    fn packages(&self, service: Service, target: Target) -> Result<Vec<Package>, String> {
        catalog::packages(service, target)
    }
    fn fetch(&self, package: &Package, path: &Path, context: &Context<'_>) -> Result<(), String> {
        let file = fs::File::create(path).map_err(|error| error.to_string())?;
        let progress = context.progress.clone();
        let service = context.service;
        self_update::Download::from_url(&package.url)
            .timeout(Duration::from_secs(120))
            .max_download_size(128 * 1024 * 1024)
            .progress_callback(move |received, total| {
                if let Ok(mut slot) = progress.lock() {
                    *slot = Some(Status {
                        service,
                        stage: Stage::Downloading { received, total },
                    });
                }
            })
            .download_to(Checked {
                writer: file,
                context,
            })
            .map_err(|error| error.to_string())
    }
}

struct Context<'a> {
    service: Service,
    stop: &'a AtomicBool,
    deadline: Instant,
    progress: &'a Progress,
    paths: Vec<PathBuf>,
}
impl Context<'_> {
    fn check(&self) -> Result<(), String> {
        if self.stop.load(Ordering::Relaxed) {
            return Err("Language service preparation canceled".into());
        }
        if Instant::now() >= self.deadline {
            return Err("Language service preparation timed out".into());
        }
        Ok(())
    }
    fn report(&self, stage: Stage) {
        if let Ok(mut slot) = self.progress.lock() {
            *slot = Some(Status {
                service: self.service,
                stage,
            });
        }
    }
}
struct Checked<'a, 'b> {
    writer: fs::File,
    context: &'a Context<'b>,
}
impl Write for Checked<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.context.check().map_err(io::Error::other)?;
        self.writer.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub struct Tools {
    root: Option<PathBuf>,
    paths: Option<Vec<PathBuf>>,
    target: Target,
    source: Arc<dyn Source>,
}
impl Default for Tools {
    fn default() -> Self {
        let root = dirs::data_local_dir().map(|path| path.join("tervia/language-services"));
        Self {
            root,
            paths: None,
            target: Target {
                platform: Platform::CURRENT,
                arch: std::env::consts::ARCH,
            },
            source: Arc::new(Official),
        }
    }
}
impl Tools {
    pub fn with_paths(root: PathBuf, paths: Vec<PathBuf>) -> Self {
        Self {
            root: Some(root),
            paths: Some(paths),
            ..Self::default()
        }
    }
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            ..Self::default()
        }
    }
    pub(super) fn prepare(
        &self,
        server: &Server,
        stop: &AtomicBool,
        deadline: Instant,
        progress: &Progress,
    ) -> Result<Launch, String> {
        self.prepare_with(server, stop, deadline, progress, Provision::System)
    }
    pub(super) fn prepare_managed(
        &self,
        server: &Server,
        stop: &AtomicBool,
        deadline: Instant,
        progress: &Progress,
    ) -> Result<Launch, String> {
        self.prepare_with(server, stop, deadline, progress, Provision::Managed)
    }
    fn prepare_with(
        &self,
        server: &Server,
        stop: &AtomicBool,
        deadline: Instant,
        progress: &Progress,
        preference: Provision,
    ) -> Result<Launch, String> {
        let Some(service) = Service::from_server(server) else {
            return Ok(Launch {
                server: server.clone(),
                paths: vec![],
                provision: Provision::Custom,
            });
        };
        let context = Context {
            service,
            stop,
            deadline,
            progress,
            paths: self.paths.clone().unwrap_or_else(search_paths),
        };
        context.check()?;
        context.report(Stage::Checking);
        let result = self.prepare_service(server, &context, preference);
        if let Ok(mut slot) = progress.lock()
            && (result.is_ok()
                || !matches!(
                    slot.as_ref().map(|status| &status.stage),
                    Some(Stage::Runtime(_))
                ))
        {
            *slot = None;
        }
        result
    }
    fn prepare_service(
        &self,
        server: &Server,
        context: &Context<'_>,
        preference: Provision,
    ) -> Result<Launch, String> {
        if preference == Provision::System
            && let Some(program) = process::find(&server.program, &context.paths)
        {
            let mut command = Command::new(&program);
            let probe = match context.service {
                Service::Rust | Service::TypeScript => Some("--version"),
                Service::Go => Some("version"),
                Service::Python => None,
            };
            if let Some(probe) = probe {
                command.arg(probe);
            }
            process::with_paths(&mut command, &context.paths)?;
            if probe.is_none()
                || process::run(
                    command,
                    context.stop,
                    context
                        .deadline
                        .min(Instant::now() + Duration::from_secs(3)),
                )
                .is_ok()
            {
                return Ok(Launch {
                    server: Server {
                        program: program.to_string_lossy().into(),
                        args: server.args.clone(),
                    },
                    paths: context.paths.clone(),
                    provision: Provision::System,
                });
            }
        }
        context.check()?;
        let runtime = self.runtime(context)?;
        Target::new(self.target.platform, self.target.arch)?;
        let destination = self.installation(context.service)?;
        if !self.ready(context.service, &destination) {
            let parent = destination
                .parent()
                .ok_or("Invalid language service directory")?;
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(parent.join("install.lock"))
                .map_err(|error| error.to_string())?;
            loop {
                context.check()?;
                match lock.try_lock() {
                    Ok(()) => break,
                    Err(fs::TryLockError::WouldBlock) => {
                        context.report(Stage::Waiting);
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(fs::TryLockError::Error(error)) => return Err(error.to_string()),
                }
            }
            if !self.ready(context.service, &destination) {
                let staging = tempfile::tempdir_in(parent).map_err(|error| error.to_string())?;
                let install = staging.path().join("install");
                fs::create_dir(&install).map_err(|error| error.to_string())?;
                if context.service == Service::Go {
                    self.install_go(
                        &install,
                        runtime.as_ref().ok_or("Go is unavailable")?,
                        context,
                    )?;
                } else {
                    for (index, package) in self
                        .source
                        .packages(context.service, self.target)?
                        .into_iter()
                        .enumerate()
                    {
                        context.check()?;
                        let archive = staging.path().join(format!("archive-{index}"));
                        context.report(Stage::Downloading {
                            received: 0,
                            total: None,
                        });
                        self.source.fetch(&package, &archive, context)?;
                        verify(&archive, &package.digest, context)?;
                        context.report(Stage::Installing);
                        match package.layout {
                            Layout::Binary { kind, file } => {
                                Extract::from_source(&archive)
                                    .archive(kind)
                                    .extract_file(&install, &file)
                                    .map_err(|error| error.to_string())?;
                                executable(&install.join(file))?;
                            }
                            Layout::Npm { directory } => {
                                let unpacked = staging.path().join(format!("unpacked-{index}"));
                                Extract::from_source(&archive)
                                    .archive(ArchiveKind::Tar(Some(Compression::Gz)))
                                    .extract_into(&unpacked)
                                    .map_err(|error| error.to_string())?;
                                let target = install.join(directory);
                                fs::create_dir_all(
                                    target.parent().ok_or("Invalid package directory")?,
                                )
                                .map_err(|error| error.to_string())?;
                                fs::rename(unpacked.join("package"), target)
                                    .map_err(|error| error.to_string())?;
                            }
                        }
                    }
                }
                context.check()?;
                for path in context.service.files(self.target) {
                    if !install.join(path).is_file() {
                        return Err(format!("Language service package is missing {path}"));
                    }
                }
                fs::write(install.join("ready"), context.service.version())
                    .map_err(|error| error.to_string())?;
                if destination.exists() {
                    fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
                }
                // A partial download/build is never visible as an installed service.
                fs::rename(install, &destination).map_err(|error| error.to_string())?;
            }
        }
        let files = context.service.files(self.target);
        let mut paths = context.paths.clone();
        let (program, args) = match runtime {
            Some(runtime) if context.service != Service::Go => (
                runtime,
                vec![
                    destination.join(files[0]).to_string_lossy().into(),
                    "--stdio".into(),
                ],
            ),
            runtime => {
                if let Some(runtime) = runtime
                    && let Some(parent) = runtime.parent()
                {
                    paths.insert(0, parent.into());
                }
                (destination.join(files[0]), Vec::new())
            }
        };
        Ok(Launch {
            server: Server {
                program: program.to_string_lossy().into(),
                args,
            },
            paths,
            provision: Provision::Managed,
        })
    }
    fn installation(&self, service: Service) -> Result<PathBuf, String> {
        Ok(self
            .root
            .as_ref()
            .ok_or("Cannot find the user data directory")?
            .join(service.id())
            .join(format!("{}-{}", service.version(), self.target.key())))
    }
    fn ready(&self, service: Service, path: &Path) -> bool {
        fs::read_to_string(path.join("ready")).is_ok_and(|version| version == service.version())
            && service
                .files(self.target)
                .iter()
                .all(|file| path.join(file).is_file())
    }
    fn runtime(&self, context: &Context<'_>) -> Result<Option<PathBuf>, String> {
        let Some(runtime) = context.service.runtime() else {
            return Ok(None);
        };
        let mut checked = std::collections::HashSet::new();
        for directory in &context.paths {
            if let Some(program) = process::find(runtime.program, std::slice::from_ref(directory)) {
                if !checked.insert(program.clone()) {
                    continue;
                }
                let mut command = Command::new(&program);
                command.arg(if runtime.program == "go" {
                    "version"
                } else {
                    "--version"
                });
                if let Ok(version) = process::run(
                    command,
                    context.stop,
                    context
                        .deadline
                        .min(Instant::now() + Duration::from_secs(3)),
                ) {
                    let version = if runtime.program == "go" {
                        version
                            .split_whitespace()
                            .find_map(|word| {
                                word.strip_prefix("go")
                                    .filter(|version| !version.is_empty())
                            })
                            .unwrap_or("")
                    } else {
                        version.trim_start_matches('v')
                    };
                    if semver::Version::parse(version).is_ok_and(|version| {
                        version >= semver::Version::parse(runtime.minimum).unwrap()
                    }) {
                        return Ok(Some(program));
                    }
                }
            }
        }
        context.report(Stage::Runtime(runtime));
        Err(format!(
            "{} {} or later is required. Install it from {} and retry.",
            runtime.name, runtime.minimum, runtime.url
        ))
    }
    fn install_go(&self, into: &Path, go: &Path, context: &Context<'_>) -> Result<(), String> {
        context.report(Stage::Installing);
        // Keep compiler/module caches in the staging directory: only the final
        // language service remains after a successful install.
        let cache = into
            .parent()
            .ok_or("Invalid language service staging directory")?
            .join("go-build");
        let mut command = Command::new(go);
        command
            .args(["install", "golang.org/x/tools/gopls@v0.24.0"])
            .current_dir(into)
            .env("GOBIN", into.join("bin"))
            .env("GOPATH", cache.join("path"))
            .env("GOMODCACHE", cache.join("mod"))
            .env("GOCACHE", cache.join("cache"))
            .env("GOWORK", "off")
            .env("CGO_ENABLED", "0")
            .env("GOFLAGS", "")
            .env("GOTELEMETRY", "off")
            .env(
                "GOOS",
                match self.target.platform {
                    Platform::Mac => "darwin",
                    Platform::Windows => "windows",
                    Platform::Linux => "linux",
                },
            )
            .env(
                "GOARCH",
                if self.target.arch == "aarch64" {
                    "arm64"
                } else {
                    "amd64"
                },
            );
        process::with_paths(&mut command, &context.paths)?;
        process::run(command, context.stop, context.deadline).map(|_| ())
    }
}

fn verify(path: &Path, expected: &Hash, context: &Context<'_>) -> Result<(), String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        context.check()?;
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        match expected {
            Hash::Sha256(_) => sha256.update(&buffer[..count]),
            Hash::Sha512(_) => sha512.update(&buffer[..count]),
        }
    }
    let matches = match expected {
        Hash::Sha256(expected) => {
            expected.len() == 64
                && format!("{:x}", sha256.finalize()).eq_ignore_ascii_case(expected)
        }
        Hash::Sha512(expected) => base64::engine::general_purpose::STANDARD
            .decode(expected)
            .is_ok_and(|expected| expected.as_slice() == &sha512.finalize()[..]),
    };
    if matches {
        Ok(())
    } else {
        Err("Language service download checksum mismatch".into())
    }
}
fn executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let _ = path;
    Ok(())
}
fn search_paths() -> Vec<PathBuf> {
    let mut paths = std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .filter(|path| path.is_absolute())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        for name in [
            ".cargo/bin",
            ".local/bin",
            ".vite-plus/bin",
            ".volta/bin",
            "go/bin",
        ] {
            paths.push(home.join(name));
        }
        if let Ok(entries) = fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions = entries
                .flatten()
                .map(|entry| entry.path().join("bin"))
                .filter(|path| path.is_dir())
                .collect::<Vec<_>>();
            versions.sort();
            versions.reverse();
            paths.extend(versions);
        }
    }
    #[cfg(unix)]
    {
        for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            paths.push(directory.into());
        }
    }
    #[cfg(windows)]
    {
        if let Some(directory) = std::env::var_os("APPDATA") {
            paths.push(PathBuf::from(directory).join("npm"));
        }
        if let Some(directory) = std::env::var_os("ProgramFiles") {
            let directory = PathBuf::from(directory);
            paths.push(directory.join("nodejs"));
            paths.push(directory.join("Go/bin"));
        }
    }
    paths.dedup();
    paths
}
