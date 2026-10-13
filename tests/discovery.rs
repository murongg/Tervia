use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use tervia::git::Repository;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tervia-discovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn repo(&self, path: &str) -> PathBuf {
        let path = self.0.join(path);
        fs::create_dir_all(&path).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-b", "main"])
                .arg(&path)
                .output()
                .unwrap()
                .status
                .success()
        );
        path.canonicalize().unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ordinary_workspace_discovers_multiple_nested_repositories() {
    let workspace = Workspace::new();
    let alpha = workspace.repo("alpha");
    let beta = workspace.repo("packages/beta");
    let discovery = Repository::discover(&workspace.0).unwrap();
    assert_eq!(discovery.root, workspace.0.canonicalize().unwrap());
    assert_eq!(discovery.repositories, vec![alpha, beta]);
    assert!(!discovery.limited);
}

#[test]
fn opened_and_discovered_repositories_use_the_same_canonical_root() {
    let workspace = Workspace::new();
    let path = workspace.repo("project");
    let repository = Repository::open(&path).unwrap();
    assert_eq!(repository.root, path);
    let discovery = Repository::discover(&workspace.0).unwrap();
    assert!(discovery.repositories.contains(&repository.root));
}

#[test]
fn nested_repositories_are_kept_and_dependency_trees_are_skipped() {
    let workspace = Workspace::new();
    let root = workspace.repo("");
    let nested = workspace.repo("nested");
    workspace.repo("node_modules/ignored");
    let discovery = Repository::discover(&workspace.0).unwrap();
    assert_eq!(discovery.repositories, vec![root, nested]);
}

#[test]
fn git_directory_files_are_discovered_without_treating_metadata_as_a_project() {
    let workspace = Workspace::new();
    let project = workspace.0.join("linked");
    let metadata = workspace.0.join("metadata");
    assert!(
        Command::new("git")
            .args(["init", "-b", "main", "--separate-git-dir"])
            .arg(metadata)
            .arg(&project)
            .output()
            .unwrap()
            .status
            .success()
    );
    let discovery = Repository::discover(&workspace.0).unwrap();
    assert_eq!(
        discovery.repositories,
        vec![project.canonicalize().unwrap()]
    );
}

#[test]
fn plain_empty_folders_have_no_repositories() {
    let workspace = Workspace::new();
    assert!(
        Repository::discover(&workspace.0)
            .unwrap()
            .repositories
            .is_empty()
    );
}

#[test]
fn discovery_associates_linked_directories_by_common_git_directory() {
    let workspace = Workspace::new();
    let main = workspace.repo("main");
    let command = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&main)
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
    };
    command(&[
        "-c",
        "user.name=Synthetic Developer",
        "-c",
        "user.email=developer@example.test",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=.synthetic-hooks",
        "commit",
        "--allow-empty",
        "-m",
        "Synthetic initial",
    ]);
    let linked = workspace.0.join("linked");
    command(&[
        "worktree",
        "add",
        "-b",
        "feature/mock",
        linked.to_str().unwrap(),
    ]);
    let found = Repository::discover(&workspace.0).unwrap();
    assert_eq!(found.repositories.len(), 2);
    assert_eq!(found.projects(), vec![main.clone()]);
    assert_eq!(
        found.identities[&main],
        found.identities[&linked.canonicalize().unwrap()]
    );
}

#[cfg(unix)]
#[test]
fn discovery_does_not_follow_child_symlinks_outside_the_workspace() {
    let workspace = Workspace::new();
    let outside = Workspace::new();
    outside.repo("");
    std::os::unix::fs::symlink(&outside.0, workspace.0.join("outside")).unwrap();
    assert!(
        Repository::discover(&workspace.0)
            .unwrap()
            .repositories
            .is_empty()
    );
}
