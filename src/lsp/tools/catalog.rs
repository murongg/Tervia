use super::*;

// Pin versions and publisher-provided hashes together; a new app release can
// advance them without changing an existing installation under a running server.
pub(super) fn packages(service: Service, target: Target) -> Result<Vec<Package>, String> {
    Ok(match service {
        Service::Rust => {
            let (triple, digest) = match (target.platform, target.arch) {
                (Platform::Mac, "aarch64") => (
                    "aarch64-apple-darwin",
                    "eedad08a96f7bf92afd0890463787337d17591ea2e3df216033c6bc3ccc9d802",
                ),
                (Platform::Mac, "x86_64") => (
                    "x86_64-apple-darwin",
                    "bdfd46e82731a8d3502d852d28b6462c1f1c13eed0b3e76bbcf1ba839028249c",
                ),
                (Platform::Windows, "x86_64") => (
                    "x86_64-pc-windows-msvc",
                    "7df6e1a2595d1906e8441cb83fd58170b47d32eb09bc52c1b2184538cfd49a71",
                ),
                (Platform::Windows, "aarch64") => (
                    "aarch64-pc-windows-msvc",
                    "dc2b6c35e2be0bf13f4ae2a6c69adca5730c91d011bfac31371279a4e55bcb2b",
                ),
                (Platform::Linux, "x86_64") => (
                    "x86_64-unknown-linux-gnu",
                    "8fde1c9dae81459f98e99eedae3654846a9166abea686306c26e57773be558da",
                ),
                (Platform::Linux, "aarch64") => (
                    "aarch64-unknown-linux-gnu",
                    "d31ee384ab788b9abd8e7aa8e1322f23e6cd4310471d373e9a79ce4656e21896",
                ),
                _ => return Err("No rust-analyzer distribution for this platform".into()),
            };
            let windows = target.platform == Platform::Windows;
            vec![Package {
                url: format!(
                    "https://github.com/rust-lang/rust-analyzer/releases/download/2026-10-19/rust-analyzer-{triple}.{}",
                    if windows { "zip" } else { "gz" }
                ),
                digest: Hash::Sha256(digest.into()),
                layout: Layout::Binary {
                    kind: if windows {
                        ArchiveKind::Zip
                    } else {
                        ArchiveKind::Plain(Some(Compression::Gz))
                    },
                    file: if windows {
                        "rust-analyzer.exe"
                    } else {
                        "rust-analyzer"
                    }
                    .into(),
                },
            }]
        }
        Service::TypeScript => vec![
            npm(
                "typescript-language-server",
                "6.0.2",
                "Ph7Ql+xkUJvMamnkh1h+aKP6HxLR7lrWIYkPn9/LZBkJdJZgtIj7OoVhkDYtQ7zFPTPmqZHOVXaUKGyz9M2q1Q==",
                "node_modules/typescript-language-server",
            ),
            npm(
                "typescript",
                "6.0.3",
                "y2TvuxSZPDyQakkFRPZHKFm+KKVqIisdg9/CZwm9ftvKXLP8NRWj38/ODjNbr43SsoXqNuAisEf1GdCxqWcdBw==",
                "node_modules/typescript",
            ),
        ],
        Service::Python => vec![npm(
            "pyright",
            "1.1.414",
            "FPZZb51jepDX4eP7TEYDeNFtmE3WgwkkEcJpvH3/QmUSsj0EAy3LXu+xB4T/FWDejtsXlCfFr/rbWFjwuwuXww==",
            "node_modules/pyright",
        )],
        Service::Go => Vec::new(),
    })
}
fn npm(name: &str, version: &str, digest: &str, directory: &'static str) -> Package {
    Package {
        url: format!("https://registry.npmjs.org/{name}/-/{name}-{version}.tgz"),
        digest: Hash::Sha512(digest.into()),
        layout: Layout::Npm { directory },
    }
}
