use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use self_update::{Release, ReleaseSource};

use crate::platform::Platform;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_URL: &str = "https://github.com/murongg/Tervia/releases";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    platform: Platform,
    suffix: &'static str,
}

impl Target {
    pub fn new(platform: Platform, arch: &str) -> Result<Self, String> {
        let suffix = match (platform, arch) {
            (Platform::Mac, "aarch64") => "macos-arm64.zip",
            (Platform::Mac, "x86_64") => "macos-x86_64.zip",
            (Platform::Windows, "x86_64") => "windows-x64.zip",
            (Platform::Linux, "x86_64") => "linux-x86_64.tar.gz",
            (Platform::Linux, "aarch64") => "linux-aarch64.tar.gz",
            _ => return Err(format!("No update package for {platform:?}/{arch}")),
        };
        Ok(Self { platform, suffix })
    }

    pub fn current() -> Result<Self, String> {
        Self::new(Platform::CURRENT, std::env::consts::ARCH)
    }

    pub fn asset_name(self, version: &str) -> String {
        format!("Tervia-v{version}-{}", self.suffix)
    }
}

#[derive(Clone, Debug)]
pub struct Candidate {
    release: Release,
    target: Target,
    digest: String,
}

impl Candidate {
    pub fn from_release(
        release: Release,
        current: &str,
        target: Target,
    ) -> Result<Option<Self>, String> {
        let remote =
            semver::Version::parse(release.version()).map_err(|error| error.to_string())?;
        let current = semver::Version::parse(current).map_err(|error| error.to_string())?;
        if !remote.pre.is_empty() || !remote.cmp_precedence(&current).is_gt() {
            return Ok(None);
        }
        let name = target.asset_name(release.version());
        let asset = release
            .assets()
            .iter()
            .find(|asset| asset.name() == name)
            .ok_or_else(|| format!("The release has no package named {name}"))?;
        let digest = asset
            .digest()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .filter(|digest| {
                digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .ok_or_else(|| format!("The release has no valid SHA-256 digest for {name}"))?
            .to_owned();
        Ok(Some(Self {
            release,
            target,
            digest,
        }))
    }

    pub fn version(&self) -> &str {
        self.release.version()
    }

    pub fn notes_url(&self) -> String {
        format!("{RELEASES_URL}/tag/v{}", self.version())
    }
}

pub fn check() -> Result<Option<Candidate>, String> {
    let updater = self_update::backends::github::Update::configure()
        .repo_owner("murongg")
        .repo_name("Tervia")
        .bin_name("tervia")
        .current_version(VERSION)
        .timeout(Duration::from_secs(30))
        .show_output(false)
        .no_confirm(true)
        .build()
        .map_err(|error| error.to_string())?;
    // The dedicated latest endpoint excludes draft and prerelease builds.
    let releases = match updater.get_latest_release() {
        Ok(releases) => releases,
        Err(error) if error.http_status() == Some(404) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let Some(release) = releases.latest() else {
        return Ok(None);
    };
    Candidate::from_release(release.clone(), VERSION, Target::current()?)
}

#[derive(Clone, Debug)]
pub struct Installation {
    executable: PathBuf,
    bundle: Option<PathBuf>,
    target: Target,
}

impl Installation {
    pub fn new(executable: PathBuf, target: Target) -> Result<Self, String> {
        if !executable.is_absolute() {
            return Err("The installed executable path must be absolute".into());
        }
        let bundle = if target.platform == Platform::Mac {
            let bundle = executable
                .ancestors()
                .find(|path| path.extension().is_some_and(|ext| ext == "app"))
                .ok_or("Move the packaged Tervia.app to a writable folder before updating")?;
            if executable.strip_prefix(bundle).ok() != Some(Path::new("Contents/MacOS/Tervia")) {
                return Err("The executable is not inside a Tervia app bundle".into());
            }
            Some(bundle.to_owned())
        } else {
            None
        };
        Ok(Self {
            executable,
            bundle,
            target,
        })
    }

    pub fn discover() -> Result<Self, String> {
        if cfg!(debug_assertions) {
            return Err(
                "Development builds can check for updates; install updates from a packaged release"
                    .into(),
            );
        }
        Self::new(
            std::env::current_exe().map_err(|error| error.to_string())?,
            Target::current()?,
        )
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn bundle(&self) -> Option<&Path> {
        self.bundle.as_deref()
    }
}

struct ConfirmedRelease(Release);

impl ReleaseSource for ConfirmedRelease {
    fn get_releases(&self) -> self_update::Result<Vec<Release>> {
        Ok(vec![self.0.clone()])
    }
}

#[derive(Clone, Debug)]
pub struct Installed {
    version: String,
    executable: PathBuf,
}

impl Installed {
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn relaunch(&self) -> Result<(), String> {
        // current_exe() can point at a renamed/deleted image after replacement (notably on Linux).
        // Always launch the original installation path captured before installing the update.
        Command::new(&self.executable)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub fn install(
    candidate: Candidate,
    installation: &Installation,
    progress: impl Fn(u64, Option<u64>) + Send + Sync + 'static,
) -> Result<Installed, String> {
    if candidate.target != installation.target {
        return Err("The update package does not match the installation platform".into());
    }
    let name = candidate.target.asset_name(candidate.version());
    let version = candidate.version().to_owned();
    let mut builder = self_update::backends::custom::Update::configure();
    // Pin both the approved release and its digest; a new release or replaced asset must not
    // change what is installed between checking, confirmation, and downloading.
    builder
        .source(ConfirmedRelease(candidate.release))
        .bin_name("tervia")
        .current_version(VERSION)
        .release_tag(&version)
        .asset_matcher(move |assets| assets.iter().find(|asset| asset.name() == name).cloned())
        .verify_checksum(self_update::Checksum::Sha256(candidate.digest))
        .timeout(Duration::from_secs(600))
        .show_output(false)
        .no_confirm(true)
        .check_install_path_writable(true)
        .progress_callback(progress);
    if let Some(bundle) = installation.bundle() {
        // Replacing only Contents/MacOS/Tervia would invalidate the signed bundle and leave stale resources.
        builder
            .bundle_path_in_archive("Tervia.app")
            .bundle_install_path(bundle)
            .verify_binary(verify_bundle);
    } else {
        builder
            .bin_install_path(installation.executable())
            .bin_path_in_archive(if installation.target.platform == Platform::Windows {
                "Tervia.exe"
            } else {
                "Tervia/tervia"
            });
    }
    let status = builder
        .build()
        .and_then(|updater| updater.update())
        .map_err(|error| error.to_string())?;
    if !status.is_updated() {
        return Err("The confirmed update was not installed".into());
    }
    Ok(Installed {
        version,
        executable: installation.executable.clone(),
    })
}

fn verify_bundle(bundle: &Path) -> self_update::Result<()> {
    for (program, arguments) in [
        (
            "/usr/bin/codesign",
            &["--verify", "--deep", "--strict"] as &[&str],
        ),
        (
            "/usr/sbin/spctl",
            &["--assess", "--type", "execute"] as &[&str],
        ),
    ] {
        let output = Command::new(program).args(arguments).arg(bundle).output()?;
        if !output.status.success() {
            return Err(self_update::Error::verification_rejected(format!(
                "macOS signature verification failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }
    Ok(())
}
