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
