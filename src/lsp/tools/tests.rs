use super::*;
use std::sync::atomic::AtomicUsize;

struct Fixture {
    bytes: Vec<u8>,
    calls: AtomicUsize,
    fail: AtomicBool,
    bad_hash: AtomicBool,
    waiting: AtomicBool,
    late: AtomicBool,
}
impl Source for Fixture {
    fn packages(&self, _: Service, _: Target) -> Result<Vec<Package>, String> {
        let digest = if self.bad_hash.load(Ordering::Relaxed) {
            "0".repeat(64)
        } else {
            format!("{:x}", Sha256::digest(&self.bytes))
        };
        Ok(vec![Package {
            url: "https://example.test/synthetic.zip".into(),
            digest: Hash::Sha256(digest),
            layout: Layout::Binary {
                kind: ArchiveKind::Zip,
                file: "rust-analyzer.exe".into(),
            },
        }])
    }
    fn fetch(&self, _: &Package, path: &Path, context: &Context<'_>) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        while self.waiting.load(Ordering::Relaxed) {
            if let Err(error) = context.check() {
                while self.late.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(5));
                }
                context.report(Stage::Downloading {
                    received: 1,
                    total: Some(2),
                });
                return Err(error);
            }
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(path, &self.bytes).map_err(|error| error.to_string())?;
        context.report(Stage::Downloading {
            received: self.bytes.len() as u64,
            total: Some(self.bytes.len() as u64),
        });
        if self.fail.load(Ordering::Relaxed) {
            return Err("synthetic interrupted download".into());
        }
        context.check()
    }
}
fn fixture() -> (tempfile::TempDir, Tools, Arc<Fixture>) {
    use std::io::{Cursor, Write};
    let root = tempfile::tempdir().unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "rust-analyzer.exe",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"synthetic language service").unwrap();
    let fixture = Arc::new(Fixture {
        bytes: zip.finish().unwrap().into_inner(),
        calls: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
        bad_hash: AtomicBool::new(false),
        waiting: AtomicBool::new(false),
        late: AtomicBool::new(false),
    });
    let mut tools = Tools::with_paths(root.path().join("services"), vec![]);
    tools.target = Target::new(crate::platform::Platform::Windows, "x86_64").unwrap();
    tools.source = fixture.clone();
    (root, tools, fixture)
}
fn prepare(tools: &Tools) -> Result<Launch, String> {
    tools.prepare(
        &Server::for_language("rust").unwrap(),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(5),
        &Arc::default(),
    )
}

#[test]
fn pinned_catalogs_cover_supported_platforms_and_only_supported_services() {
    for platform in [
        crate::platform::Platform::Mac,
        crate::platform::Platform::Windows,
        crate::platform::Platform::Linux,
    ] {
        for arch in ["aarch64", "x86_64"] {
            let target = Target::new(platform, arch).unwrap();
            let packages = Official.packages(Service::Rust, target).unwrap();
            assert_eq!(packages.len(), 1);
            assert!(
                packages[0]
                    .url
                    .starts_with("https://github.com/rust-lang/rust-analyzer/releases/download/")
            );
            assert!(matches!(&packages[0].digest, Hash::Sha256(hash) if hash.len() == 64));
        }
    }
    assert!(Target::new(crate::platform::Platform::Linux, "synthetic-arch").is_err());
    let target = Target::current().unwrap();
    assert_eq!(
        Official
            .packages(Service::TypeScript, target)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(Official.packages(Service::Python, target).unwrap().len(), 1);
    assert!(
        Service::from_server(&Server {
            program: "synthetic-custom-server".into(),
            args: vec![]
        })
        .is_none()
    );
}

#[test]
fn a_verified_install_is_published_once_and_reused_by_another_client() {
    let (_root, tools, fixture) = fixture();
    let first = prepare(&tools).unwrap();
    assert!(Path::new(&first.server.program).is_file());
    let second = prepare(&tools).unwrap();
    assert_eq!(first.server, second.server);
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
    let mut another = Tools::with_paths(tools.root.clone().unwrap(), vec![]);
    another.target = tools.target;
    another.source = fixture.clone();
    prepare(&another).unwrap();
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn corrupt_or_interrupted_downloads_never_become_ready_and_can_be_retried() {
    for corrupt in [false, true] {
        let (_root, tools, fixture) = fixture();
        fixture.fail.store(!corrupt, Ordering::Relaxed);
        fixture.bad_hash.store(corrupt, Ordering::Relaxed);
        assert!(prepare(&tools).is_err());
        assert!(
            !tools
                .installation(Service::Rust)
                .unwrap()
                .join("ready")
                .exists()
        );
        fixture.fail.store(false, Ordering::Relaxed);
        fixture.bad_hash.store(false, Ordering::Relaxed);
        prepare(&tools).unwrap();
        assert!(
            tools
                .installation(Service::Rust)
                .unwrap()
                .join("ready")
                .is_file()
        );
        assert_eq!(fixture.calls.load(Ordering::Relaxed), 2);
    }
}

#[test]
fn two_simultaneous_installers_share_a_process_lock() {
    let (_root, tools, fixture) = fixture();
    let tools = Arc::new(tools);
    let first = {
        let tools = tools.clone();
        std::thread::spawn(move || prepare(&tools))
    };
    let second = {
        let tools = tools.clone();
        std::thread::spawn(move || prepare(&tools))
    };
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn canceled_requests_do_not_start_installing_and_missing_runtimes_are_actionable() {
    let (_root, tools, fixture) = fixture();
    let status = Arc::default();
    assert!(
        tools
            .prepare(
                &Server::for_language("rust").unwrap(),
                &AtomicBool::new(true),
                Instant::now() + Duration::from_secs(1),
                &status
            )
            .is_err()
    );
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 0);
    for language in ["typescript", "python", "go"] {
        let error = tools
            .prepare(
                &Server::for_language(language).unwrap(),
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(1),
                &status,
            )
            .unwrap_err();
        assert!(error.contains("Node.js") || error.contains("Go"));
        assert!(matches!(
            status.lock().unwrap().as_ref().unwrap().stage,
            Stage::Runtime(_)
        ));
    }
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 0);
}

#[cfg(unix)]
#[test]
fn go_runtime_probes_use_only_the_installed_toolchain() {
    use std::os::unix::fs::PermissionsExt;
    let (root, tools, _) = fixture();
    let program = root.path().join("go");
    fs::write(&program, "#!/bin/sh\nif [ \"$GOTOOLCHAIN\" != local ]; then exit 1; fi\nprintf 'go version go1.27.0 linux/amd64\\n'\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let context = Context {
        service: Service::Go,
        stop: &AtomicBool::new(false),
        deadline: Instant::now() + Duration::from_secs(3),
        progress: &Arc::default(),
        paths: vec![root.path().into()],
    };
    assert_eq!(tools.runtime(&context).unwrap(), Some(program));
}

#[cfg(unix)]
#[test]
fn a_working_system_server_is_reused_but_a_broken_rustup_proxy_is_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let (_root, mut tools, fixture) = fixture();
    let bin = tools.root.clone().unwrap().join("synthetic-bin");
    fs::create_dir_all(&bin).unwrap();
    let path = bin.join("rust-analyzer");
    fs::write(&path, "#!/bin/sh\nprintf 'rust-analyzer synthetic'\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    tools.paths = Some(vec![bin]);
    let launch = prepare(&tools).unwrap();
    assert_eq!(Path::new(&launch.server.program), path);
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 0);
    fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
    let launch = prepare(&tools).unwrap();
    assert_ne!(Path::new(&launch.server.program), path);
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn closing_a_client_cancels_preparation_without_waiting_for_the_download() {
    let (root, tools, fixture) = fixture();
    fixture.waiting.store(true, Ordering::Relaxed);
    let client = crate::lsp::Client::with_tools(Arc::new(tools));
    let worker = client.clone();
    let path = root.path().to_path_buf();
    let query = std::thread::spawn(move || {
        worker.definitions(
            &Server::for_language("rust").unwrap(),
            crate::lsp::Query {
                root: path,
                path: "mock.rs".into(),
                offset: 0,
                documents: vec![],
            },
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while fixture.calls.load(Ordering::Relaxed) == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    client.shutdown();
    assert!(started.elapsed() < Duration::from_millis(100));
    assert!(query.join().unwrap().unwrap_err().contains("canceled"));
}

#[cfg(unix)]
#[test]
fn typescript_navigation_does_not_start_extra_workers_or_type_downloads() {
    use std::os::unix::fs::PermissionsExt;
    for language in ["typescript", "rust"] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let server = Server::for_language(language).unwrap();
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = '--version' ]; then printf 'synthetic version'; exit 0; fi\nexec python3 '{}/tests/fixtures/lsp.py' normal\n",
            env!("CARGO_MANIFEST_DIR")
        );
        let program = bin.join(&server.program);
        fs::write(&program, script).unwrap();
        fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(root.path().join("target.ts"), "synthetic").unwrap();
        let client = crate::lsp::Client::with_tools(Arc::new(Tools::with_paths(
            root.path().join("services"),
            vec![bin],
        )));
        let found = client
            .definitions(
                &server,
                crate::lsp::Query {
                    root: root.path().into(),
                    path: "mock.ts".into(),
                    offset: 0,
                    documents: vec![crate::lsp::Document {
                        path: "mock.ts".into(),
                        language: language.into(),
                        text: "mock()".into(),
                    }],
                },
            )
            .unwrap();
        assert_eq!(found.len(), 1);
        client.shutdown();
        let log: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
                .unwrap();
        if language == "typescript" {
            let options = crate::lsp::process::node_options(std::env::var_os("NODE_OPTIONS"));
            assert_eq!(
                log["small_node_allocation"],
                options
                    .to_string_lossy()
                    .contains("--max-semi-space-size=8")
            );
            assert_eq!(
                log["initialization_options"]["tsserver"]["useSyntaxServer"],
                "never"
            );
            assert_eq!(
                log["initialization_options"]["disableAutomaticTypingAcquisition"],
                true
            );
        } else {
            assert!(log["initialization_options"].is_null());
            assert_eq!(
                log["small_node_allocation"],
                std::env::var("NODE_OPTIONS")
                    .unwrap_or_default()
                    .contains("--max-semi-space-size=8")
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn repeated_queries_do_not_reprobe_an_already_running_service() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let counter = root.path().join("probes");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = '--version' ]; then printf 'probe\\n' >> '{}'; printf 'rust-analyzer synthetic'; exit 0; fi\nexec python3 '{}/tests/fixtures/lsp.py' normal\n",
        counter.display(),
        env!("CARGO_MANIFEST_DIR")
    );
    fs::write(bin.join("rust-analyzer"), script).unwrap();
    fs::set_permissions(bin.join("rust-analyzer"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.path().join("target.ts"), "synthetic").unwrap();
    let tools = Tools::with_paths(root.path().join("services"), vec![bin]);
    let client = crate::lsp::Client::with_tools(Arc::new(tools));
    let query = crate::lsp::Query {
        root: root.path().into(),
        path: "mock.ts".into(),
        offset: 0,
        documents: vec![crate::lsp::Document {
            path: "mock.ts".into(),
            language: "rust".into(),
            text: "mock()".into(),
        }],
    };
    let server = Server::for_language("rust").unwrap();
    client.definitions(&server, query.clone()).unwrap();
    client.definitions(&server, query).unwrap();
    client.shutdown();
    assert_eq!(fs::read_to_string(counter).unwrap().lines().count(), 1);
}

#[cfg(unix)]
#[test]
fn go_install_reuses_the_runtime_and_discards_private_build_caches() {
    use std::os::unix::fs::PermissionsExt;
    let (root, mut tools, _) = fixture();
    tools.target = Target::current().unwrap();
    let bin = root.path().join("synthetic-bin");
    fs::create_dir(&bin).unwrap();
    let log = root.path().join("build.log");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = 'version' ]; then printf 'go version go1.27.2 darwin/arm64'; exit 0; fi\n[ \"$GOTOOLCHAIN\" = 'local' ] || exit 1\nmkdir -p \"$GOBIN\" \"$GOCACHE\" \"$GOMODCACHE\"\nprintf 'synthetic-gopls' > \"$GOBIN/gopls\"\nprintf '%s\\n' \"$GOTOOLCHAIN\" \"$GOCACHE\" \"$GOMODCACHE\" > '{}'\n",
        log.display()
    );
    fs::write(bin.join("go"), script).unwrap();
    fs::set_permissions(bin.join("go"), fs::Permissions::from_mode(0o755)).unwrap();
    tools.paths = Some(vec![bin]);
    let launch = tools
        .prepare(
            &Server::for_language("go").unwrap(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(5),
            &Arc::default(),
        )
        .unwrap();
    assert!(Path::new(&launch.server.program).is_file());
    let log = fs::read_to_string(log).unwrap();
    assert_eq!(log.lines().next(), Some("local"));
    for path in log.lines().skip(1) {
        assert!(
            !Path::new(path).exists(),
            "Private build caches should leave with the staging directory"
        );
    }
}

#[test]
fn canceled_download_progress_cannot_replace_a_later_runtime_prompt() {
    let (root, tools, fixture) = fixture();
    fixture.waiting.store(true, Ordering::Relaxed);
    fixture.late.store(true, Ordering::Relaxed);
    let client = crate::lsp::Client::with_tools(Arc::new(tools));
    let worker = client.clone();
    let path = root.path().to_path_buf();
    let pending = thread::spawn(move || {
        worker.definitions(
            &Server::for_language("rust").unwrap(),
            crate::lsp::Query {
                root: path,
                path: "mock.rs".into(),
                offset: 0,
                documents: vec![],
            },
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while fixture.calls.load(Ordering::Relaxed) == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    client.shutdown();
    let result = client.definitions(
        &Server::for_language("typescript").unwrap(),
        crate::lsp::Query {
            root: root.path().into(),
            path: "mock.ts".into(),
            offset: 0,
            documents: vec![],
        },
    );
    let before = client.status();
    fixture.late.store(false, Ordering::Relaxed);
    assert!(pending.join().unwrap().is_err());
    assert!(result.is_err());
    assert!(matches!(before.unwrap().stage, Stage::Runtime(_)));
    assert!(matches!(client.status().unwrap().stage, Stage::Runtime(_)));
}

#[cfg(unix)]
#[test]
fn a_server_that_reports_a_version_but_cannot_initialize_gets_a_managed_fallback() {
    use std::os::unix::fs::PermissionsExt;
    let (root, mut tools, fixture) = fixture();
    let bin = root.path().join("synthetic-bin");
    fs::create_dir(&bin).unwrap();
    fs::write(bin.join("rust-analyzer"), "#!/bin/sh\nif [ \"$1\" = '--version' ]; then printf 'synthetic rust-analyzer'; exit 0; fi\nexit 1\n").unwrap();
    fs::set_permissions(bin.join("rust-analyzer"), fs::Permissions::from_mode(0o755)).unwrap();
    tools.paths = Some(vec![bin]);
    let client = crate::lsp::Client::with_tools(Arc::new(tools));
    let result = client.definitions(
        &Server::for_language("rust").unwrap(),
        crate::lsp::Query {
            root: root.path().into(),
            path: "mock.rs".into(),
            offset: 0,
            documents: vec![],
        },
    );
    // This fixture's managed binary is intentionally synthetic. It cannot run,
    // but preparation must have replaced the broken system service exactly once.
    assert!(result.is_err());
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn progress_from_an_earlier_lookup_cannot_hide_the_current_runtime_prompt() {
    let (root, tools, fixture) = fixture();
    fixture.waiting.store(true, Ordering::Relaxed);
    let client = crate::lsp::Client::with_tools(Arc::new(tools));
    let worker = client.clone();
    let path = root.path().to_path_buf();
    let pending = thread::spawn(move || {
        worker.definitions(
            &Server::for_language("rust").unwrap(),
            crate::lsp::Query {
                root: path,
                path: "mock.rs".into(),
                offset: 0,
                documents: vec![],
            },
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while fixture.calls.load(Ordering::Relaxed) == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let result = client.definitions(
        &Server::for_language("typescript").unwrap(),
        crate::lsp::Query {
            root: root.path().into(),
            path: "mock.ts".into(),
            offset: 0,
            documents: vec![],
        },
    );
    let before = client.status();
    fixture.waiting.store(false, Ordering::Relaxed);
    assert!(pending.join().unwrap().is_err());
    assert!(result.is_err());
    assert!(matches!(before.unwrap().stage, Stage::Runtime(_)));
    assert!(matches!(client.status().unwrap().stage, Stage::Runtime(_)));
    client.shutdown();
}

#[test]
fn a_queued_lookup_cannot_begin_installation_after_the_project_closes() {
    let (root, tools, fixture) = fixture();
    let client = crate::lsp::Client::with_tools(Arc::new(tools));
    let request = client
        .definition_request(
            &Server::for_language("rust").unwrap(),
            crate::lsp::Query {
                root: root.path().into(),
                path: "mock.rs".into(),
                offset: 0,
                documents: vec![],
            },
        )
        .unwrap();
    client.shutdown();
    assert!(request.run().unwrap_err().contains("canceled"));
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 0);
}
