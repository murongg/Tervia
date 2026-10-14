use std::{fs, sync::atomic::AtomicBool};
use tervia::files::search;

#[test]
fn long_line_results_show_the_match_and_explicitly_opened_dependency_roots_are_searchable() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("build");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join("mock.txt"),
        format!("{}synthetic needle", "x".repeat(400)),
    )
    .unwrap();
    let found = search::text(&project, "needle", &AtomicBool::new(false)).unwrap();
    assert_eq!(found.hits.len(), 1);
    assert!(found.hits[0].text.contains("needle"));
}

#[test]
fn search_respects_ignores_binary_files_unicode_and_literal_queries() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::create_dir(root.path().join("node_modules")).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(
        root.path().join("src/mock.rs"),
        "Synthetic [query]\n中文 Synthetic [QUERY]\n",
    )
    .unwrap();
    fs::write(root.path().join("ignored.txt"), "[query]").unwrap();
    fs::write(root.path().join("binary.bin"), b"[query]\0").unwrap();
    fs::write(
        root.path().join("mock.svg"),
        "<svg><!-- Synthetic [query] --></svg>",
    )
    .unwrap();
    fs::write(root.path().join("node_modules/mock.js"), "[query]").unwrap();
    let cancel = AtomicBool::new(false);
    let found = search::text(root.path(), "[query]", &cancel).unwrap();
    assert_eq!(found.hits.len(), 2);
    assert_eq!(found.hits[1].line, 2);
    assert_eq!(found.hits[1].column, 13);
    assert!(
        found
            .hits
            .iter()
            .all(|hit| hit.path.to_str() == Some("src/mock.rs"))
    );
    let index = search::index(root.path(), &cancel).unwrap();
    assert!(
        !index
            .paths
            .iter()
            .any(|path| path.to_str() == Some("ignored.txt"))
    );
    assert_eq!(
        search::find(&index.paths, "smrs")[0].to_str(),
        Some("src/mock.rs")
    );
}

#[test]
fn search_cancels_and_does_not_follow_symlinks_outside_the_project() {
    let root = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(true);
    assert!(
        search::index(root.path(), &cancel)
            .unwrap()
            .paths
            .is_empty()
    );
    assert!(
        search::text(root.path(), "synthetic", &cancel)
            .unwrap()
            .hits
            .is_empty()
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("mock.txt"), "synthetic").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(
            search::text(root.path(), "synthetic", &cancel)
                .unwrap()
                .hits
                .is_empty()
        );
    }
}
