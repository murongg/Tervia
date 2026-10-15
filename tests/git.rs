use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use tervia::git::{ErrorKind, Operation, Repository, Side};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tervia-git-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        fixture.git(&["init", "-b", "main"]);
        fixture.git(&["config", "user.name", "Synthetic Developer"]);
        fixture.git(&["config", "user.email", "developer@example.test"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture.git(&["config", "tag.gpgsign", "false"]);
        fixture.git(&["config", "core.hooksPath", ".synthetic-hooks"]);
        fixture.git(&["config", "core.autocrlf", "false"]);
        fixture
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.0)
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
        String::from_utf8_lossy(&output.stdout).trim().into()
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
    fn seed(&self) {
        self.write("sample.txt", "first\nsecond\nthird\n");
        self.git(&["add", "."]);
        self.git(&["commit", "-m", "Synthetic initial commit"]);
    }
    fn repo(&self) -> Repository {
        Repository::open(&self.0).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn probe_accepts_an_ordinary_directory_without_a_repository() {
    let root = tempfile::tempdir().unwrap();
    assert!(Repository::probe(root.path()).unwrap().is_none());
}

#[test]
fn probe_preserves_invalid_and_empty_git_metadata_errors() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".git"), "synthetic invalid gitfile\n").unwrap();
    assert!(Repository::probe(root.path()).is_err());
    fs::remove_file(root.path().join(".git")).unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    assert!(Repository::probe(root.path()).is_err());
}

#[test]
fn probe_preserves_damaged_ancestor_metadata_and_missing_paths() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("nested/child");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    assert!(Repository::probe(&nested).is_err());
    assert!(Repository::probe(&root.path().join("synthetic-missing")).is_err());
}

#[cfg(unix)]
#[test]
fn probe_preserves_broken_git_symlink_errors() {
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("synthetic-missing", root.path().join(".git")).unwrap();
    assert!(Repository::probe(root.path()).is_err());
}

#[test]
fn probe_finds_repository_from_a_nested_directory() {
    let fixture = Fixture::new();
    let nested = fixture.0.join("nested/child");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(
        Repository::probe(&nested).unwrap().unwrap().root,
        fixture.0.canonicalize().unwrap()
    );
}

#[test]
fn probe_finds_the_nearest_nested_repository() {
    let fixture = Fixture::new();
    let inner = fixture.0.join("synthetic-inner");
    fs::create_dir(&inner).unwrap();
    let repository = Repository::initialize(&inner).unwrap();
    let nested = inner.join("nested/child");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(
        Repository::probe(&nested).unwrap().unwrap().root,
        repository.root
    );
}

#[test]
fn probe_does_not_accept_files_or_missing_gitfile_targets_as_ordinary_directories() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("synthetic-file");
    fs::write(&file, "synthetic contents\n").unwrap();
    assert!(Repository::probe(&file).is_err());
    fs::write(root.path().join(".git"), "gitdir: synthetic-missing\n").unwrap();
    let error = Repository::probe(root.path()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Command);
    assert!(!error.status().unwrap().success());
}

#[cfg(unix)]
#[test]
fn probe_preserves_inaccessible_paths_and_git_metadata() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o100)).unwrap();
    let inaccessible = Repository::probe(root.path());
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(inaccessible.is_err());
    let marker = root.path().join(".git");
    fs::create_dir(&marker).unwrap();
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o000)).unwrap();
    let damaged = Repository::probe(root.path());
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(damaged.is_err());
}

#[test]
fn typed_validation_errors_preserve_actionable_text() {
    let fixture = Fixture::new();
    let error = fixture
        .repo()
        .execute(Operation::Commit {
            message: String::new(),
            amend: false,
        })
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Message);
    assert_eq!(error.message(), "A commit message is required");
    assert_eq!(error.to_string(), "A commit message is required");
}

#[test]
fn empty_repository_and_literal_paths_are_supported() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    assert!(repo.snapshot(50).unwrap().commits.is_empty());
    fixture.write("[mock] space.txt", "synthetic\n");
    repo.execute(Operation::Stage(PathBuf::from("[mock] space.txt")))
        .unwrap();
    let snapshot = repo.snapshot(50).unwrap();
    assert_eq!(snapshot.status.files.len(), 1);
    assert_eq!(snapshot.status.files[0].index, 'A');
    assert!(
        repo.diff(&snapshot.status.files[0], Side::Index)
            .unwrap()
            .contains("+synthetic")
    );
    fixture.write("[mock] space.txt", "new synthetic draft\n");
    repo.execute(Operation::Unstage(PathBuf::from("[mock] space.txt")))
        .unwrap();
    assert_eq!(repo.snapshot(50).unwrap().status.files[0].worktree, '?');
    assert_eq!(
        fs::read_to_string(fixture.0.join("[mock] space.txt")).unwrap(),
        "new synthetic draft\n"
    );
}

#[test]
fn staging_a_group_only_adds_its_literal_paths_and_accepts_an_empty_group() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.write("sample.txt", "synthetic tracked edit\n");
    fixture.write("[mock] space.txt", "synthetic new file\n");
    fixture.write("m space.txt", "synthetic other new file\n");
    let mut paths = vec!["[mock] space.txt".into()];
    if cfg!(unix) {
        fixture.write("synthetic\nnew.txt", "synthetic newline path\n");
        paths.push("synthetic\nnew.txt".into());
    }
    let repo = fixture.repo();
    repo.execute(Operation::StageFiles(paths)).unwrap();
    let status = repo.status().unwrap();
    let state = |path: &str| {
        let file = status
            .files
            .iter()
            .find(|file| file.path.to_str() == Some(path))
            .unwrap();
        (file.index, file.worktree)
    };
    assert_eq!(state("[mock] space.txt"), ('A', '.'));
    if cfg!(unix) {
        assert_eq!(state("synthetic\nnew.txt"), ('A', '.'));
    }
    assert_eq!(state("m space.txt"), ('.', '?'));
    assert_eq!(state("sample.txt"), ('.', 'M'));
    repo.execute(Operation::StageFiles(Vec::new())).unwrap();
    assert_eq!(repo.status().unwrap().files, status.files);
    fs::remove_file(fixture.0.join("sample.txt")).unwrap();
    repo.execute(Operation::StageFiles(vec!["sample.txt".into()]))
        .unwrap();
    assert_eq!(
        repo.status()
            .unwrap()
            .files
            .iter()
            .find(|file| file.path.to_str() == Some("sample.txt"))
            .unwrap()
            .index,
        'D'
    );
}

#[test]
fn staged_and_worktree_changes_coexist_and_commit_only_consumes_index() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    fixture.write("sample.txt", "staged\nsecond\nthird\n");
    repo.execute(Operation::Stage("sample.txt".into())).unwrap();
    fixture.write("sample.txt", "worktree\nsecond\nthird\n");
    let snapshot = repo.snapshot(50).unwrap();
    let file = &snapshot.status.files[0];
    assert_eq!((file.index, file.worktree), ('M', 'M'));
    assert!(repo.diff(file, Side::Index).unwrap().contains("+staged"));
    assert!(
        repo.diff(file, Side::Worktree)
            .unwrap()
            .contains("+worktree")
    );
    repo.execute(Operation::Commit {
        message: "Synthetic staged commit\n\nBody".into(),
        amend: false,
    })
    .unwrap();
    assert!(
        fixture
            .git(&["show", "HEAD:sample.txt"])
            .starts_with("staged")
    );
    assert_eq!(repo.snapshot(50).unwrap().status.files[0].index, '.');
    assert!(
        repo.execute(Operation::Commit {
            message: "  ".into(),
            amend: false
        })
        .is_err()
    );
}

#[test]
fn renames_deletions_binary_files_and_untracked_diffs_are_real() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["mv", "sample.txt", "renamed sample.txt"]);
    let repo = fixture.repo();
    let snapshot = repo.snapshot(50).unwrap();
    assert_eq!(
        snapshot.status.files[0].original.as_deref(),
        Some(std::path::Path::new("sample.txt"))
    );
    repo.execute(Operation::Unstage("renamed sample.txt".into()))
        .unwrap();
    assert!(
        repo.snapshot(50)
            .unwrap()
            .status
            .files
            .iter()
            .any(|file| file.worktree == 'D')
    );
    fixture.write("new.txt", "mock contents\n");
    fs::write(fixture.0.join("binary.dat"), b"mock\0bytes").unwrap();
    let snapshot = repo.snapshot(50).unwrap();
    let text = snapshot
        .status
        .files
        .iter()
        .find(|file| file.path == std::path::Path::new("new.txt"))
        .unwrap();
    assert!(
        repo.diff(text, Side::Worktree)
            .unwrap()
            .contains("mock contents")
    );
    let binary = snapshot
        .status
        .files
        .iter()
        .find(|file| file.path == std::path::Path::new("binary.dat"))
        .unwrap();
    assert!(
        repo.diff(binary, Side::Worktree)
            .unwrap()
            .contains("Binary")
    );
}

#[test]
fn branches_tags_stashes_and_history_have_complete_metadata() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    repo.execute(Operation::CreateBranch("feature/mock".into()))
        .unwrap();
    repo.execute(Operation::Switch("feature/mock".into()))
        .unwrap();
    repo.execute(Operation::CreateTag("mock-v1".into()))
        .unwrap();
    fixture.write("sample.txt", "synthetic stash\n");
    fixture.write("untracked.txt", "synthetic untracked\n");
    repo.execute(Operation::StashPush("Synthetic stash".into()))
        .unwrap();
    let snapshot = repo.snapshot(50).unwrap();
    assert!(
        snapshot.status.files.is_empty(),
        "{:?}",
        snapshot.status.files
    );
    assert_eq!(snapshot.status.branch, "feature/mock");
    assert!(
        snapshot
            .branches
            .iter()
            .any(|branch| branch.name == "feature/mock" && branch.current)
    );
    assert!(snapshot.tags.contains(&"mock-v1".into()));
    assert_eq!(snapshot.stashes.len(), 1);
    assert_eq!(
        snapshot.commits.len(),
        1,
        "Stash implementation commits belong to the stash view, not branch history"
    );
    assert_eq!(snapshot.commits[0].message, "Synthetic initial commit");
    assert_eq!(snapshot.commits[0].author, "Synthetic Developer");
    assert!(
        !repo
            .show_commit(&snapshot.commits[0].hash)
            .unwrap()
            .is_empty()
    );
    repo.execute(Operation::StashPop {
        reference: snapshot.stashes[0].reference.clone(),
        expected: snapshot.stashes[0].hash.clone(),
    })
    .unwrap();
    assert!(fixture.0.join("untracked.txt").exists());
    assert!(repo.snapshot(50).unwrap().stashes.is_empty());
}

#[test]
fn reference_history_follows_ancestry_without_switching_the_checkout() {
    let fixture = Fixture::new();
    fixture.seed();
    let base = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["switch", "-c", "feature/mock-history"]);
    fixture.write("feature.txt", "Synthetic feature content\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic feature commit"]);
    let feature = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["branch", "mock-collision"]);
    fixture.git(&["tag", "mock-collision", &base]);
    fixture.git(&["update-ref", "refs/remotes/origin/mock-history", &feature]);
    fixture.git(&["switch", "main"]);
    fixture.write("main.txt", "Synthetic main content\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic main commit"]);
    fixture.write("sample.txt", "Synthetic unsaved working change\n");
    let repo = fixture.repo();
    let main = fixture.git(&["rev-parse", "HEAD"]);
    assert!(
        repo.snapshot(10)
            .unwrap()
            .branches
            .iter()
            .any(|branch| branch.name == "mock-collision" && !branch.remote)
    );
    assert_eq!(
        repo.history_for("refs/heads/main", 1).unwrap()[0].hash,
        main
    );
    for reference in [
        "refs/heads/feature/mock-history",
        "refs/heads/mock-collision",
        "refs/remotes/origin/mock-history",
    ] {
        let commits = repo.history_for(reference, 10).unwrap();
        assert_eq!(
            commits
                .iter()
                .map(|commit| commit.hash.as_str())
                .collect::<Vec<_>>(),
            vec![feature.as_str(), base.as_str()]
        );
        assert_eq!(repo.history_for(reference, 1).unwrap().len(), 1);
    }
    let tagged = repo.history_for("refs/tags/mock-collision", 10).unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].hash, base);
    assert!(repo.history_for("--all", 10).is_err());
    assert!(repo.history_for("refs/heads/mock-missing", 10).is_err());
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), main);
    assert_eq!(
        fs::read_to_string(fixture.0.join("sample.txt")).unwrap(),
        "Synthetic unsaved working change\n"
    );
}

#[test]
fn individual_commit_metadata_can_follow_parents_beyond_the_loaded_history() {
    let fixture = Fixture::new();
    fixture.seed();
    let root = fixture.git(&["rev-parse", "HEAD"]);
    fixture.write("sample.txt", "synthetic second commit\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic second commit"]);
    let repo = fixture.repo();
    let page = repo.history(1).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].parents, root);
    let parent = repo.commit(&page[0].parents).unwrap();
    assert_eq!(parent.hash, root);
    assert!(parent.parents.is_empty());
    assert_eq!(parent.author, "Synthetic Developer");
    assert_eq!(parent.message, "Synthetic initial commit");
    assert!(repo.commit("mock-missing-commit").is_err());
}

#[test]
fn merge_details_show_changes_against_the_first_parent_and_preserve_both_parent_ids() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["checkout", "-b", "feature/mock-graph"]);
    fixture.write("feature.txt", "synthetic feature change\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic feature"]);
    let feature = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["checkout", "main"]);
    fixture.git(&[
        "merge",
        "--no-ff",
        "feature/mock-graph",
        "-m",
        "Synthetic merge",
    ]);
    let repo = fixture.repo();
    let merge = repo.history(1).unwrap().remove(0);
    assert_eq!(merge.parents.split_whitespace().count(), 2);
    assert_eq!(
        merge.parents.split_whitespace().nth(1),
        Some(feature.as_str())
    );
    assert!(
        repo.show_commit(&merge.hash)
            .unwrap()
            .contains("+synthetic feature change")
    );
}

#[test]
fn symbolic_remote_heads_are_not_actionable_branches() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    fixture.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    let snapshot = fixture.repo().snapshot(20).unwrap();
    assert_eq!(snapshot.branches.len(), 2);
    assert!(
        snapshot
            .branches
            .iter()
            .any(|branch| branch.name == "origin/main" && branch.remote)
    );
    assert!(
        snapshot
            .branches
            .iter()
            .all(|branch| branch.name != "origin" && !branch.name.ends_with("/HEAD"))
    );
}

#[test]
fn shifted_stash_references_cannot_pop_or_delete_a_different_stash() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    fixture.write("sample.txt", "first synthetic stash\n");
    repo.execute(Operation::StashPush("First".into())).unwrap();
    let original = repo.snapshot(10).unwrap().stashes[0].clone();
    fixture.write("sample.txt", "second synthetic stash\n");
    repo.execute(Operation::StashPush("Second".into())).unwrap();
    assert!(
        repo.execute(Operation::StashDrop {
            reference: original.reference.clone(),
            expected: original.hash.clone()
        })
        .is_err()
    );
    assert!(
        repo.execute(Operation::StashPop {
            reference: original.reference,
            expected: original.hash
        })
        .is_err()
    );
    assert_eq!(repo.snapshot(10).unwrap().stashes.len(), 2);
    assert!(repo.status().unwrap().files.is_empty());
}

#[test]
fn local_bare_remote_roundtrip_supports_tracking_and_ahead_behind() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    let remote = fixture.0.join("mock-remote.git");
    fixture.git(&["init", "--bare", remote.to_str().unwrap()]);
    repo.execute(Operation::AddRemote {
        name: "mock".into(),
        url: remote.to_string_lossy().into(),
    })
    .unwrap();
    repo.execute(Operation::Push {
        remote: "mock".into(),
        set_upstream: true,
    })
    .unwrap();
    repo.execute(Operation::Fetch).unwrap();
    repo.execute(Operation::Pull).unwrap();
    let snapshot = repo.snapshot(50).unwrap();
    assert_eq!(snapshot.status.upstream.as_deref(), Some("mock/main"));
    assert_eq!((snapshot.status.ahead, snapshot.status.behind), (0, 0));
    assert!(snapshot.remotes.iter().any(|remote| remote.name == "mock"));
    assert_eq!(snapshot.push_remote.as_deref(), Some("mock"));
}

#[test]
fn saving_detects_external_edits_and_preserves_the_disk_version() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    let original =
        String::from_utf8(repo.read_file(std::path::Path::new("sample.txt")).unwrap()).unwrap();
    repo.write_file(
        std::path::Path::new("sample.txt"),
        "edited synthetic\n",
        &original,
    )
    .unwrap();
    fixture.write("sample.txt", "external synthetic\n");
    assert!(
        repo.write_file(
            std::path::Path::new("sample.txt"),
            "new draft\n",
            "edited synthetic\n"
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("sample.txt")).unwrap(),
        "external synthetic\n"
    );
}

#[test]
fn plain_directories_support_safe_file_reads_without_git() {
    let fixture = Fixture::new();
    let folder = fixture.0.join("plain");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("mock.txt"), "synthetic text\n").unwrap();
    assert_eq!(
        tervia::files::read(&folder, std::path::Path::new("mock.txt")).unwrap(),
        b"synthetic text\n"
    );
    assert!(tervia::files::read(&folder, std::path::Path::new("../sample.txt")).is_err());
}

#[test]
fn push_target_is_explicit_when_multiple_remotes_have_no_upstream() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    repo.execute(Operation::AddRemote {
        name: "alpha".into(),
        url: "https://alpha.example.test/mock.git".into(),
    })
    .unwrap();
    repo.execute(Operation::AddRemote {
        name: "beta".into(),
        url: "https://beta.example.test/mock.git".into(),
    })
    .unwrap();
    assert!(repo.snapshot(10).unwrap().push_remote.is_none());
    fixture.git(&["config", "branch.main.pushRemote", "beta"]);
    assert_eq!(
        repo.snapshot(10).unwrap().push_remote.as_deref(),
        Some("beta")
    );
}

#[test]
fn initialization_and_local_clone_create_normal_worktrees() {
    let fixture = Fixture::new();
    fixture.seed();
    let empty = fixture.0.join("empty");
    fs::create_dir(&empty).unwrap();
    assert!(
        Repository::initialize(&empty)
            .unwrap()
            .status()
            .unwrap()
            .unborn
    );
    let cloned = fixture.0.join("cloned");
    let repo = Repository::clone_into(fixture.0.to_str().unwrap(), &cloned).unwrap();
    assert_eq!(repo.snapshot(10).unwrap().commits.len(), 1);
    assert!(cloned.join("sample.txt").exists());
}

#[test]
fn branch_renaming_remote_tracking_and_remote_url_changes_are_supported() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    repo.execute(Operation::CreateBranch("feature/old".into()))
        .unwrap();
    repo.execute(Operation::RenameBranch {
        old: "feature/old".into(),
        new: "feature/new".into(),
    })
    .unwrap();
    assert!(
        repo.snapshot(10)
            .unwrap()
            .branches
            .iter()
            .any(|branch| branch.name == "feature/new")
    );
    repo.execute(Operation::DeleteBranch("feature/new".into()))
        .unwrap();
    repo.execute(Operation::AddRemote {
        name: "mock".into(),
        url: "https://old.example.test/mock.git".into(),
    })
    .unwrap();
    repo.execute(Operation::SetRemoteUrl {
        name: "mock".into(),
        url: "https://new.example.test/mock.git".into(),
    })
    .unwrap();
    assert_eq!(
        repo.snapshot(10).unwrap().remotes[0].url,
        "https://new.example.test/mock.git"
    );
    fixture.git(&["update-ref", "refs/remotes/mock/feature/remote", "HEAD"]);
    repo.execute(Operation::Track("mock/feature/remote".into()))
        .unwrap();
    assert_eq!(
        repo.status().unwrap().upstream.as_deref(),
        Some("mock/feature/remote")
    );
}

#[test]
fn individual_hunks_can_be_staged_unstaged_and_stale_patches_are_rejected() {
    let fixture = Fixture::new();
    let initial: String = (0..30).map(|n| format!("synthetic line {n}\n")).collect();
    fixture.write("sample.txt", &initial);
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic initial"]);
    let changed = initial
        .replace("line 1\n", "first edit\n")
        .replace("line 28\n", "second edit\n");
    fixture.write("sample.txt", &changed);
    let repo = fixture.repo();
    let file = repo.status().unwrap().files[0].clone();
    let patch = repo.diff(&file, Side::Worktree).unwrap();
    repo.execute(Operation::Hunk {
        file: file.clone(),
        side: Side::Worktree,
        index: 0,
        expected: patch.clone().into(),
    })
    .unwrap();
    let indexed = fixture.git(&["show", ":sample.txt"]);
    assert!(indexed.contains("first edit"));
    assert!(!indexed.contains("second edit"));
    assert!(
        repo.execute(Operation::Hunk {
            file: file.clone(),
            side: Side::Worktree,
            index: 1,
            expected: patch.into()
        })
        .is_err()
    );
    let file = repo.status().unwrap().files[0].clone();
    let indexed_patch = repo.diff(&file, Side::Index).unwrap();
    repo.execute(Operation::Hunk {
        file,
        side: Side::Index,
        index: 0,
        expected: indexed_patch.into(),
    })
    .unwrap();
    assert_eq!(repo.status().unwrap().files[0].index, '.');
}

#[test]
fn hunk_staging_preserves_crlf_and_trailing_whitespace_exactly() {
    let fixture = Fixture::new();
    let initial: String = (0..12).map(|n| format!("synthetic {n}\r\n")).collect();
    fixture.write("sample.txt", &initial);
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic CRLF"]);
    let changed = initial.replace("synthetic 0\r\n", "changed\t  \r\n");
    fixture.write("sample.txt", &changed);
    let repo = fixture.repo();
    let file = repo.status().unwrap().files[0].clone();
    let patch = repo.diff(&file, Side::Worktree).unwrap();
    repo.execute(Operation::Hunk {
        file,
        side: Side::Worktree,
        index: 0,
        expected: patch.into(),
    })
    .unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(&fixture.0)
        .args(["show", ":sample.txt"])
        .output()
        .unwrap();
    assert_eq!(output.stdout, changed.as_bytes());
}

#[test]
fn unsupported_diff_encoding_does_not_rewrite_bytes_when_staging_a_file() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("sample.txt"), b"mock\xff\n").unwrap();
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Synthetic byte content"]);
    let changed = b"changed\xff\n";
    fs::write(fixture.0.join("sample.txt"), changed).unwrap();
    let repo = fixture.repo();
    let file = repo.status().unwrap().files[0].clone();
    assert!(repo.diff(&file, Side::Worktree).is_err());
    repo.execute(Operation::Stage(file.path)).unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(&fixture.0)
        .args(["show", ":sample.txt"])
        .output()
        .unwrap();
    assert_eq!(output.stdout, changed);
}

#[test]
fn merge_conflicts_are_detected_and_can_be_resolved_then_continued() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    fixture.git(&["checkout", "-b", "feature/mock"]);
    fixture.write("sample.txt", "feature\n");
    fixture.git(&["commit", "-am", "Synthetic feature"]);
    fixture.git(&["checkout", "main"]);
    fixture.write("sample.txt", "main\n");
    fixture.git(&["commit", "-am", "Synthetic main"]);
    assert!(
        repo.execute(Operation::Merge("feature/mock".into()))
            .is_err()
    );
    let snapshot = repo.snapshot(50).unwrap();
    assert_eq!(snapshot.status.operation.as_deref(), Some("merge"));
    assert!(repo.execute(Operation::StageAll).is_err());
    assert!(
        repo.execute(Operation::StageFiles(vec!["sample.txt".into()]))
            .is_err()
    );
    assert!(repo.execute(Operation::Stage("sample.txt".into())).is_err());
    assert!(snapshot.status.files[0].conflicted);
    assert!(
        repo.execute(Operation::Resolve("sample.txt".into()))
            .is_err()
    );
    assert!(
        repo.conflict_content(&snapshot.status.files[0].path, 2)
            .unwrap()
            .starts_with("main")
    );
    assert!(
        repo.conflict_content(&snapshot.status.files[0].path, 3)
            .unwrap()
            .starts_with("feature")
    );
    fixture.write("sample.txt", "resolved synthetic\n=======\n");
    repo.execute(Operation::Resolve("sample.txt".into()))
        .unwrap();
    repo.execute(Operation::Continue).unwrap();
    assert!(repo.snapshot(50).unwrap().status.operation.is_none());
}

#[test]
fn a_conflicted_file_can_be_resolved_as_deleted() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    fixture.git(&["checkout", "-b", "feature/delete"]);
    fixture.git(&["rm", "sample.txt"]);
    fixture.git(&["commit", "-m", "Synthetic deletion"]);
    fixture.git(&["checkout", "main"]);
    fixture.write("sample.txt", "synthetic modified\n");
    fixture.git(&["commit", "-am", "Synthetic modification"]);
    assert!(
        repo.execute(Operation::Merge("feature/delete".into()))
            .is_err()
    );
    fs::remove_file(fixture.0.join("sample.txt")).unwrap();
    repo.execute(Operation::Resolve("sample.txt".into()))
        .unwrap();
    repo.execute(Operation::Continue).unwrap();
    assert!(!fixture.0.join("sample.txt").exists());
    assert!(repo.status().unwrap().operation.is_none());
}

#[test]
fn linked_worktree_discovery_and_detached_head_do_not_assume_git_directory() {
    let fixture = Fixture::new();
    fixture.seed();
    let linked = fixture.0.join("linked");
    fixture.git(&["worktree", "add", "--detach", linked.to_str().unwrap()]);
    let repo = Repository::probe(&linked).unwrap().unwrap();
    assert_eq!(repo.root, linked.canonicalize().unwrap());
    assert_eq!(repo.snapshot(50).unwrap().status.branch, "(detached)");
}

#[test]
fn rebase_abort_and_failed_commit_preserve_work_and_expose_errors() {
    let fixture = Fixture::new();
    fixture.seed();
    let repo = fixture.repo();
    fixture.git(&["checkout", "-b", "feature/mock"]);
    fixture.write("sample.txt", "feature\n");
    fixture.git(&["commit", "-am", "Synthetic feature"]);
    fixture.git(&["checkout", "main"]);
    fixture.write("sample.txt", "main\n");
    fixture.git(&["commit", "-am", "Synthetic main"]);
    fixture.git(&["checkout", "feature/mock"]);
    assert!(repo.execute(Operation::Rebase("main".into())).is_err());
    assert_eq!(
        repo.snapshot(50).unwrap().status.operation.as_deref(),
        Some("rebase")
    );
    repo.execute(Operation::Abort).unwrap();
    assert!(
        fixture
            .git(&["show", "HEAD:sample.txt"])
            .starts_with("feature")
    );
    assert!(repo.execute(Operation::Rebase("main".into())).is_err());
    fixture.write("sample.txt", "resolved synthetic rebase\n");
    repo.execute(Operation::Resolve("sample.txt".into()))
        .unwrap();
    repo.execute(Operation::Continue).unwrap();
    assert!(repo.status().unwrap().operation.is_none());
    fixture.git(&["merge-base", "--is-ancestor", "main", "HEAD"]);
    fixture.write("sample.txt", "uncommitted synthetic\n");
    repo.execute(Operation::Stage("sample.txt".into())).unwrap();
    fixture.git(&["config", "commit.gpgsign", "true"]);
    fixture.git(&["config", "gpg.program", "tervia-synthetic-missing-signer"]);
    assert!(
        repo.execute(Operation::Commit {
            message: "Synthetic failure".into(),
            amend: false
        })
        .is_err()
    );
    assert_eq!(repo.snapshot(50).unwrap().status.files[0].index, 'M');
}
#[test]
fn clone_destination_name_follows_https_ssh_and_local_repository_names() {
    for (address, name) in [
        (
            "https://example.test/team/mock-repository.git",
            "mock-repository",
        ),
        (
            "git@example.test:team/mock-repository.git",
            "mock-repository",
        ),
        (
            "ssh://git@example.test:2222/team/mock-repository.git/",
            "mock-repository",
        ),
        (" /synthetic/mock-repository.git/ ", "mock-repository"),
        ("C:\\synthetic\\mock-repository.git", "mock-repository"),
        ("https://example.test/team/mock-project", "mock-project"),
        ("", "repository"),
        ("..", "repository"),
        ("https://example.test/team/.git", "repository"),
    ] {
        assert_eq!(tervia::git::Repository::clone_name(address), name);
    }
}
