use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Index {
    pub paths: Vec<PathBuf>,
    pub limited: bool,
}

#[derive(Clone)]
pub struct Hit {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

#[derive(Default)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub limited: bool,
}

fn walk(root: &Path) -> ignore::Walk {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || !matches!(
                    entry.file_name().to_str(),
                    Some(
                        ".git" | "node_modules" | "target" | "build" | "dist" | ".venv" | ".cache"
                    )
                )
        })
        .build()
}

pub fn index(root: &Path, cancel: &AtomicBool) -> Result<Index, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut index = Index::default();
    for entry in walk(&root) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if index.paths.len() >= 50_000 || started.elapsed() > Duration::from_secs(3) {
            index.limited = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                index.limited = true;
                continue;
            }
        };
        if entry.file_type().is_some_and(|kind| kind.is_file()) {
            index.paths.push(
                entry
                    .path()
                    .strip_prefix(&root)
                    .map_err(|error| error.to_string())?
                    .into(),
            );
        }
    }
    index.paths.sort();
    Ok(index)
}

struct Name {
    path: PathBuf,
    full: String,
    name: String,
}

/// Normalized names belong to one immutable directory index, shared across queries.
pub struct Names {
    names: Vec<Name>,
    pub limited: bool,
}

impl Names {
    pub fn new(index: Index) -> Self {
        Self {
            limited: index.limited,
            names: index
                .paths
                .into_iter()
                .filter_map(|path| {
                    let name = path.file_name()?.to_string_lossy().to_lowercase();
                    let full = path.to_string_lossy().replace('\\', "/").to_lowercase();
                    Some(Name { path, full, name })
                })
                .collect(),
        }
    }

    pub fn find(&self, query: &str) -> Vec<PathBuf> {
        let query = normalize(query);
        top(self.names.iter().filter_map(|entry| {
            score(&entry.full, &entry.name, &query)
                .map(|score| (score, entry.full.len(), &entry.path))
        }))
    }
}

fn normalize(query: &str) -> String {
    query.trim().replace('\\', "/").to_lowercase()
}

fn score(full: &str, name: &str, query: &str) -> Option<u8> {
    Some(if name.starts_with(query) {
        0
    } else if name.contains(query) {
        1
    } else if full.contains(query) {
        2
    } else {
        let mut chars = full.chars();
        if !query.chars().all(|wanted| chars.any(|ch| ch == wanted)) {
            return None;
        }
        3
    })
}

fn top<'a>(matches: impl Iterator<Item = (u8, usize, &'a PathBuf)>) -> Vec<PathBuf> {
    let mut found: Vec<_> = matches.collect();
    // Preserve the complete ranking, but only sort and clone the visible prefix.
    if found.len() > 500 {
        found.select_nth_unstable(500);
        found.truncate(500);
    }
    found.sort_unstable();
    found.into_iter().map(|(_, _, path)| path.clone()).collect()
}

pub fn find(paths: &[PathBuf], query: &str) -> Vec<PathBuf> {
    let query = normalize(query);
    top(paths.iter().filter_map(|path| {
        let full = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        score(&full, &name, &query).map(|score| (score, full.len(), path))
    }))
}

pub fn text(root: &Path, query: &str, cancel: &AtomicBool) -> Result<Results, String> {
    let mut results = Results::default();
    if query.trim().is_empty() {
        return Ok(results);
    }
    let matcher = regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
        .map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let index = index(&root, cancel)?;
    results.limited = index.limited;
    let started = Instant::now();
    let mut read_bytes = 0;
    for path in index.paths {
        if super::image_mime(&path).is_some() {
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if started.elapsed() > Duration::from_secs(3) || read_bytes > 128 * 1024 * 1024 {
            results.limited = true;
            break;
        }
        let Ok(bytes) = super::read_at(&root, &path) else {
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
        // Most files do not match. Avoid starting a regex search on every line
        // while retaining line-local matching and UTF-16 columns for actual hits.
        if !matcher.is_match(content) {
            continue;
        }
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
                });
            }
        }
    }
    Ok(results)
}
