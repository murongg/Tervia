use std::{fs, path::Path};
use tervia::files::{Operation, apply};

#[test]
fn creates_files_and_directories_without_overwriting_existing_entries() {
    let root = tempfile::tempdir().unwrap();
    apply(root.path(), &Operation::CreateDirectory("mock".into())).unwrap();
    apply(
        root.path(),
        &Operation::CreateFile("mock/example.rs".into()),
    )
    .unwrap();
    fs::write(root.path().join("mock/example.rs"), "synthetic draft").unwrap();
    assert!(
        apply(
            root.path(),
            &Operation::CreateFile("mock/example.rs".into())
        )
        .is_err()
    );
    assert!(apply(root.path(), &Operation::CreateDirectory("mock".into())).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("mock/example.rs")).unwrap(),
        "synthetic draft"
    );
}

#[test]
fn moves_directories_and_refuses_existing_targets_or_descendants() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("mock")).unwrap();
    fs::write(root.path().join("mock/example.rs"), "synthetic source").unwrap();
    assert!(
        apply(
            root.path(),
            &Operation::Move {
                from: "mock".into(),
                to: "mock/child".into()
            }
        )
        .is_err()
    );
    fs::write(root.path().join("occupied"), "synthetic target").unwrap();
    assert!(
        apply(
            root.path(),
            &Operation::Move {
                from: "mock/example.rs".into(),
                to: "occupied".into()
            }
        )
        .is_err()
    );
    apply(
        root.path(),
        &Operation::Move {
            from: "mock".into(),
            to: "renamed".into(),
        },
    )
    .unwrap();
    assert!(!root.path().join("mock").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("renamed/example.rs")).unwrap(),
        "synthetic source"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("occupied")).unwrap(),
        "synthetic target"
    );
}

#[test]
fn case_only_renames_preserve_contents_and_use_the_requested_spelling() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Mock.rs"), "synthetic source").unwrap();
    apply(
        root.path(),
        &Operation::Move {
            from: "Mock.rs".into(),
            to: "mock.rs".into(),
        },
    )
    .unwrap();
    let names: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from("mock.rs")]);
    assert_eq!(
        fs::read_to_string(root.path().join("mock.rs")).unwrap(),
        "synthetic source"
    );
}

#[test]
fn git_metadata_is_protected_even_through_case_aliases() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".git/refs")).unwrap();
    for path in [".GIT", ".GIT/refs/mock", ".GiT/refs/mock"] {
        assert!(
            apply(root.path(), &Operation::CreateFile(path.into())).is_err(),
            "{path}"
        );
    }
    fs::write(root.path().join("mock.rs"), "synthetic source").unwrap();
    assert!(
        apply(
            root.path(),
            &Operation::Move {
                from: "mock.rs".into(),
                to: ".GIT/refs/mock".into()
            }
        )
        .is_err()
    );
    assert!(root.path().join("mock.rs").exists());
    assert!(!root.path().join(".git/refs/mock").exists());
}

#[test]
fn copies_nested_and_hidden_files_without_changing_the_source() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("mock/nested")).unwrap();
    fs::write(
        root.path().join("mock/nested/example.rs"),
        "synthetic source",
    )
    .unwrap();
    fs::write(root.path().join("mock/.hidden"), "synthetic hidden").unwrap();
    apply(
        root.path(),
        &Operation::Copy {
            from: "mock".into(),
            to: "duplicate".into(),
        },
    )
    .unwrap();
    assert_eq!(
        fs::read(root.path().join("mock/nested/example.rs")).unwrap(),
        fs::read(root.path().join("duplicate/nested/example.rs")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("duplicate/.hidden")).unwrap(),
        "synthetic hidden"
    );
    assert!(
        apply(
            root.path(),
            &Operation::Copy {
                from: "mock".into(),
                to: "mock/child".into()
            }
        )
        .is_err()
    );
    assert!(
        apply(
            root.path(),
            &Operation::Copy {
                from: "mock".into(),
                to: "duplicate".into()
            }
        )
        .is_err()
    );
}

#[test]
fn rejects_root_parent_absolute_and_git_metadata_paths() {
    let root = tempfile::tempdir().unwrap();
    for path in [
        Path::new(""),
        Path::new("."),
        Path::new("../mock"),
        root.path(),
        Path::new(".git/config"),
        Path::new("nested/.git"),
    ] {
        assert!(
            apply(root.path(), &Operation::CreateFile(path.into())).is_err(),
            "{}",
            path.display()
        );
        assert!(
            apply(root.path(), &Operation::Trash(path.into())).is_err(),
            "{}",
            path.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_the_project_or_partially_copy_a_directory() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("example.rs"), "synthetic outside").unwrap();
    symlink(outside.path(), root.path().join("linked")).unwrap();
    assert!(apply(root.path(), &Operation::CreateFile("linked/new.rs".into())).is_err());
    assert!(
        apply(
            root.path(),
            &Operation::Move {
                from: "linked/example.rs".into(),
                to: "stolen.rs".into()
            }
        )
        .is_err()
    );
    fs::create_dir(root.path().join("mock")).unwrap();
    fs::write(root.path().join("mock/first.rs"), "synthetic first").unwrap();
    symlink(outside.path(), root.path().join("mock/link")).unwrap();
    assert!(
        apply(
            root.path(),
            &Operation::Copy {
                from: "mock".into(),
                to: "copy".into()
            }
        )
        .is_err()
    );
    assert!(!root.path().join("copy").exists());
    apply(
        root.path(),
        &Operation::Move {
            from: "linked".into(),
            to: "renamed-link".into(),
        },
    )
    .unwrap();
    assert!(
        fs::symlink_metadata(root.path().join("renamed-link"))
            .unwrap()
            .is_symlink()
    );
    assert!(outside.path().join("example.rs").exists());
}
