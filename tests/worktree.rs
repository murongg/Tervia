use std::{fs, process::Command};
use tempfile::TempDir;
use tervia::git::{Repository, WorktreeCheckout, WorktreeOperation};

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let fixture = Self(tempfile::tempdir().unwrap());
        fixture.git(&["init", "-b", "main"]);
        fixture.git(&["config", "user.name", "Synthetic Developer"]);
        fixture.git(&["config", "user.email", "developer@example.test"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture.git(&["config", "core.hooksPath", ".synthetic-hooks"]);
        fs::write(fixture.0.path().join("sample.txt"), "synthetic initial\n").unwrap();
        fixture.git(&["add", "."]);
        fixture.git(&["commit", "-m", "Synthetic initial commit"]);
        fixture
    }
    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.0.path())
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
    fn repo(&self) -> Repository {
        Repository::open(self.0.path()).unwrap()
    }
}

#[test]
fn lists_sibling_worktrees_from_a_linked_directory_with_independent_indices() {
    let fixture = Fixture::new();
    let linked = tempfile::tempdir().unwrap();
    let path = linked.path().join("feature space");
    fixture.git(&[
        "worktree",
        "add",
        "-b",
        "feature/synthetic",
        path.to_str().unwrap(),
    ]);
    let repo = Repository::open(&path).unwrap();
    let worktrees = repo.worktrees().unwrap();
    assert_eq!(worktrees.len(), 2);
    assert!(worktrees[0].main);
    assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
    assert_eq!(worktrees[1].branch.as_deref(), Some("feature/synthetic"));
    assert!(!worktrees[1].main);
    assert_eq!(worktrees[1].path, path.canonicalize().unwrap());
    assert_eq!(
        repo.common_directory().unwrap(),
        fixture.repo().common_directory().unwrap()
    );
    fs::write(path.join("sample.txt"), "synthetic linked edit\n").unwrap();
    assert_eq!(repo.status().unwrap().files.len(), 1);
    assert!(fixture.repo().status().unwrap().files.is_empty());
    assert_eq!(repo.snapshot(10).unwrap().worktrees.len(), 2);
}

#[test]
fn worktree_paths_share_the_canonical_repository_identity() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let linked = outside.path().join("linked");
    fixture.git(&[
        "worktree",
        "add",
        "--detach",
        linked.to_str().unwrap(),
        "HEAD",
    ]);
    // Git can report a different spelling from fs::canonicalize (notably Windows \\?\ paths).
    // A synthetic parent component exercises that boundary on every platform.
    let alternate = linked.join("..").join("linked").join(".git");
    fs::write(
        fixture.0.path().join(".git/worktrees/linked/gitdir"),
        format!("{}\n", alternate.to_string_lossy().replace('\\', "/")),
    )
    .unwrap();
    let repository = Repository::open(&linked).unwrap();
    let tree = repository
        .worktrees()
        .unwrap()
        .into_iter()
        .find(|tree| !tree.main)
        .unwrap();
    assert_eq!(tree.path, linked.canonicalize().unwrap());
    assert_eq!(tree.path, repository.root);
}

#[test]
fn captures_detached_locked_and_missing_worktrees() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let detached = outside.path().join("detached");
    fixture.git(&[
        "worktree",
        "add",
        "--detach",
        detached.to_str().unwrap(),
        "HEAD",
    ]);
    fixture.git(&[
        "worktree",
        "lock",
        "--reason",
        "Synthetic portable directory",
        detached.to_str().unwrap(),
    ]);
    let trees = fixture.repo().worktrees().unwrap();
    let tree = trees
        .iter()
        .find(|tree| tree.path == detached.canonicalize().unwrap())
        .unwrap();
    assert!(tree.detached);
    assert!(tree.branch.is_none());
    assert_eq!(tree.locked.as_deref(), Some("Synthetic portable directory"));
    assert!(tree.prunable.is_none());
    fixture.git(&["worktree", "unlock", detached.to_str().unwrap()]);
    fs::remove_dir_all(&detached).unwrap();
    assert!(
        fixture
            .repo()
            .worktrees()
            .unwrap()
            .iter()
            .any(|tree| tree.missing)
    );
    assert!(
        fixture
            .repo()
            .worktrees()
            .unwrap()
            .iter()
            .any(|tree| tree.prunable.is_some())
    );
}

#[cfg(unix)]
#[test]
fn worktree_paths_with_newlines_are_not_split_or_unquoted() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("synthetic\nquoted \"folder\"");
    fixture.git(&[
        "worktree",
        "add",
        "--detach",
        path.to_str().unwrap(),
        "HEAD",
    ]);
    let tree = fixture
        .repo()
        .worktrees()
        .unwrap()
        .into_iter()
        .find(|tree| !tree.main)
        .unwrap();
    assert_eq!(tree.path, path.canonicalize().unwrap());
    assert!(tree.path.exists());
}

#[test]
fn creates_new_existing_and_detached_worktrees_without_switching_the_source() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let repo = fixture.repo();
    let new = outside.path().join("new");
    repo.execute_worktree(WorktreeOperation::Add {
        path: new.clone(),
        checkout: WorktreeCheckout::NewBranch {
            name: "feature/mock".into(),
            start: "HEAD".into(),
        },
    })
    .unwrap();
    assert_eq!(
        Repository::open(&new).unwrap().status().unwrap().branch,
        "feature/mock"
    );
    fixture.git(&["branch", "test/existing"]);
    let existing = outside.path().join("existing");
    repo.execute_worktree(WorktreeOperation::Add {
        path: existing.clone(),
        checkout: WorktreeCheckout::ExistingBranch("test/existing".into()),
    })
    .unwrap();
    assert_eq!(
        Repository::open(&existing)
            .unwrap()
            .status()
            .unwrap()
            .branch,
        "test/existing"
    );
    let detached = outside.path().join("detached");
    repo.execute_worktree(WorktreeOperation::Add {
        path: detached.clone(),
        checkout: WorktreeCheckout::Detached("HEAD".into()),
    })
    .unwrap();
    assert!(
        repo.worktrees()
            .unwrap()
            .iter()
            .any(|tree| tree.path == detached.canonicalize().unwrap() && tree.detached)
    );
    assert_eq!(repo.status().unwrap().branch, "main");
    let occupied = repo.execute_worktree(WorktreeOperation::Add {
        path: outside.path().join("occupied"),
        checkout: WorktreeCheckout::ExistingBranch("main".into()),
    });
    assert!(occupied.is_err());
    assert!(!outside.path().join("occupied").exists());
    assert!(
        repo.execute_worktree(WorktreeOperation::Add {
            path: outside.path().join("bad"),
            checkout: WorktreeCheckout::NewBranch {
                name: "--force".into(),
                start: "HEAD".into()
            }
        })
        .is_err()
    );
    assert!(
        repo.execute_worktree(WorktreeOperation::Add {
            path: outside.path().join("bad-ref"),
            checkout: WorktreeCheckout::Detached("--help".into())
        })
        .is_err()
    );
}

#[test]
fn removal_checks_current_main_lock_dirty_and_stale_target_and_keeps_branches() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("remove");
    let repo = fixture.repo();
    repo.execute_worktree(WorktreeOperation::Add {
        path: path.clone(),
        checkout: WorktreeCheckout::NewBranch {
            name: "feature/remove".into(),
            start: "HEAD".into(),
        },
    })
    .unwrap();
    let tree = repo
        .worktrees()
        .unwrap()
        .into_iter()
        .find(|tree| !tree.main)
        .unwrap();
    let main = repo.worktrees().unwrap()[0].clone();
    assert!(
        repo.execute_worktree(WorktreeOperation::Remove(main))
            .is_err()
    );
    assert!(
        Repository::open(&path)
            .unwrap()
            .execute_worktree(WorktreeOperation::Remove(tree.clone()))
            .is_err()
    );
    fs::write(path.join("untracked.txt"), "Synthetic untracked\n").unwrap();
    assert!(
        repo.execute_worktree(WorktreeOperation::Remove(tree.clone()))
            .is_err()
    );
    assert!(path.join("untracked.txt").exists());
    fs::remove_file(path.join("untracked.txt")).unwrap();
    repo.execute_worktree(WorktreeOperation::Lock {
        tree: tree.clone(),
        reason: "Synthetic lock".into(),
    })
    .unwrap();
    assert!(
        repo.execute_worktree(WorktreeOperation::Remove(tree.clone()))
            .is_err()
    );
    let locked = repo
        .worktrees()
        .unwrap()
        .into_iter()
        .find(|tree| !tree.main)
        .unwrap();
    repo.execute_worktree(WorktreeOperation::Unlock(locked))
        .unwrap();
    let mut stale = tree.clone();
    stale.head = "deadbeef".into();
    assert!(
        repo.execute_worktree(WorktreeOperation::Remove(stale))
            .is_err()
    );
    repo.execute_worktree(WorktreeOperation::Remove(tree))
        .unwrap();
    assert!(!path.exists());
    assert!(
        repo.snapshot(10)
            .unwrap()
            .branches
            .iter()
            .any(|branch| branch.name == "feature/remove")
    );
}

#[test]
fn prune_only_cleans_confirmed_missing_unlocked_metadata() {
    let fixture = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("missing");
    let repo = fixture.repo();
    fixture.git(&[
        "worktree",
        "add",
        "--detach",
        path.to_str().unwrap(),
        "HEAD",
    ]);
    fs::remove_dir_all(&path).unwrap();
    let targets: Vec<_> = repo
        .worktrees()
        .unwrap()
        .into_iter()
        .filter(|tree| tree.prunable.is_some())
        .collect();
    assert_eq!(targets.len(), 1);
    assert!(
        repo.execute_worktree(WorktreeOperation::Prune(Vec::new()))
            .is_err()
    );
    repo.execute_worktree(WorktreeOperation::Prune(targets))
        .unwrap();
    assert_eq!(repo.worktrees().unwrap().len(), 1);
}
