use super::*;

#[test]
fn taskfile_required_variables_are_left_to_the_runner_environment() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Taskfile.yml",
        "version: '3'\nvars:\n  MOCK_ENV: mock\ntasks:\n  deploy:\n    requires:\n      vars: [MOCK_ENV]\n    cmds: ['echo mock']\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["deploy"]
    );
    assert!(catalog.warnings.is_empty());
}

#[test]
fn taskfile_never_expands_filesystem_include_tags() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "mock-hidden.yml", "hidden: echo mock\n");
    write(
        root.path(),
        "Taskfile.yml",
        "version: '3'\ntasks: !include mock-hidden.yml\n",
    );
    let catalog = discover(root.path());
    assert!(catalog.tasks.is_empty());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn taskfile_reads_yaml_shorthands_aliases_and_anchors_without_running_commands() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Taskfile.yml",
        r#"version: '3'
vars:
  MOCK:
    sh: touch should-not-exist
tasks:
  helper: &shared
    internal: true
    cmds: ['echo mock']
  test:
    <<: *shared
    internal: false
    aliases: [t]
  build:
    - echo mock
  check: echo mock
  deploy:
    requires:
      vars: [MOCK_TOKEN]
    cmds: ['echo mock']
  'dynamic-*': echo mock
"#,
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["build", "check", "deploy", "t", "test"]
    );
    assert_eq!(catalog.tasks[0].program, "task");
    assert_eq!(
        catalog.tasks[0].args,
        ["--taskfile", "Taskfile.yml", "build"]
    );
    assert!(!root.path().join("should-not-exist").exists());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn taskfile_uses_documented_filename_precedence_and_supports_dist_variants() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Taskfile.dist.yaml",
        "version: '3'\ntasks:\n  dist: echo mock\n",
    );
    assert_eq!(discover(root.path()).tasks[0].source, "Taskfile.dist.yaml");
    write(
        root.path(),
        "Taskfile.yml",
        "version: '3'\ntasks:\n  local: echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks.len(), 1);
    assert_eq!(catalog.tasks[0].label, "local");
    assert_eq!(
        Task::from_json(&catalog.tasks[0].to_json()).unwrap(),
        catalog.tasks[0]
    );
}

#[test]
fn taskfile_bad_yaml_and_duplicate_keys_leave_other_task_sources_available() {
    for yaml in [
        "version: '3'\ntasks: [broken",
        "version: '3'\ntasks:\n  test: echo first\n  test: echo second\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "Taskfile.yml", yaml);
        write(root.path(), "Makefile", "mock-check:\n\t@echo mock\n");
        let catalog = discover(root.path());
        assert_eq!(catalog.tasks.len(), 1);
        assert_eq!(catalog.tasks[0].program, "make");
        assert!(
            catalog
                .warnings
                .iter()
                .any(|warning| warning.contains("Taskfile.yml"))
        );
    }
}

#[test]
fn taskfile_reports_unexpanded_includes_and_rejects_excessive_yaml_depth() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Taskfile.yml",
        "version: '3'\nincludes:\n  extra: ./mock-tasks.yml\ntasks:\n  root: echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks[0].label, "root");
    assert!(!catalog.warnings.is_empty());
    let nested = format!(
        "version: '3'\nvars:\n  MOCK: {}0{}\ntasks:\n  root: echo mock\n",
        "[".repeat(100),
        "]".repeat(100)
    );
    write(root.path(), "Taskfile.yml", &nested);
    let catalog = discover(root.path());
    assert!(catalog.tasks.is_empty());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn discovery_stays_in_the_selected_directory_and_keeps_the_task_limit() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("child")).unwrap();
    write(
        &root.path().join("child"),
        "Makefile",
        "nested:\n\t@echo mock\n",
    );
    write(
        &root.path().join("child"),
        "justfile",
        "nested:\n  echo mock\n",
    );
    assert!(discover(root.path()).tasks.is_empty());
    let yaml = format!(
        "version: '3'\ntasks:\n{}",
        (0..300)
            .map(|index| format!("  mock-{index:03}: echo mock\n"))
            .collect::<String>()
    );
    write(root.path(), "Taskfile.yml", &yaml);
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks.len(), 256);
    assert!(!catalog.warnings.is_empty());
}
