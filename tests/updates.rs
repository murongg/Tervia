use self_update::{Release, ReleaseAsset};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};
use tervia::{
    platform::Platform,
    updates::{Candidate, Installation, Target},
};

fn release(version: &str, target: Target, digest: Option<&str>) -> Release {
    let name = target.asset_name(version);
    let mut asset = ReleaseAsset::new(&name, format!("https://example.test/{name}"));
    if let Some(digest) = digest {
        asset = asset.with_digest(digest);
    }
    Release::builder()
        .version(version)
        .asset(asset)
        .build()
        .unwrap()
}

#[test]
fn release_assets_match_the_existing_platform_packages_exactly() {
    for (platform, arch, suffix) in [
        (Platform::Mac, "aarch64", "macos-arm64.zip"),
        (Platform::Mac, "x86_64", "macos-x86_64.zip"),
        (Platform::Windows, "x86_64", "windows-x64.zip"),
        (Platform::Linux, "x86_64", "linux-x86_64.tar.gz"),
        (Platform::Linux, "aarch64", "linux-aarch64.tar.gz"),
    ] {
        assert_eq!(
            Target::new(platform, arch).unwrap().asset_name("1.2.0"),
            format!("Tervia-v1.2.0-{suffix}")
        );
    }
    assert!(Target::new(Platform::Windows, "aarch64").is_err());
}

#[test]
fn stable_semver_comparison_prevents_downgrades_and_build_metadata_updates() {
    let target = Target::new(Platform::Linux, "x86_64").unwrap();
    let digest = format!("sha256:{}", "ab".repeat(32));
    for (current, remote, available) in [
        ("1.2.9", "1.2.10", true),
        ("1.2.0", "1.2.0", false),
        ("1.3.0", "1.2.0", false),
        ("1.2.0", "1.3.0-beta.1", false),
        ("1.2.0-beta.2", "1.2.0", true),
        ("1.2.0+old", "1.2.0+new", false),
    ] {
        assert_eq!(
            Candidate::from_release(release(remote, target, Some(&digest)), current, target)
                .unwrap()
                .is_some(),
            available
        );
    }
}

#[test]
fn an_update_requires_an_exact_architecture_asset_and_valid_digest() {
    let target = Target::new(Platform::Mac, "aarch64").unwrap();
    let other = Target::new(Platform::Mac, "x86_64").unwrap();
    let digest = format!("sha256:{}", "ab".repeat(32));
    assert!(
        Candidate::from_release(release("1.2.0", other, Some(&digest)), "1.1.0", target).is_err()
    );
    for digest in [
        None,
        Some("sha256:abc"),
        Some("md5:abcdef"),
        Some("sha256:zz"),
    ] {
        assert!(
            Candidate::from_release(release("1.2.0", target, digest), "1.1.0", target).is_err()
        );
    }
    assert!(
        Candidate::from_release(release("1.2.0", target, Some(&digest)), "invalid", target)
            .is_err()
    );
}

#[test]
fn macos_installation_resolves_the_bundle_root_and_original_launch_path() {
    let target = Target::new(Platform::Mac, "aarch64").unwrap();
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("Tervia.app");
    let executable = bundle.join("Contents/MacOS/Tervia");
    let install = Installation::new(executable.clone(), target).unwrap();
    assert_eq!(install.bundle().unwrap(), bundle);
    assert_eq!(install.executable(), executable);
    assert!(Installation::new(root.path().join("target/debug/tervia"), target).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn an_unsigned_macos_bundle_cannot_replace_the_installed_application() {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive
        .start_file(
            "Tervia.app/Contents/MacOS/Tervia",
            zip::write::SimpleFileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
    archive.write_all(b"synthetic unsigned executable").unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    let (url, server) = serve_archive(bytes);
    let target = Target::new(Platform::Mac, std::env::consts::ARCH).unwrap();
    let release = Release::builder()
        .version("9.0.0")
        .asset(ReleaseAsset::new(target.asset_name("9.0.0"), url).with_digest(digest))
        .build()
        .unwrap();
    let candidate = Candidate::from_release(release, "1.0.0", target)
        .unwrap()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("Tervia.app/Contents/MacOS/Tervia");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, b"synthetic original executable").unwrap();
    let installation = Installation::new(executable.clone(), target).unwrap();
    let error = tervia::updates::install(candidate, &installation, |_, _| {}).unwrap_err();
    server.join().unwrap();
    assert!(error.contains("signature verification failed"), "{error}");
    assert_eq!(
        std::fs::read(executable).unwrap(),
        b"synthetic original executable"
    );
}

fn serve_archive(bytes: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/synthetic.zip", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&mut stream);
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        stream.write_all(&bytes).unwrap();
    });
    (url, thread)
}

#[test]
fn verified_archive_installs_only_the_confirmed_release_into_a_synthetic_destination() {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive
        .start_file("Tervia.exe", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"synthetic new executable").unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    let (url, server) = serve_archive(bytes);
    let target = Target::new(Platform::Windows, "x86_64").unwrap();
    let release = Release::builder()
        .version("9.0.0")
        .asset(ReleaseAsset::new(target.asset_name("9.0.0"), url).with_digest(digest))
        .build()
        .unwrap();
    let candidate = Candidate::from_release(release, "1.0.0", target)
        .unwrap()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Tervia.exe");
    std::fs::write(&path, b"synthetic original executable").unwrap();
    let installation = Installation::new(path.clone(), target).unwrap();
    let installed = tervia::updates::install(candidate, &installation, |_, _| {}).unwrap();
    server.join().unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"synthetic new executable");
    assert_eq!(installed.version(), "9.0.0");
    assert_eq!(installed.executable(), installation.executable());
}

#[test]
fn checksum_failure_leaves_the_installed_app_untouched() {
    let (url, server) = serve_archive(b"synthetic corrupt archive".to_vec());
    let target = Target::new(Platform::Windows, "x86_64").unwrap();
    let release = Release::builder()
        .version("9.0.0")
        .asset(
            ReleaseAsset::new(target.asset_name("9.0.0"), url)
                .with_digest(format!("sha256:{}", "00".repeat(32))),
        )
        .build()
        .unwrap();
    let candidate = Candidate::from_release(release, "1.0.0", target)
        .unwrap()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Tervia.exe");
    std::fs::write(&path, b"synthetic original executable").unwrap();
    let installation = Installation::new(path.clone(), target).unwrap();
    let error = tervia::updates::install(candidate, &installation, |_, _| {}).unwrap_err();
    server.join().unwrap();
    assert!(error.to_lowercase().contains("checksum"), "{error}");
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"synthetic original executable"
    );
}
