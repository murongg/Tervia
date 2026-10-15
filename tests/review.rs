use std::{fs, process::Command};
use tervia::git::{Repository, ReviewKey, Reviews, Side};

#[test]
fn review_marks_only_survive_matching_versions() {
    let mut reviews = Reviews::default();
    let key = ReviewKey::new("mock.rs".into(), Side::Worktree);
    reviews.toggle(key.clone(), [1; 32]);
    assert!(reviews.contains(&key));
    reviews.reconcile(&std::collections::BTreeMap::from([(key.clone(), [2; 32])]));
    assert!(!reviews.contains(&key));
    reviews.toggle(key.clone(), [2; 32]);
    reviews.toggle(key.clone(), [2; 32]);
    assert!(!reviews.contains(&key));
}

#[test]
fn checkpoints_include_existing_dirty_and_untracked_files_without_changing_the_index() {
    let root = tempfile::tempdir().unwrap();
    let repo = Repository::initialize(root.path()).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(root.path())
            .args([
                "-c",
                "user.name=Synthetic",
                "-c",
                "user.email=synthetic@example.test",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    fs::write(root.path().join("mock.rs"), "base\n").unwrap();
    git(&["add", "mock.rs"]);
    git(&["commit", "-m", "synthetic base"]);
    fs::write(root.path().join("mock.rs"), "already dirty\n").unwrap();
    fs::write(root.path().join("new.rs"), "already untracked\n").unwrap();
    let before = git(&["ls-files", "--stage"]);
    let checkpoint = repo.checkpoint().unwrap();
    assert!(repo.compare_checkpoint(&checkpoint).unwrap().is_empty());
    fs::write(root.path().join("mock.rs"), "new round\n").unwrap();
    fs::write(root.path().join("new.rs"), "new untracked round\n").unwrap();
    let patch = repo.compare_checkpoint(&checkpoint).unwrap();
    assert!(patch.contains("-already dirty"));
    assert!(patch.contains("+new round"));
    assert!(patch.contains("-already untracked"));
    assert_eq!(git(&["ls-files", "--stage"]), before);
    assert!(repo.compare_checkpoint("--synthetic-option").is_err());
    let file = repo
        .status()
        .unwrap()
        .files
        .into_iter()
        .find(|file| file.path.to_string_lossy() == "mock.rs")
        .unwrap();
    let first = repo
        .review_tokens(&[(file.clone(), Side::Worktree)])
        .unwrap();
    fs::write(root.path().join("mock.rs"), "another round\n").unwrap();
    let second = repo.review_tokens(&[(file, Side::Worktree)]).unwrap();
    assert_ne!(first, second);
    fs::write(root.path().join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(root.path().join("ignored.rs"), "synthetic ignored before\n").unwrap();
    git(&["add", "-f", "ignored.rs"]);
    git(&["update-index", "--split-index"]);
    let before = git(&["ls-files", "--stage"]);
    let checkpoint = repo.checkpoint().unwrap();
    fs::write(root.path().join("ignored.rs"), "synthetic ignored after\n").unwrap();
    let patch = repo.compare_checkpoint(&checkpoint).unwrap();
    assert!(
        patch.contains("-synthetic ignored before"),
        "Tracked ignored files belong in checkpoints"
    );
    assert!(patch.contains("+synthetic ignored after"));
    assert_eq!(git(&["ls-files", "--stage"]), before);
}

#[test]
fn stale_review_validation_cannot_clear_a_newer_review_of_the_same_file() {
    let mut reviews = Reviews::default();
    let key = ReviewKey::new("mock.rs".into(), Side::Worktree);
    reviews.toggle(key.clone(), [1; 32]);
    let revision = reviews.revision();
    reviews.toggle(key.clone(), [2; 32]);
    reviews.reconcile_at(
        revision,
        &std::collections::BTreeMap::from([(key.clone(), [1; 32])]),
    );
    assert!(reviews.contains(&key));
}
