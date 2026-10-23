//! Run with `cargo bench --bench latency`. Fixtures contain synthetic data only.
//! Autosave preparation measures UI work separately; disk completion is reported too.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tervia::{
    files::search::{self, Hit, Results, index},
    state::{File, Workspace},
    workspace::Store,
};

fn report(name: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    println!(
        "{name}: median_ms={:.3} p95_ms={:.3} samples={n}",
        samples[n / 2],
        samples[((n as f64 * 0.95).ceil() as usize - 1).min(n - 1)]
    );
}
fn measure(name: &str, count: usize, mut run: impl FnMut()) {
    run();
    let mut samples = Vec::new();
    for _ in 0..count {
        let started = Instant::now();
        run();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    report(name, samples);
}

fn legacy_find(paths: &[PathBuf], query: &str) -> Vec<PathBuf> {
    let query = query.trim().replace('\\', "/").to_lowercase();
    let mut found: Vec<_> = paths
        .iter()
        .filter_map(|path| {
            let full = path.to_string_lossy().replace('\\', "/").to_lowercase();
            let name = path.file_name()?.to_string_lossy().to_lowercase();
            let score = if name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else if full.contains(&query) {
                2
            } else {
                let mut chars = full.chars();
                if !query.chars().all(|wanted| chars.any(|ch| ch == wanted)) {
                    return None;
                }
                3
            };
            Some((score, full.len(), path.clone()))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .take(500)
        .map(|(_, _, path)| path)
        .collect()
}

fn legacy_text(root: &Path, query: &str, cancel: &AtomicBool) -> Result<Results, String> {
    let mut results = Results::default();
    if query.trim().is_empty() {
        return Ok(results);
    }
    let matcher = regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
        .map_err(|error| error.to_string())?;
    let index = index(root, cancel)?;
    results.limited = index.limited;
    let started = Instant::now();
    let mut read_bytes = 0;
    for path in index.paths {
        if tervia::files::image_mime(&path).is_some() {
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if started.elapsed() > Duration::from_secs(3) || read_bytes > 128 * 1024 * 1024 {
            results.limited = true;
            break;
        }
        let Ok(bytes) = tervia::files::read(root, &path) else {
            results.limited = true;
            continue;
        };
        read_bytes += bytes.len();
        if bytes.contains(&0) {
            continue;
        }
        let Ok(content) = std::str::from_utf8(&bytes) else {
            continue;
        };
        for (line, text) in content.lines().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(results);
            }
            let mut column = 0;
            let mut previous = 0;
            for found in matcher.find_iter(text) {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(results);
                }
                if results.hits.len() == 1000 || started.elapsed() > Duration::from_secs(3) {
                    results.limited = true;
                    return Ok(results);
                }
                column += text[previous..found.start()].encode_utf16().count();
                previous = found.start();
                let start = text[..found.start()]
                    .char_indices()
                    .rev()
                    .nth(80)
                    .map_or(0, |(offset, _)| offset);
                let snippet: String = text[start..].chars().take(300).collect();
                // LSP/editor columns are UTF-16 positions, rather than UTF-8 byte offsets.
                results.hits.push(Hit {
                    path: path.clone(),
                    line: line + 1,
                    column,
                    text: if start == 0 {
                        snippet
                    } else {
                        format!("…{snippet}")
                    },
                    matches: Vec::new(),
                });
            }
        }
    }
    Ok(results)
}

fn main() {
    let paths: Vec<PathBuf> = (0..50_000)
        .map(|index| {
            PathBuf::from(format!(
                "src/module_{:04}/synthetic_file_{index:05}.rs",
                index % 200
            ))
        })
        .collect();
    let names = search::Names::new(search::Index {
        paths: paths.clone(),
        limited: false,
    });
    for (label, query) in [("broad", "file"), ("absent", "zzzz")] {
        assert_eq!(legacy_find(&paths, query), names.find(query));
        measure(&format!("file_find_{label}_before"), 30, || {
            std::hint::black_box(legacy_find(&paths, query));
        });
        measure(&format!("file_find_{label}_after"), 30, || {
            std::hint::black_box(names.find(query));
        });
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("text");
    std::fs::create_dir(&root).unwrap();
    let body = "synthetic source abcdefghijklmnopqrstuvwxyz\n".repeat(96);
    for index in 0..2000 {
        std::fs::write(root.join(format!("mock_{index:04}.txt")), &body).unwrap();
    }
    let cancel = AtomicBool::new(false);
    measure("text_2000_files_absent_before", 10, || {
        assert!(
            legacy_text(&root, "not-in-fixture", &cancel)
                .unwrap()
                .hits
                .is_empty()
        );
    });
    measure("text_2000_files_absent_after", 10, || {
        assert!(
            search::text(&root, "not-in-fixture", &cancel)
                .unwrap()
                .hits
                .is_empty()
        );
    });
    let mut store = Store::new(temporary.path().join("workspace.json"));
    let mut state: Workspace = store.load().unwrap();
    state.open_directory(root);
    state.projects[0].files.clear();
    state.projects[0].open_files.clear();
    for index in 0..20 {
        state.projects[0].files.push(File {
            path: format!("mock_{index}.txt"),
            disk_path: format!("mock_{index}.txt").into(),
            language: "text".into(),
            saved: "s".repeat(256 * 1024),
            draft: "d".repeat(256 * 1024),
        });
        state.projects[0].open_files.push(index);
    }
    store.save(&state).unwrap();
    measure("autosave_sync_before", 12, || {
        state.sidebar_expanded = !state.sidebar_expanded;
        store.save(&state).unwrap();
    });
    let mut samples = Vec::new();
    for sample in 0..13 {
        state.sidebar_expanded = !state.sidebar_expanded;
        let started = Instant::now();
        let pending = store.prepare(&state);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        // Native UI runs this completion on its background executor.
        pending.save().unwrap();
        if sample > 0 {
            samples.push(elapsed);
        }
    }
    report("autosave_ui_prepare_after", samples);
    measure("autosave_total_after", 12, || {
        state.sidebar_expanded = !state.sidebar_expanded;
        store.prepare(&state).save().unwrap();
    });
}
