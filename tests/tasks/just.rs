use super::*;

#[test]
fn just_distinguishes_assignments_with_namespace_colons_from_recipe_headers() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "justfile",
        "value := other::VALUE\nset export\nexport MOCK := 'mock'\nreal text='a:=b':\n  echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["real"]
    );
    assert!(catalog.warnings.is_empty());
}

#[test]
fn just_reports_unrecognized_declarations_instead_of_claiming_complete_discovery() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "justfile",
        "unknown declaration\nreal:\n  echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks[0].label, "real");
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn just_selects_platform_recipes_before_checking_duplicate_names_and_aliases() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "justfile",
        "[private]\n[windows]\nimplementation:\n  echo mock-windows\n[private]\n[unix]\nimplementation:\n  echo mock-unix\nalias run := implementation\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["run"]
    );
    assert!(catalog.warnings.is_empty());
}

#[test]
fn required_parameters_on_private_helpers_do_not_warn_about_public_recipes() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "justfile",
        "_helper target:\n  echo '{{target}}'\nbuild: (_helper 'mock')\n  echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks[0].label, "build");
    assert!(catalog.warnings.is_empty());
}

#[test]
fn just_keeps_empty_and_expression_defaults_runnable_without_arguments() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "justfile",
        "empty filter='':\n  echo mock\nexpression target=(os() + '-mock'):\n  echo mock\nrequired target:\n  echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["empty", "expression"]
    );
}

#[test]
fn just_lists_public_recipes_optional_parameters_and_static_aliases() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "justfile",
        r#"# synthetic recipes
value := `touch should-not-exist`
alias b := build
[private]
hidden:
  echo mock
_helper:
  echo mock
@build mode='mock release': _helper
  echo '{{mode}}'
test *flags:
  echo mock
required target:
  echo '{{target}}'
many +files:
  echo mock
"#,
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["b", "build", "test"]
    );
    assert_eq!(catalog.tasks[0].program, "just");
    assert_eq!(catalog.tasks[0].args, ["--justfile", "justfile", "--", "b"]);
    assert!(!root.path().join("should-not-exist").exists());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn just_ignores_multiline_literals_and_reports_external_modules() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        ".justfile",
        "value := '''\nphantom:\n'''\nscript := `\nbacktick-phantom:\n`\nimport 'mock-extra.just'\nmod extra\nreal:\n  echo mock\n",
    );
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks.len(), 1);
    assert_eq!(catalog.tasks[0].label, "real");
    assert_eq!(catalog.tasks[0].source, ".justfile");
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn just_reports_ambiguous_files_instead_of_choosing_a_different_runner_default() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "justfile", "first:\n  echo mock\n");
    write(root.path(), ".justfile", "second:\n  echo mock\n");
    let catalog = discover(root.path());
    assert!(catalog.tasks.is_empty());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn just_finds_case_insensitive_filenames_and_preserves_the_actual_path() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "Justfile", "mock-test:\n  echo mock\n");
    let task = discover(root.path()).tasks.remove(0);
    assert_eq!(task.source, "Justfile");
    assert_eq!(Task::from_json(&task.to_json()).unwrap(), task);
}
