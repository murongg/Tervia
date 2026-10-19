//! Run with `cargo bench --bench refresh`. All data is synthetic and temporary.
//! Baselines reproduce the previous full-scan/full-refresh paths. Timings exclude
//! native watcher delivery and GUI painting; both variants use identical fixtures.
use alacritty_terminal::{
    grid::Dimensions,
    index::{Column, Point},
    term::search::{Match, RegexSearch},
};
use std::{
    collections::BTreeSet,
    hint::black_box,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use tervia::{
    files::{Changes, Entry, EntryKind, Tree},
    git::{Repository, SnapshotCache},
    terminal::{Search, TerminalBuffer},
};

fn measure(name: &str, count: usize, mut run: impl FnMut()) {
    run();
    let mut times = Vec::new();
    for _ in 0..count {
        let start = Instant::now();
        run();
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{name}: median_ms={:.3} p95_ms={:.3} samples={count}",
        times[count / 2],
        times[((count as f64 * 0.95).ceil() as usize - 1).min(count - 1)]
    );
}

fn legacy_find(buffer: &TerminalBuffer, query: &str) -> Vec<Match> {
    let mut regex = RegexSearch::new(&regex::escape(query)).unwrap();
    let term = &buffer.term;
    let mut origin = Point::new(term.topmost_line(), Column(0));
    let end = Point::new(term.bottommost_line(), term.last_column());
    let mut matches = Vec::new();
    while let Some(found) = term.regex_search_right(&mut regex, origin, end) {
        let last = *found.end();
        matches.push(found);
        if last >= end || matches.len() == 5000 {
            break;
        }
        origin = if last.column.0 + 1 == term.columns() {
            Point::new(last.line + 1, Column(0))
        } else {
            Point::new(last.line, last.column + 1)
        };
    }
    matches
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn main() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let mut terminal = TerminalBuffer::new(160, 50);
    for index in 0..10_050 {
        terminal
            .feed(format!("synthetic-row-{index:05} {}\r\n", "abcdefghij".repeat(13)).as_bytes());
    }
    for (name, query) in [("absent", "not-in-buffer"), ("5000_hits", "synthetic-row")] {
        let expected = legacy_find(&terminal, query);
        let mut search = Search::new(query);
        assert_eq!(search.find(&terminal), expected);
        measure(&format!("terminal_{name}_before"), 40, || {
            black_box(legacy_find(&terminal, query));
        });
        measure(&format!("terminal_{name}_after"), 40, || {
            black_box(search.find(&terminal));
        });
    }
    terminal.feed("合成\r\n".as_bytes());
    let mut search = Search::new("not-in-buffer");
    assert_eq!(
        search.find(&terminal),
        legacy_find(&terminal, "not-in-buffer")
    );
    measure("terminal_unicode_fallback_before", 40, || {
        black_box(legacy_find(&terminal, "not-in-buffer"));
    });
    measure("terminal_unicode_fallback_after", 40, || {
        black_box(search.find(&terminal));
    });

    let root = base.join("repository");
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.name", "Synthetic Benchmark"]);
    git(&root, &["config", "user.email", "synthetic@example.test"]);
    git(&root, &["config", "core.hooksPath", ".synthetic-hooks"]);
    std::fs::write(root.join("mock.txt"), "synthetic\n").unwrap();
    git(&root, &["add", "mock.txt"]);
    git(
        &root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "Synthetic commit",
        ],
    );
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.test/synthetic.git",
        ],
    );
    let mut cache = SnapshotCache::default();
    measure("git_unchanged_before", 12, || {
        black_box(
            Repository::probe(&root)
                .unwrap()
                .unwrap()
                .snapshot(100)
                .unwrap(),
        );
    });
    measure("git_unchanged_after", 12, || {
        black_box(
            cache
                .refresh(&Repository::probe(&root).unwrap().unwrap(), 100)
                .unwrap(),
        );
    });
    measure("git_forced_before", 5, || {
        black_box(
            Repository::probe(&root)
                .unwrap()
                .unwrap()
                .snapshot(100)
                .unwrap(),
        );
    });
    measure("git_forced_after", 5, || {
        cache.clear();
        black_box(
            Repository::probe(&root)
                .unwrap()
                .unwrap()
                .snapshot(100)
                .unwrap(),
        );
    });

    let root = base.join("files");
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q"]);
    let mut tree = Tree::default();
    let mut files = Vec::new();
    let mut entries = Vec::new();
    for directory in 0..50 {
        let path = PathBuf::from(format!("dir_{directory:02}"));
        std::fs::create_dir(root.join(&path)).unwrap();
        entries.push(Entry {
            path,
            kind: EntryKind::Directory,
            ignored: false,
        });
    }
    let token = tree.begin("".into(), false).unwrap();
    tree.finish("".as_ref(), token, Ok(entries));
    for directory in 0..50 {
        let path = PathBuf::from(format!("dir_{directory:02}"));
        tree.toggle(&path);
        let mut entries = Vec::new();
        for file in 0..10 {
            let file = path.join(format!("mock_{file:02}.txt"));
            std::fs::write(root.join(&file), "synthetic\n".repeat(400)).unwrap();
            files.push(file.clone());
            entries.push(Entry {
                path: file,
                kind: EntryKind::File,
                ignored: false,
            });
        }
        let token = tree.begin(path.clone(), false).unwrap();
        tree.finish(&path, token, Ok(entries));
    }
    let changes = Changes {
        paths: BTreeSet::from([PathBuf::from("dir_00/mock_00.txt")]),
        ..Default::default()
    };
    for optimized in [false, true] {
        measure(
            if optimized {
                "watch_single_file_after"
            } else {
                "watch_single_file_before"
            },
            5,
            || {
                let requests = if optimized {
                    tree.refresh_paths(&changes.paths)
                } else {
                    tree.refresh()
                };
                let mut reads = 0;
                for (path, token) in requests {
                    tree.finish(&path, token, tervia::files::list(&root, &path));
                }
                for path in &files {
                    if !optimized || changes.affects_file(&root, path) {
                        black_box(tervia::files::text(&root, path).unwrap());
                        reads += 1;
                    }
                }
                assert_eq!(reads, if optimized { 1 } else { 500 });
            },
        );
    }
}
