use std::fs;
use tervia::tasks::{Task, discover};

#[path = "tasks/just.rs"]
mod just;
#[path = "tasks/make.rs"]
mod make;
#[path = "tasks/taskfile.rs"]
mod taskfile;

fn write(root: &std::path::Path, name: &str, content: &str) {
    fs::write(root.join(name), content).unwrap();
}

#[test]
fn combines_task_families_and_bounds_each_source_without_hiding_valid_sources() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "package.json",
        r#"{"scripts":{"mock-js":"echo mock"}}"#,
    );
    write(root.path(), "Cargo.toml", "[workspace]\nmembers = []\n");
    write(root.path(), "Makefile", "mock-make:\n\t@echo mock\n");
    write(root.path(), "justfile", "mock-just:\n  echo mock\n");
    write(
        root.path(),
        "Taskfile.yml",
        "version: '3'\ntasks:\n  mock-task: echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.program.as_str())
            .collect::<Vec<_>>(),
        ["npm", "cargo", "cargo", "cargo", "make", "just", "task"]
    );
    for file in ["Makefile", "justfile", "Taskfile.yml"] {
        write(root.path(), file, &" ".repeat(1024 * 1024 + 1));
    }
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks.len(), 4);
    assert_eq!(catalog.warnings.len(), 3);
}

#[cfg(unix)]
#[test]
fn non_regular_task_sources_report_an_error_instead_of_blocking_discovery() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let root = tempfile::tempdir().unwrap();
    let path = CString::new(root.path().join("Makefile").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let catalog = discover(root.path());
    assert!(catalog.tasks.is_empty());
    assert!(
        catalog
            .warnings
            .iter()
            .any(|warning| warning.contains("regular file"))
    );
}

#[test]
fn discovers_named_scripts_in_stable_order_without_executing_them() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "package.json",
        r#"{"scripts":{"test":"echo mock","dev":"touch should-not-exist","invalid":4}}"#,
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["dev", "test"]
    );
    assert_eq!(catalog.tasks[0].program, "npm");
    assert_eq!(catalog.tasks[0].args, ["run", "dev"]);
    assert_eq!(catalog.tasks[0].source, "package.json");
    assert!(!root.path().join("should-not-exist").exists());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn declared_package_manager_wins_over_lockfiles() {
    for manager in ["npm", "pnpm", "yarn", "bun"] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "package.json",
            &format!(
                r#"{{"packageManager":"{manager}@1.0.0","scripts":{{"mock:check":"echo mock"}}}}"#
            ),
        );
        write(root.path(), "package-lock.json", "{}");
        let catalog = discover(root.path());
        assert_eq!(catalog.tasks[0].program, manager);
        assert_eq!(catalog.tasks[0].args, ["run", "mock:check"]);
    }
}

#[test]
fn infers_package_manager_from_one_lockfile_and_reports_ambiguity() {
    for (file, manager) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
    ] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "package.json",
            r#"{"scripts":{"dev":"echo mock"}}"#,
        );
        write(root.path(), file, "mock");
        assert_eq!(discover(root.path()).tasks[0].program, manager);
        if manager != "npm" {
            write(root.path(), "package-lock.json", "{}");
            let catalog = discover(root.path());
            assert!(catalog.tasks.is_empty());
            assert!(!catalog.warnings.is_empty());
        }
    }
}

#[test]
fn reads_only_the_selected_directory_and_keeps_other_sources_after_a_parse_error() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "package.json", "{ invalid");
    write(root.path(), "Cargo.toml", "[workspace]\nmembers = []\n");
    fs::create_dir(root.path().join("child")).unwrap();
    write(
        &root.path().join("child"),
        "package.json",
        r#"{"scripts":{"nested":"echo mock"}}"#,
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.display())
            .collect::<Vec<_>>(),
        ["cargo build", "cargo check", "cargo test"]
    );
    assert!(catalog.warnings[0].contains("package.json"));
    assert!(discover(&root.path().join("missing")).tasks.is_empty());
}

#[test]
fn cargo_run_tasks_name_explicit_binary_targets() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname = 'mock-app'\nversion = '0.1.0'\n[[bin]]\nname = 'mock-server'\n[[bin]]\nname = 'mock-client'\n",
    );
    let catalog = discover(root.path());
    let runs: Vec<_> = catalog
        .tasks
        .iter()
        .filter(|task| task.args.first().is_some_and(|arg| arg == "run"))
        .map(|task| task.display())
        .collect();
    assert_eq!(
        runs,
        ["cargo run --bin mock-client", "cargo run --bin mock-server"]
    );
}

#[test]
fn cargo_run_tasks_enable_required_features_and_respect_disabled_autobins() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname = 'mock-library'\nversion = '0.1.0'\nautobins = false\n[[bin]]\nname = 'mock-app'\nrequired-features = ['mock-native', 'mock-pty']\n",
    );
    fs::create_dir(root.path().join("src")).unwrap();
    write(&root.path().join("src"), "main.rs", "fn main() {}\n");
    let catalog = discover(root.path());
    let runs: Vec<_> = catalog
        .tasks
        .iter()
        .filter(|task| task.args[0] == "run")
        .map(|task| task.args.clone())
        .collect();
    assert_eq!(
        runs,
        [vec![
            "run",
            "--bin",
            "mock-app",
            "--features",
            "mock-native,mock-pty"
        ]]
    );
}

#[test]
fn oversized_manifests_and_unsupported_managers_are_visible_errors() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "package.json", &" ".repeat(1024 * 1024 + 1));
    assert!(!discover(root.path()).warnings.is_empty());
    write(
        root.path(),
        "package.json",
        r#"{"packageManager":"mock-unknown@1","scripts":{"dev":"echo mock"}}"#,
    );
    let catalog = discover(root.path());
    assert!(catalog.tasks.is_empty());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn task_metadata_round_trips_and_rejects_invalid_commands() {
    let task = Task {
        label: "mock:check".into(),
        source: "package.json".into(),
        program: "npm".into(),
        args: vec!["run".into(), "mock:check".into()],
    };
    assert_eq!(Task::from_json(&task.to_json()).unwrap(), task);
    let mut invalid = task.to_json();
    invalid["program"] = "unexpected-program".into();
    assert!(Task::from_json(&invalid).is_err());
    invalid = task.to_json();
    invalid["args"] = serde_json::json!(["run", "bad\nname"]);
    assert!(Task::from_json(&invalid).is_err());
}
