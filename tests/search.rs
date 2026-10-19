use std::{fs, path::Path, sync::atomic::AtomicBool};
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
    let mock_path = Path::new("src").join("mock.rs");
    fs::create_dir(root.path().join("src")).unwrap();
    fs::create_dir(root.path().join("node_modules")).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(
        root.path().join(&mock_path),
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
    assert!(found.hits.iter().all(|hit| hit.path == mock_path));
    let index = search::index(root.path(), &cancel).unwrap();
    assert!(
        !index
            .paths
            .iter()
            .any(|path| path == Path::new("ignored.txt"))
    );
    assert_eq!(search::find(&index.paths, "smrs")[0], mock_path);
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

#[test]
fn prepared_file_names_preserve_ranking_unicode_and_the_first_500_results() {
    use std::path::PathBuf;
    let mut paths: Vec<PathBuf> = (0..1200)
        .rev()
        .map(|index| PathBuf::from(format!("src/mock_{index:04}.rs")))
        .collect();
    paths.extend(["z/mock.rs", "mock.rs", "a-MOCK.rs", "合成/测试.rs"].map(PathBuf::from));
    let names = search::Names::new(search::Index {
        paths: paths.clone(),
        limited: false,
    });
    let found = names.find("mock");
    assert_eq!(found.len(), 500);
    assert_eq!(
        &found[..3],
        &[
            PathBuf::from("mock.rs"),
            PathBuf::from("z/mock.rs"),
            PathBuf::from("src/mock_0000.rs")
        ]
    );
    assert_eq!(found[499], PathBuf::from("src/mock_0497.rs"));
    assert_eq!(names.find("测试"), vec![PathBuf::from("合成/测试.rs")]);
    assert_eq!(
        names.find("s/m_0000"),
        vec![PathBuf::from("src/mock_0000.rs")]
    );
    for query in ["", "mock", "MOCK", "src\\mock", "测试", "missing"] {
        assert_eq!(names.find(query), search::find(&paths, query));
    }
}

#[test]
fn text_search_prefilter_keeps_literal_multiline_utf16_and_long_line_behavior() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("mock.txt"),
        "irrelevant\n😀 synthetic needle needle\r\nlast\n",
    )
    .unwrap();
    let results = search::text(root.path(), "needle", &AtomicBool::new(false)).unwrap();
    assert_eq!(
        results
            .hits
            .iter()
            .map(|hit| (hit.line, hit.column))
            .collect::<Vec<_>>(),
        [(2, 13), (2, 20)]
    );
    assert!(
        search::text(root.path(), "needle\nlast", &AtomicBool::new(false))
            .unwrap()
            .hits
            .is_empty()
    );
}
