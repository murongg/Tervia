use super::*;

#[test]
fn make_accepts_space_before_a_phony_colon_without_reading_recipe_continuations_as_rules() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Makefile",
        ".PHONY : mock-phony\nreal:\n\t@printf 'mock \\\nrecipe-phantom: still a command'\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["mock-phony", "real"]
    );
}

#[test]
fn discovers_literal_make_targets_without_evaluating_make_code() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Makefile",
        r#"MOCK := $(shell touch should-not-exist)
URL := https://example.test/mock
.PHONY: test clean \
 build
define MOCK_TEMPLATE
phantom:
endef
all: build
build test: ; @echo mock
	@echo 'recipe-phantom: not a target'
%.o: %.c
$(MOCK)-generated: ; @echo mock
clean::
	@echo mock
"#,
    );
    let catalog = discover(root.path());
    let tasks: Vec<_> = catalog
        .tasks
        .iter()
        .filter(|task| task.program == "make")
        .collect();
    assert_eq!(
        tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["all", "build", "clean", "test"]
    );
    assert_eq!(tasks[0].source, "Makefile");
    assert_eq!(tasks[0].args, ["-f", "Makefile", "--", "all"]);
    assert!(!root.path().join("should-not-exist").exists());
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn make_respects_file_precedence_and_binds_the_selected_file() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "Makefile", "fallback:\n\t@echo mock\n");
    write(root.path(), "GNUmakefile", "preferred:\n\t@echo mock\n");
    let catalog = discover(root.path());
    assert_eq!(catalog.tasks.len(), 1);
    assert_eq!(catalog.tasks[0].label, "preferred");
    assert_eq!(
        catalog.tasks[0].args,
        ["-f", "GNUmakefile", "--", "preferred"]
    );
}

#[test]
fn make_skips_conditional_targets_and_honors_a_literal_recipe_prefix() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Makefile",
        ".RECIPEPREFIX := >\nreal:\n>echo 'not-a-target:'\nifeq ($(MOCK),yes)\nconditional:\nelse\nalternative:\nendif\ninclude mock-extra.mk\nlast:\n",
    );
    let catalog = discover(root.path());
    assert_eq!(
        catalog
            .tasks
            .iter()
            .map(|task| task.label.as_str())
            .collect::<Vec<_>>(),
        ["last", "real"]
    );
    assert!(!catalog.warnings.is_empty());
}

#[test]
fn make_metadata_is_recoverable_and_cannot_change_the_selected_manifest() {
    let task = Task {
        label: "mock-test".into(),
        source: "Makefile".into(),
        program: "make".into(),
        args: vec![
            "-f".into(),
            "Makefile".into(),
            "--".into(),
            "mock-test".into(),
        ],
    };
    assert_eq!(Task::from_json(&task.to_json()).unwrap(), task);
    let mut invalid = task.to_json();
    invalid["args"] = serde_json::json!(["-f", "../outside.mk", "--", "mock-test"]);
    assert!(Task::from_json(&invalid).is_err());
}
