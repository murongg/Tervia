use std::{fs, path::PathBuf, time::Duration};
use tervia::lsp::{Client, Document, Query, Server, position, target_path};

fn fixture(mode: &str) -> (tempfile::TempDir, Server, Query) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mock.ts"), "mock()").unwrap();
    fs::write(root.path().join("target.ts"), "export function mock() {}\n").unwrap();
    let server = Server {
        program: if cfg!(windows) { "python" } else { "python3" }.into(),
        args: vec![
            "-u".into(),
            format!("{}/tests/fixtures/lsp.py", env!("CARGO_MANIFEST_DIR")),
            mode.into(),
        ],
    };
    let query = Query {
        root: root.path().to_path_buf(),
        path: "mock.ts".into(),
        offset: 4,
        documents: vec![Document {
            path: "mock.ts".into(),
            language: "typescript".into(),
            text: "mock()".into(),
        }],
    };
    (root, server, query)
}

#[test]
fn installed_server_commands_share_the_typescript_family() {
    assert_eq!(
        Server::for_language("rust").unwrap().program,
        "rust-analyzer"
    );
    assert_eq!(Server::for_language("go").unwrap().program, "gopls");
    assert_eq!(
        Server::for_language("python").unwrap().program,
        "pyright-langserver"
    );
    for language in ["javascript", "typescript", "tsx"] {
        assert_eq!(
            Server::for_language(language),
            Server::for_language("typescript")
        );
    }
    assert!(Server::for_language("markdown").is_none());
}

#[test]
fn jsx_buffers_use_the_react_language_identifier() {
    let (_root, server, mut query) = fixture("jsx");
    query.path = "mock.jsx".into();
    query.documents[0].path = query.path.clone();
    query.documents[0].language = "javascript".into();
    assert_eq!(
        Client::default().definitions(&server, query).unwrap().len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn idle_language_services_exit_and_restart_on_the_next_request() {
    let (root, server, query) = fixture("normal");
    let client = Client::with_idle_timeout(Duration::from_millis(50));
    client.definitions(&server, query.clone()).unwrap();
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    let first = log["pid"].as_i64().unwrap() as i32;
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(unsafe { libc::kill(first, 0) }, -1);
    client.definitions(&server, query).unwrap();
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_ne!(log["pid"].as_i64().unwrap() as i32, first);
    client.shutdown();
}

#[test]
fn the_first_rust_lookup_waits_for_workspace_indexing() {
    let (_root, server, query) = fixture("indexing");
    assert_eq!(
        Client::default().definitions(&server, query).unwrap().len(),
        1
    );
}

#[test]
fn owned_servers_cannot_automatically_download_a_go_toolchain() {
    let (_root, server, query) = fixture("local-toolchain");
    assert_eq!(
        Client::default().definitions(&server, query).unwrap().len(),
        1
    );
}

#[test]
fn positions_and_file_uris_preserve_unicode_and_project_boundaries() {
    let source = "// 合成\r\n😀mock()";
    let offset = source.find("mock").unwrap();
    assert_eq!(position(source, offset).unwrap().line, 1);
    assert_eq!(position(source, offset).unwrap().character, 2);
    assert!(position(source, offset - 1).is_err());
    assert!(position(source, source.len() + 1).is_err());
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("合成 # file.ts");
    fs::write(&path, "synthetic").unwrap();
    let uri = url::Url::from_file_path(&path).unwrap();
    assert_eq!(
        target_path(root.path(), uri.as_str()).unwrap(),
        PathBuf::from("合成 # file.ts")
    );
    assert!(target_path(root.path(), "https://example.test/mock.ts").is_err());
    let outside = tempfile::NamedTempFile::new().unwrap();
    let uri = url::Url::from_file_path(outside.path()).unwrap();
    assert!(target_path(root.path(), uri.as_str()).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, root.path().join("alias.ts")).unwrap();
        let uri = url::Url::from_file_path(root.path().join("alias.ts")).unwrap();
        assert_eq!(
            target_path(root.path(), uri.as_str()).unwrap(),
            PathBuf::from("alias.ts")
        );
        std::os::unix::fs::symlink(outside.path(), root.path().join("link.ts")).unwrap();
        let uri = url::Url::from_file_path(root.path().join("link.ts")).unwrap();
        assert!(target_path(root.path(), uri.as_str()).is_err());
    }
}

#[test]
fn stdio_initializes_syncs_unsaved_buffers_and_handles_server_requests() {
    let (root, server, mut query) = fixture("normal");
    let client = Client::default();
    let found = client.definitions(&server, query.clone()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        target_path(root.path(), found[0].target_uri.as_str()).unwrap(),
        PathBuf::from("target.ts")
    );
    query.documents[0].text = "// 😀\nmock()".into();
    query.offset = query.documents[0].text.find("mock").unwrap();
    query.documents.push(Document {
        path: "target.ts".into(),
        language: "typescript".into(),
        text: "\nexport function mock() {}\n".into(),
    });
    let found = client.definitions(&server, query.clone()).unwrap();
    assert_eq!(found[0].target_selection_range.start.line, 1);
    client.definitions(&server, query).unwrap();
    client.shutdown();
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_eq!(log["initialize"], 1);
    assert_eq!(log["open"], 2);
    assert_eq!(log["change"], 1);
    assert_eq!(log["definition"], 3);
    assert_eq!(log["configuration"], true);
    assert_eq!(log["refused_edit"], true);
}

#[test]
fn locations_links_null_errors_and_unresponsive_servers_are_handled() {
    for mode in ["location", "links", "empty"] {
        let (_root, server, query) = fixture(mode);
        let found = Client::default().definitions(&server, query).unwrap();
        assert_eq!(found.len(), usize::from(mode != "empty"));
    }
    for mode in ["error", "unsupported", "utf8", "malformed"] {
        let (_root, server, query) = fixture(mode);
        assert!(
            Client::default().definitions(&server, query).is_err(),
            "{mode}"
        );
    }
    let (_root, server, query) = fixture("hang");
    let client = Client::with_timeout(Duration::from_millis(150));
    assert!(
        client
            .definitions(&server, query)
            .unwrap_err()
            .contains("timed out")
    );
}

#[test]
fn missing_or_failed_servers_can_be_retried_and_shutdown_can_restart() {
    let (_root, mut server, query) = fixture("normal");
    let client = Client::default();
    let program = server.program.clone();
    server.program = "synthetic-missing-language-server".into();
    assert!(
        client
            .definitions(&server, query.clone())
            .unwrap_err()
            .contains(&server.program)
    );
    server.program = program;
    assert_eq!(client.definitions(&server, query.clone()).unwrap().len(), 1);
    client.shutdown();
    assert_eq!(client.definitions(&server, query).unwrap().len(), 1);
}

#[test]
fn synchronizing_many_open_buffers_does_not_overflow_the_write_queue() {
    let (_root, server, mut query) = fixture("normal");
    for index in 0..32 {
        query.documents.push(Document {
            path: format!("synthetic-{index}.ts").into(),
            language: "typescript".into(),
            text: format!("// {}\n", "x".repeat(64 * 1024)),
        });
    }
    assert_eq!(
        Client::default().definitions(&server, query).unwrap().len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn shutdown_reaps_the_owned_server_before_returning() {
    let (root, server, query) = fixture("normal");
    let client = Client::default();
    client.definitions(&server, query).unwrap();
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    let pid = log["pid"].as_i64().unwrap() as i32;
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    client.shutdown();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

#[test]
#[ignore = "requires an installed typescript-language-server and TypeScript"]
fn typescript_server_resolves_import_aliases_and_unsaved_targets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true},"include":["*.ts"]}"#,
    )
    .unwrap();
    let source = "import { mock as runMock } from './target';\nconst banner = '😀'; runMock();\n";
    let target = "export function mock() { return 1; }\n";
    fs::write(root.path().join("mock.ts"), source).unwrap();
    fs::write(root.path().join("target.ts"), target).unwrap();
    let server = Server {
        program: std::env::var("TERVIA_TYPESCRIPT_SERVER")
            .unwrap_or_else(|_| "typescript-language-server".into()),
        args: vec!["--stdio".into()],
    };
    let client = Client::default();
    let found = client
        .definitions(
            &server,
            Query {
                root: root.path().into(),
                path: "mock.ts".into(),
                offset: source.rfind("runMock").unwrap() + 3,
                documents: vec![
                    Document {
                        path: "mock.ts".into(),
                        language: "typescript".into(),
                        text: source.into(),
                    },
                    Document {
                        path: "target.ts".into(),
                        language: "typescript".into(),
                        text: format!("\n\n{target}"),
                    },
                ],
            },
        )
        .unwrap();
    assert!(!found.is_empty());
    assert_eq!(
        target_path(root.path(), found[0].target_uri.as_str()).unwrap(),
        PathBuf::from("target.ts")
    );
    assert_eq!(found[0].target_selection_range.start.line, 2);
    assert_eq!(found[0].target_selection_range.start.character, 16);
    client.shutdown();
}
