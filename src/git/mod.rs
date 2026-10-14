mod command;
mod diff;
mod discovery;
pub use discovery::{Discovery, Scan, ScanLimit};
mod operations;
mod pipe;
mod stash;
mod status;
mod worktree;
pub use worktree::{Worktree, WorktreeCheckout, WorktreeOperation};

pub use status::{FileStatus, Status};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Repository {
    pub root: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Index,
    Worktree,
}
#[derive(Clone, Debug)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub remote: bool,
    pub upstream: String,
}
#[derive(Clone, Debug)]
pub struct Commit {
    pub hash: String,
    pub parents: String,
    pub author: String,
    pub when: String,
    pub message: String,
    pub refs: String,
}
#[derive(Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub url: String,
}
#[derive(Clone, Debug)]
pub struct Stash {
    pub reference: String,
    pub hash: String,
    pub message: String,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub status: Status,
    pub branches: Vec<Branch>,
    pub tags: Vec<String>,
    pub remotes: Vec<Remote>,
    pub stashes: Vec<Stash>,
    pub commits: Vec<Commit>,
    pub push_remote: Option<String>,
    pub worktrees: Vec<Worktree>,
}

#[derive(Clone, Debug)]
pub enum Operation {
    Stage(PathBuf),
    Unstage(PathBuf),
    Resolve(PathBuf),
    StageAll,
    UnstageAll,
    Hunk {
        file: FileStatus,
        side: Side,
        index: usize,
        expected: std::sync::Arc<str>,
    },
    Commit {
        message: String,
        amend: bool,
    },
    CreateBranch(String),
    Switch(String),
    Track(String),
    RenameBranch {
        old: String,
        new: String,
    },
    DeleteBranch(String),
    CreateTag(String),
    DeleteTag(String),
    AddRemote {
        name: String,
        url: String,
    },
    RemoveRemote(String),
    SetRemoteUrl {
        name: String,
        url: String,
    },
    Fetch,
    Pull,
    PullMerge,
    PullRebase,
    Push {
        remote: String,
        set_upstream: bool,
    },
    StashPush(String),
    StashApply(String),
    StashPop {
        reference: String,
        expected: String,
    },
    StashDrop {
        reference: String,
        expected: String,
    },
    Merge(String),
    Rebase(String),
    Continue,
    Abort,
}

fn arguments(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes)
        .trim_end_matches(['\n', '\r'])
        .into()
}
fn patch(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| {
        "Diff contains non-UTF-8 text. Stage the whole file or inspect it in the terminal.".into()
    })
}

fn reference(value: &str) -> Result<&str, String> {
    if value.is_empty() || value.starts_with('-') || value.contains(['\0', '\n', '\r']) {
        Err("Invalid or empty Git reference".into())
    } else {
        Ok(value)
    }
}

impl Repository {
    pub fn open(directory: &Path) -> Result<Self, String> {
        let root = command::run(directory, &arguments(&["rev-parse", "--show-toplevel"]))?;
        // Git prints ordinary drive paths on Windows, while workspace identities use
        // canonical \\?\ paths. Normalize here so containment and ownership comparisons agree.
        let root = status::path(root.strip_suffix(b"\n").unwrap_or(&root))?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let git_dir = command::run(&root, &arguments(&["rev-parse", "--absolute-git-dir"]))?;
        let git_dir = status::path(git_dir.strip_suffix(b"\n").unwrap_or(&git_dir))?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let common = command::run(&root, &arguments(&["rev-parse", "--git-common-dir"]))?;
        let common = status::path(common.strip_suffix(b"\n").unwrap_or(&common))?;
        let common_dir = root
            .join(common)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            root,
            git_dir,
            common_dir,
        })
    }
    fn run(&self, values: &[&str]) -> Result<Vec<u8>, String> {
        command::run(&self.root, &arguments(values))
    }
    pub fn initialize(directory: &Path) -> Result<Self, String> {
        command::run(directory, &arguments(&["init"]))?;
        Self::open(directory)
    }
    pub fn clone_into(url: &str, destination: &Path) -> Result<Self, String> {
        reference(url)?;
        let parent = destination.parent().ok_or("Select a clone destination")?;
        let args = vec![
            "clone".into(),
            "--".into(),
            url.into(),
            destination.as_os_str().into(),
        ];
        command::run(parent, &args)?;
        Self::open(destination)
    }
    pub fn clone_name(url: &str) -> &str {
        let last = url
            .trim()
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\', ':'])
            .next()
            .unwrap_or_default();
        let name = last.strip_suffix(".git").unwrap_or(last);
        if name.is_empty()
            || matches!(name, "." | "..")
            || name.contains(['?', '#', '<', '>', '|', '*', '\n', '\r'])
        {
            "repository"
        } else {
            name
        }
    }
    pub fn status(&self) -> Result<Status, String> {
        let mut status = status::parse(&self.run(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ])?)?;
        status.operation = self.operation();
        Ok(status)
    }
    fn operation(&self) -> Option<String> {
        for (name, file) in [
            ("rebase", "rebase-merge"),
            ("rebase", "rebase-apply"),
            ("merge", "MERGE_HEAD"),
            ("cherry-pick", "CHERRY_PICK_HEAD"),
            ("revert", "REVERT_HEAD"),
        ] {
            if self.git_dir.join(file).exists() {
                return Some(name.into());
            }
        }
        None
    }
    pub fn snapshot(&self, limit: usize) -> Result<Snapshot, String> {
        let status = self.status()?;
        let branches = text(self.run(&[
            "for-each-ref",
            "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)%00%(refname)%00%(symref)",
            "refs/heads",
            "refs/remotes",
        ])?)
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split('\0').collect();
            // Remote HEAD is an alias, so it must never become a checkout or merge target.
            (parts.len() == 5 && parts[4].is_empty()).then(|| Branch {
                name: parts[0].into(),
                current: parts[1] == "*",
                upstream: parts[2].into(),
                remote: parts[3].starts_with("refs/remotes/"),
            })
        })
        .collect();
        let tags = text(self.run(&["tag", "--list"])?)
            .lines()
            .map(String::from)
            .collect();
        let mut remotes = Vec::new();
        for name in text(self.run(&["remote"])?).lines() {
            remotes.push(Remote {
                name: name.into(),
                url: text(self.run(&["remote", "get-url", name])?),
            });
        }
        let stashes = text(self.run(&["stash", "list", "--format=%gd%x00%H%x00%gs"])?)
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\0');
                Some(Stash {
                    reference: parts.next()?.into(),
                    hash: parts.next()?.into(),
                    message: parts.next()?.into(),
                })
            })
            .collect();
        let commits = if status.unborn {
            Vec::new()
        } else {
            self.history(limit)?
        };
        let mut push_remote = None;
        for key in [
            format!("branch.{}.pushRemote", status.branch),
            "remote.pushDefault".into(),
            format!("branch.{}.remote", status.branch),
        ] {
            if let Ok(value) = self.run(&["config", "--get", &key]) {
                push_remote = Some(text(value));
                break;
            }
        }
        if push_remote.is_none() && remotes.len() == 1 {
            push_remote = Some(remotes[0].name.clone());
        }
        push_remote = push_remote.filter(|name| remotes.iter().any(|remote| &remote.name == name));
        Ok(Snapshot {
            status,
            branches,
            tags,
            remotes,
            stashes,
            commits,
            push_remote,
            worktrees: self.worktrees()?,
        })
    }
    pub fn history(&self, limit: usize) -> Result<Vec<Commit>, String> {
        // --all also includes stash's private index/untracked parents; those belong to the stash view.
        let output = self.run(&[
            "log",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
            "--date-order",
            &format!("--max-count={}", limit.min(2000)),
            "--format=%H%x00%P%x00%an%x00%aI%x00%s%x00%D%x00",
        ])?;
        let fields: Vec<_> = output.split(|byte| *byte == 0).collect();
        Ok(fields
            .chunks(6)
            .filter_map(|parts| {
                if parts.len() < 6 {
                    return None;
                }
                Some(Commit {
                    hash: String::from_utf8_lossy(parts[0]).trim().into(),
                    parents: String::from_utf8_lossy(parts[1]).into(),
                    author: String::from_utf8_lossy(parts[2]).into(),
                    when: String::from_utf8_lossy(parts[3]).into(),
                    message: String::from_utf8_lossy(parts[4]).into(),
                    refs: String::from_utf8_lossy(parts[5]).into(),
                })
            })
            .collect())
    }
    pub fn show_commit(&self, hash: &str) -> Result<String, String> {
        reference(hash)?;
        patch(self.run(&[
            "show",
            "--no-ext-diff",
            "--no-textconv",
            "--format=fuller",
            "--stat",
            "--patch",
            hash,
            "--",
        ])?)
    }
    pub fn show_stash(&self, name: &str) -> Result<String, String> {
        reference(name)?;
        patch(self.run(&[
            "stash",
            "show",
            "--no-ext-diff",
            "--no-textconv",
            "--include-untracked",
            "--patch",
            name,
        ])?)
    }
    pub fn diff(&self, file: &FileStatus, side: Side) -> Result<String, String> {
        if file.conflicted {
            return patch(self.run_path(
                &["diff", "--no-ext-diff", "--no-textconv", "--cc", "--"],
                &file.path,
            )?);
        }
        if side == Side::Worktree && file.worktree == '?' {
            let bytes = self.read_file(&file.path)?;
            if bytes.contains(&0) {
                return Ok("Binary file (untracked)".into());
            }
            let content = String::from_utf8(bytes).map_err(|_| "File is not UTF-8 text")?;
            return Ok(format!(
                "--- /dev/null\n+++ {}\n@@ -0,0 +1,{} @@\n{}",
                file.path.to_string_lossy(),
                content.lines().count(),
                content
                    .lines()
                    .map(|line| format!("+{line}\n"))
                    .collect::<String>()
            ));
        }
        let mut args = arguments(&["diff", "--no-ext-diff", "--no-textconv", "--no-color"]);
        if side == Side::Index {
            args.push("--cached".into());
        }
        args.push("--".into());
        args.push(file.path.as_os_str().into());
        if let Some(original) = &file.original {
            args.push(original.as_os_str().into());
        }
        patch(command::run(&self.root, &args)?)
    }
    fn run_path(&self, values: &[&str], path: &Path) -> Result<Vec<u8>, String> {
        let mut args = arguments(values);
        args.push(path.as_os_str().into());
        command::run(&self.root, &args)
    }
    pub fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        crate::files::read(&self.root, path)
    }
    pub fn write_file(&self, path: &Path, contents: &str, expected: &str) -> Result<(), String> {
        crate::files::write(&self.root, path, contents, expected)
    }
    pub fn conflict_content(&self, path: &Path, stage: usize) -> Result<String, String> {
        if !(1..=3).contains(&stage) {
            return Err("Invalid conflict stage".into());
        }
        let mut object = OsString::from(format!(":{stage}:"));
        object.push(path.as_os_str());
        Ok(text(command::run(&self.root, &["show".into(), object])?))
    }
}
