use std::{fs, iter::once, path::Path, sync::atomic::AtomicBool};
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

#[test]
fn filename_highlights_follow_name_priority_paths_and_fuzzy_matches() {
    for (path, query, expected) in [
        ("src/mock.rs", " MOCK ", once(4..8).collect()),
        ("mock/mock.rs", "mock", once(5..9).collect()),
        ("src/mock.rs", "src/mo", once(0..6).collect()),
        ("src\\mock.rs", "SRC/MO", once(0..6).collect()),
        ("src/mock.rs", "smrs", vec![0..1, 4..5, 9..11]),
        ("src/mock.rs", "missing", vec![]),
        ("src/mock.rs", " ", vec![]),
        ("合成/测试.rs", "测试", once(7..13).collect()),
        ("İ/Mock.rs", "mock", once(3..7).collect()),
        ("İ.rs", "i\u{307}", once(0..2).collect()),
        ("ΟΣ/mock.rs", "ος", once(0..4).collect()),
    ] {
        let ranges = search::name_matches(Path::new(path), query);
        assert_eq!(ranges, expected, "{path:?}, {query:?}");
        assert!(ranges.iter().all(|range| path.get(range.clone()).is_some()));
    }
}

#[test]
fn text_highlights_cover_every_literal_match_with_original_utf8_offsets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("mock.txt"),
        "😀 synthetic [QUERY] [query]\nK k\n",
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    let results = search::text(root.path(), "[query]", &cancel).unwrap();
    assert_eq!(results.hits.len(), 2);
    for hit in &results.hits {
        assert_eq!(hit.matches, vec![15..22, 23..30]);
    }
    let results = search::text(root.path(), "k", &cancel).unwrap();
    assert_eq!(results.hits[0].matches, vec![0..3, 4..5]);
}

#[test]
fn truncated_text_highlights_keep_the_visible_match_without_highlighting_the_ellipsis() {
    let root = tempfile::tempdir().unwrap();
    let query = "n".repeat(350);
    fs::write(
        root.path().join("mock.txt"),
        format!("{}{query}", "x".repeat(400)),
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    let results = search::text(root.path(), &query, &cancel).unwrap();
    let hit = &results.hits[0];
    assert!(hit.text.starts_with('…'));
    assert_eq!(hit.matches.len(), 1);
    let range = hit.matches[0].clone();
    assert_eq!(range.end, hit.text.len());
    assert_eq!(&hit.text[range.clone()], "n".repeat(range.len()));

    fs::write(
        root.path().join("mock.txt"),
        format!("{}…x", "x".repeat(400)),
    )
    .unwrap();
    let results = search::text(root.path(), "…x", &cancel).unwrap();
    let hit = &results.hits[0];
    assert_eq!(hit.matches.len(), 1);
    assert_ne!(hit.matches[0].start, 0);
    assert_eq!(&hit.text[hit.matches[0].clone()], "…x");
}

#[test]
fn text_highlights_stay_disjoint_when_a_snippet_starts_inside_a_repeated_match() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mock.txt"), "a".repeat(400)).unwrap();
    let results = search::text(root.path(), "aa", &AtomicBool::new(false)).unwrap();
    assert!(results.hits.iter().any(|hit| hit.text.starts_with('…')));
    for hit in results.hits {
        assert!(
            hit.matches
                .windows(2)
                .all(|pair| pair[0].end <= pair[1].start),
            "overlapping ranges: {:?}",
            hit.matches
        );
        assert!(
            hit.matches
                .iter()
                .all(|range| hit.text.get(range.clone()).is_some())
        );
    }
}
