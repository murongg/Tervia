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
