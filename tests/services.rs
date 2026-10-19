use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tervia::lsp::{Client, Document, Query, Server, target_path, tools::Tools};

fn client(cache: &Path) -> Client {
    let node = PathBuf::from(
        std::env::var_os("TERVIA_TEST_NODE")
            .expect("Set TERVIA_TEST_NODE to an installed Node executable"),
    );
    Client::with_tools(Arc::new(Tools::with_paths(
        cache.into(),
        vec![node.parent().unwrap().into()],
    )))
}

#[test]
#[ignore = "downloads pinned language services into a temporary cache; requires Node.js 22.22.2+"]
fn managed_node_services_install_on_demand_and_reuse_the_cache() {
    let temporary = tempfile::tempdir().unwrap();
    let cache = std::env::var_os("TERVIA_SERVICE_TEST_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("services"));
    let project = temporary.path().join("project");
    fs::create_dir(&project).unwrap();
    let source = "import { mock as runMock } from './target';\nconst banner = '😀'; runMock();\n";
    let target = "export function mock() { return 1; }\n";
    fs::write(
        project.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true},"include":["*.ts"]}"#,
    )
    .unwrap();
    fs::write(project.join("mock.ts"), source).unwrap();
    fs::write(project.join("target.ts"), target).unwrap();
    let query = Query {
        root: project.clone(),
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
    };
    let service = Server::for_language("typescript").unwrap();
    for _ in 0..2 {
        let client = client(&cache);
        let found = client.definitions(&service, query.clone()).unwrap();
        assert_eq!(
            target_path(&project, found[0].target_uri.as_str()).unwrap(),
            PathBuf::from("target.ts")
        );
        assert_eq!(found[0].target_selection_range.start.line, 2);
        client.shutdown();
    }
    assert!(!cache.join("rust-analyzer").exists());
    assert!(!cache.join("gopls").exists());
    assert!(!cache.join("pyright").exists());

    let source = "from target import mock as run_mock\nrun_mock()\n";
    let target = "def mock():\n    return 1\n";
    fs::write(project.join("mock.py"), source).unwrap();
    fs::write(project.join("target.py"), target).unwrap();
    let client = client(&cache);
    let found = client
        .definitions(
            &Server::for_language("python").unwrap(),
            Query {
                root: project.clone(),
                path: "mock.py".into(),
                offset: source.rfind("run_mock").unwrap() + 3,
                documents: vec![
                    Document {
                        path: "mock.py".into(),
                        language: "python".into(),
                        text: source.into(),
                    },
                    Document {
                        path: "target.py".into(),
                        language: "python".into(),
                        text: format!("\n{target}"),
                    },
                ],
            },
        )
        .unwrap();
    assert!(!found.is_empty());
    assert_eq!(
        target_path(&project, found[0].target_uri.as_str()).unwrap(),
        PathBuf::from("target.py")
    );
    assert_eq!(found[0].target_selection_range.start.line, 1);
    client.shutdown();
}

#[test]
#[ignore = "downloads pinned rust-analyzer into a temporary cache; requires an installed Rust toolchain"]
fn managed_rust_analyzer_installs_and_resolves_a_definition() {
    let temporary = tempfile::tempdir().unwrap();
    let cache = std::env::var_os("TERVIA_SERVICE_TEST_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("services"));
    let source = "fn mock() {}\nfn caller() { mock(); }\n";
    fs::write(temporary.path().join("Cargo.toml"), "[package]\nname = \"synthetic-lsp-project\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\npath = \"mock.rs\"\n").unwrap();
    fs::write(temporary.path().join("mock.rs"), source).unwrap();
    let client = Client::with_tools(Arc::new(Tools::with_paths(cache, vec![])));
    let found = client
        .definitions(
            &Server::for_language("rust").unwrap(),
            Query {
                root: temporary.path().into(),
                path: "mock.rs".into(),
                offset: source.rfind("mock").unwrap() + 2,
                documents: vec![Document {
                    path: "mock.rs".into(),
                    language: "rust".into(),
                    text: source.into(),
                }],
            },
        )
        .unwrap();
    assert!(!found.is_empty());
    assert_eq!(found[0].target_selection_range.start.line, 0);
    client.shutdown();
}
