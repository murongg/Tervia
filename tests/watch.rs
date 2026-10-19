use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tervia::files::{Changes, Update, Watch, reconcile};

#[test]
fn external_updates_replace_clean_buffers_but_preserve_newly_typed_drafts() {
    assert_eq!(
        reconcile("old", "old", Ok("new".into())),
        Update::Replace("new".into())
    );
    assert_eq!(
        reconcile("old", "draft", Ok("new".into())),
        Update::Conflict
    );
    assert_eq!(
        reconcile("old", "draft", Ok("old".into())),
        Update::Unchanged
    );
    assert_eq!(
        reconcile("old", "draft", Ok("draft".into())),
        Update::Replace("draft".into())
    );
    assert_eq!(
        reconcile("old", "draft", Err("Synthetic missing file".into())),
        Update::Unavailable("Synthetic missing file".into())
    );
}

#[test]
fn native_changes_coalesce_paths_and_ignore_access_and_git_metadata() {
    use notify::{
        Event, EventKind,
        event::{AccessKind, ModifyKind},
    };
    let root = std::path::Path::new("/synthetic/root");
    let mut changes = Changes::default();
    changes.record(
        root,
        Ok(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(root.join("src/mock.rs"))),
    );
    changes.record(
        root,
        Ok(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(root.join("src/mock.rs"))),
    );
    changes.record(
        root,
        Ok(Event::new(EventKind::Access(AccessKind::Any)).add_path(root.join("src/mock.rs"))),
    );
    changes.record(
        root,
        Ok(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(root.join(".git/index"))),
    );
    assert_eq!(
        changes.paths.into_iter().collect::<Vec<_>>(),
        vec![PathBuf::from("src/mock.rs")]
    );
}

#[test]
fn renamed_directories_invalidate_their_old_watch_registration() {
    use notify::{
        Event, EventKind,
        event::{ModifyKind, RenameMode},
    };
    let root = std::path::Path::new("/synthetic/root");
    let mut changes = Changes::default();
    changes.record(
        root,
        Ok(
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(root.join("old"))
                .add_path(root.join("new")),
        ),
    );
    assert!(changes.removed.contains(std::path::Path::new("old")));
    assert!(changes.paths.contains(std::path::Path::new("new")));
}

#[test]
fn native_watcher_detects_real_synthetic_file_changes() {
    let root = std::env::temp_dir().join(format!("tervia-watch-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let watcher = Watch::new(&root).unwrap();
    fs::write(root.join("mock.txt"), "Synthetic external change").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = false;
    while Instant::now() < deadline {
        if watcher
            .drain()
            .paths
            .contains(std::path::Path::new("mock.txt"))
        {
            seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(watcher);
    fs::remove_dir_all(root).unwrap();
    assert!(seen, "The platform watcher must deliver the external write");
}

#[test]
fn precise_changes_include_directory_descendants_and_force_a_rescan_on_errors() {
    let mut changes = Changes::default();
    changes.paths.insert("src/mock.rs".into());
    assert!(changes.affects("src/mock.rs".as_ref()));
    assert!(!changes.affects("src/other.rs".as_ref()));
    changes.paths.insert("renamed".into());
    assert!(changes.affects("renamed/nested/mock.rs".as_ref()));
    assert!(!changes.affects("renamed-other/mock.rs".as_ref()));
    changes.error = Some("Synthetic overflow".into());
    assert!(changes.affects("unrelated/mock.rs".as_ref()));
}

#[test]
fn precise_changes_resolve_internal_aliases_and_recheck_unavailable_paths() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/mock.txt"), "synthetic source").unwrap();
    fs::write(root.join("unrelated.txt"), "synthetic other").unwrap();
    let changes = Changes {
        paths: [PathBuf::from("src/mock.txt")].into(),
        ..Default::default()
    };
    assert!(changes.affects_file(&root, "src/mock.txt".as_ref()));
    assert!(!changes.affects_file(&root, "unrelated.txt".as_ref()));
    assert!(changes.affects_file(&root, "missing.txt".as_ref()));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("src"), root.join("alias")).unwrap();
        std::os::unix::fs::symlink(root.join("src/mock.txt"), root.join("alias.txt")).unwrap();
        assert!(changes.affects_file(&root, "alias/mock.txt".as_ref()));
        assert!(changes.affects_file(&root, "alias.txt".as_ref()));
        fs::remove_file(root.join("src/mock.txt")).unwrap();
        assert!(changes.affects_file(&root, "alias.txt".as_ref()));
    }
}
