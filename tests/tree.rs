use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use tervia::files::{Entry, EntryKind, Row, Tree, list};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tervia-tree-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn entry(path: &str, kind: EntryKind) -> Entry {
    Entry {
        path: path.into(),
        kind,
    }
}
fn install(tree: &mut Tree, path: &str, entries: Vec<Entry>) {
    let token = tree.begin(path.into(), false).unwrap();
    assert!(tree.finish(std::path::Path::new(path), token, Ok(entries)));
}

#[test]
fn lists_only_one_directory_with_folders_first_and_keeps_dotfiles() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.0.join("src/nested")).unwrap();
    fs::create_dir(fixture.0.join(".git")).unwrap();
    fs::create_dir(fixture.0.join("node_modules")).unwrap();
    fs::write(fixture.0.join("README.md"), "synthetic").unwrap();
    fs::write(fixture.0.join(".gitignore"), "synthetic").unwrap();
    assert_eq!(
        list(&fixture.0, std::path::Path::new("")).unwrap(),
        vec![
            entry("node_modules", EntryKind::Directory),
            entry("src", EntryKind::Directory),
            entry(".gitignore", EntryKind::File),
            entry("README.md", EntryKind::File),
        ]
    );
    assert_eq!(
        list(&fixture.0, std::path::Path::new("src")).unwrap(),
        vec![entry("src/nested", EntryKind::Directory)]
    );
    assert!(list(&fixture.0, std::path::Path::new("../outside")).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_are_visible_but_cannot_traverse_outside_the_project() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    std::os::unix::fs::symlink(&outside.0, fixture.0.join("linked")).unwrap();
    assert_eq!(
        list(&fixture.0, std::path::Path::new("")).unwrap(),
        vec![entry("linked", EntryKind::Symlink)]
    );
    assert!(list(&fixture.0, std::path::Path::new("linked")).is_err());
}

#[test]
fn folders_load_lazily_and_keep_nested_expansion_when_collapsed() {
    let mut tree = Tree::default();
    install(
        &mut tree,
        "",
        vec![
            entry("src", EntryKind::Directory),
            entry("README.md", EntryKind::File),
        ],
    );
    assert_eq!(tree.rows().len(), 2);
    assert!(tree.toggle("src".as_ref()));
    assert!(matches!(&tree.rows()[1], Row::Loading { depth: 1, .. }));
    install(
        &mut tree,
        "src",
        vec![entry("src/nested", EntryKind::Directory)],
    );
    assert!(tree.toggle("src/nested".as_ref()));
    install(
        &mut tree,
        "src/nested",
        vec![entry("src/nested/mock.rs", EntryKind::File)],
    );
    assert_eq!(tree.rows().len(), 4);
    assert!(!tree.toggle("src".as_ref()));
    assert_eq!(tree.rows().len(), 2);
    assert!(tree.toggle("src".as_ref()));
    assert_eq!(tree.rows().len(), 4);
    assert!(tree.begin("src".into(), false).is_none());
}

#[test]
fn refresh_rejects_old_results_and_removes_deleted_subtrees() {
    let mut tree = Tree::default();
    let old = tree.begin("".into(), false).unwrap();
    assert!(tree.begin("".into(), false).is_none());
    let current = tree.begin("".into(), true).unwrap();
    assert!(!tree.finish("".as_ref(), old, Ok(vec![entry("old", EntryKind::File)])));
    assert!(tree.finish(
        "".as_ref(),
        current,
        Ok(vec![entry("src", EntryKind::Directory)])
    ));
    tree.toggle("src".as_ref());
    let child = tree.begin("src".into(), false).unwrap();
    let root = tree.begin("".into(), true).unwrap();
    assert!(tree.finish("".as_ref(), root, Ok(vec![entry("new", EntryKind::File)])));
    assert!(!tree.finish(
        "src".as_ref(),
        child,
        Ok(vec![entry("src/late.rs", EntryKind::File)])
    ));
    assert_eq!(tree.rows().len(), 1);
    assert!(!tree.is_expanded("src".as_ref()));
}

#[test]
fn load_failures_can_be_retried_and_project_trees_are_independent() {
    let mut alpha = Tree::default();
    let mut beta = Tree::default();
    let token = alpha.begin("".into(), false).unwrap();
    assert!(alpha.finish("".as_ref(), token, Err("Synthetic failure".into())));
    assert!(
        matches!(&alpha.rows()[0], Row::Error { message, .. } if message == "Synthetic failure")
    );
    install(&mut alpha, "", vec![entry("alpha", EntryKind::Directory)]);
    install(&mut beta, "", vec![entry("beta", EntryKind::Directory)]);
    alpha.toggle("alpha".as_ref());
    assert_eq!(beta.rows().len(), 1);
    assert_eq!(alpha.expanded(), vec![PathBuf::new(), "alpha".into()]);
}

#[test]
fn refresh_invalidates_collapsed_contents_and_reopens_them_lazily() {
    let mut tree = Tree::default();
    install(&mut tree, "", vec![entry("src", EntryKind::Directory)]);
    tree.toggle("src".as_ref());
    install(&mut tree, "src", vec![entry("src/old.rs", EntryKind::File)]);
    tree.toggle("src".as_ref());
    let requests = tree.refresh();
    assert_eq!(
        requests.len(),
        1,
        "A collapsed directory is not read eagerly"
    );
    assert_eq!(requests[0].0, PathBuf::new());
    tree.finish(
        "".as_ref(),
        requests[0].1,
        Ok(vec![entry("src", EntryKind::Directory)]),
    );
    assert!(tree.toggle("src".as_ref()));
    let token = tree
        .begin("src".into(), false)
        .expect("Reopening after refresh must reread the stale directory");
    tree.finish(
        "src".as_ref(),
        token,
        Ok(vec![entry("src/new.rs", EntryKind::File)]),
    );
    assert!(
        matches!(&tree.rows()[1], Row::Entry { entry, .. } if entry.path == std::path::Path::new("src/new.rs"))
    );
}
#[test]
fn directory_resolution_rejects_a_folder_replaced_by_an_external_symlink() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("mock-child")).unwrap();
    assert_eq!(
        tervia::files::directory(root.path(), std::path::Path::new("mock-child")).unwrap(),
        root.path().join("mock-child").canonicalize().unwrap()
    );
    assert!(tervia::files::directory(root.path(), std::path::Path::new("../outside")).is_err());
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::fs::remove_dir(root.path().join("mock-child")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("mock-child")).unwrap();
        assert!(tervia::files::directory(root.path(), std::path::Path::new("mock-child")).is_err());
    }
}

fn visible(tree: &Tree) -> Vec<(PathBuf, usize)> {
    tree.rows()
        .iter()
        .filter_map(|row| match row {
            Row::Entry { entry, depth } => Some((entry.path.clone(), *depth)),
            _ => None,
        })
        .collect()
}

#[test]
fn nesting_is_optional_and_groups_related_files_without_changing_paths() {
    let mut tree = Tree::default();
    let entries = [
        ".dockerignore",
        ".env",
        ".env.example",
        ".env.production.local",
        ".npmrc",
        "Dockerfile",
        "Dockerfile.dev",
        "docker-compose.yml",
        "package.json",
        "pnpm-lock.yaml",
    ]
    .map(|path| entry(path, EntryKind::File))
    .to_vec();
    install(&mut tree, "", entries.clone());
    assert_eq!(tree.rows().len(), entries.len());
    tree.set_nesting(true);
    assert_eq!(
        visible(&tree),
        vec![
            (".env".into(), 0),
            ("Dockerfile".into(), 0),
            ("package.json".into(), 0)
        ]
    );
    assert!(tree.has_nested(".env".as_ref()));
    assert!(tree.toggle_nested(".env".as_ref()));
    assert_eq!(
        &visible(&tree)[..3],
        &[
            (".env".into(), 0),
            (".env.example".into(), 1),
            (".env.production.local".into(), 1)
        ]
    );
    assert!(tree.toggle_nested("Dockerfile".as_ref()));
    assert!(tree.toggle_nested("package.json".as_ref()));
    let rows = visible(&tree);
    assert_eq!(rows.len(), entries.len());
    for path in [
        ".dockerignore",
        "Dockerfile.dev",
        "docker-compose.yml",
        ".npmrc",
        "pnpm-lock.yaml",
    ] {
        assert!(rows.contains(&(path.into(), 1)));
    }
    tree.set_nesting(false);
    assert_eq!(
        visible(&tree),
        entries
            .iter()
            .map(|entry| (entry.path.clone(), 0))
            .collect::<Vec<_>>()
    );
    assert!(!tree.has_nested(".env".as_ref()));
    tree.set_nesting(true);
    assert!(
        tree.is_expanded(".env".as_ref()),
        "Switching settings preserves group expansion"
    );
    assert!(!tree.toggle_nested(".env".as_ref()));
    assert!(!tree.is_expanded(".env".as_ref()));
}

#[test]
fn nested_files_never_become_directories_or_scan_requests() {
    let mut tree = Tree::default();
    tree.set_nesting(true);
    install(
        &mut tree,
        "",
        vec![
            entry("Cargo.lock", EntryKind::File),
            entry("Cargo.toml", EntryKind::File),
        ],
    );
    assert!(tree.toggle_nested("Cargo.toml".as_ref()));
    assert!(!tree.toggle("Cargo.toml".as_ref()));
    assert!(tree.begin("Cargo.toml".into(), false).is_none());
    assert_eq!(tree.directories(), vec![PathBuf::new()]);
    assert_eq!(tree.expanded(), vec![PathBuf::new()]);
    assert_eq!(
        tree.refresh()
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::new()]
    );
}

#[test]
fn nesting_requires_a_regular_parent_in_the_same_directory() {
    let mut tree = Tree::default();
    tree.set_nesting(true);
    install(
        &mut tree,
        "",
        vec![
            entry("src", EntryKind::Directory),
            entry(".env", EntryKind::Symlink),
            entry(".env.local", EntryKind::File),
            entry("Cargo.lock", EntryKind::File),
            entry("Dockerfile", EntryKind::Directory),
            entry("Dockerfile.dev", EntryKind::File),
        ],
    );
    assert_eq!(visible(&tree).len(), 6);
    assert!(!tree.has_nested(".env".as_ref()));
    assert!(!tree.toggle_nested("Cargo.lock".as_ref()));
    tree.toggle("src".as_ref());
    install(
        &mut tree,
        "src",
        vec![
            entry("src/.env", EntryKind::File),
            entry("src/.env.local", EntryKind::File),
        ],
    );
    assert!(tree.toggle_nested("src/.env".as_ref()));
    assert!(visible(&tree).contains(&("src/.env.local".into(), 2)));
    assert!(visible(&tree).contains(&(".env.local".into(), 0)));
    tree.toggle("src".as_ref());
    tree.toggle("src".as_ref());
    assert!(visible(&tree).contains(&("src/.env.local".into(), 2)));
}

#[test]
fn nesting_flattens_companions_and_never_hides_files_in_cycles() {
    let mut tree = Tree::default();
    tree.set_nesting(true);
    install(
        &mut tree,
        "",
        [
            "mock.js",
            "mock.js.map",
            "mock.test.ts",
            "mock.ts",
            "mock.ts.map",
            "mock.tsx",
        ]
        .map(|path| entry(path, EntryKind::File))
        .to_vec(),
    );
    assert!(tree.toggle_nested("mock.ts".as_ref()));
    let rows = visible(&tree);
    assert_eq!(
        rows.len(),
        6,
        "Overlapping source rules must keep every file reachable exactly once"
    );
    for path in ["mock.js", "mock.js.map", "mock.test.ts", "mock.ts.map"] {
        assert!(rows.contains(&(path.into(), 1)), "{path}: {rows:?}");
    }
    assert!(rows.contains(&("mock.tsx".into(), 0)));
}

#[test]
fn refreshing_nesting_releases_orphans_and_rebuilds_groups() {
    let mut tree = Tree::default();
    tree.set_nesting(true);
    install(
        &mut tree,
        "",
        vec![
            entry(".env", EntryKind::File),
            entry(".env.local", EntryKind::File),
        ],
    );
    tree.toggle_nested(".env".as_ref());
    let token = tree.begin("".into(), true).unwrap();
    tree.finish(
        "".as_ref(),
        token,
        Ok(vec![entry(".env.local", EntryKind::File)]),
    );
    assert_eq!(visible(&tree), vec![(".env.local".into(), 0)]);
    assert!(!tree.has_nested(".env".as_ref()));
    let token = tree.begin("".into(), true).unwrap();
    tree.finish(
        "".as_ref(),
        token,
        Ok(vec![
            entry(".env", EntryKind::File),
            entry(".env.staging", EntryKind::File),
        ]),
    );
    assert!(tree.has_nested(".env".as_ref()));
    assert!(
        tree.toggle_nested(".env".as_ref()),
        "A deleted group loses its old expansion state"
    );
    assert_eq!(
        visible(&tree),
        vec![(".env".into(), 0), (".env.staging".into(), 1)]
    );
}
