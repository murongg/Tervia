use super::{Repository, arguments, command, reference, status, text};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub main: bool,
    pub bare: bool,
    pub detached: bool,
    pub missing: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
}

#[derive(Clone, Debug)]
pub enum WorktreeCheckout {
    NewBranch { name: String, start: String },
    ExistingBranch(String),
    Detached(String),
}

#[derive(Clone, Debug)]
pub enum WorktreeOperation {
    Add {
        path: PathBuf,
        checkout: WorktreeCheckout,
    },
    Remove(Worktree),
    Lock {
        tree: Worktree,
        reason: String,
    },
    Unlock(Worktree),
    Prune(Vec<Worktree>),
}

impl Repository {
    pub fn common_directory(&self) -> Result<PathBuf, String> {
        Ok(self.common_dir.clone())
    }

    pub fn worktrees(&self) -> Result<Vec<Worktree>, String> {
        // NUL records preserve literal paths, including newlines and quote characters.
        let mut trees = parse(&self.run(&["worktree", "list", "--porcelain", "-z"])?)?;
        for tree in &mut trees {
            tree.missing = !tree.path.is_dir();
        }
        Ok(trees)
    }

    pub fn execute_worktree(&self, operation: WorktreeOperation) -> Result<String, String> {
        let mut args: Vec<OsString>;
        match operation {
            WorktreeOperation::Add { path, checkout } => {
                if !path.is_absolute() {
                    return Err("Select an absolute worktree destination".into());
                }
                args = arguments(&["worktree", "add"]);
                let revision = match checkout {
                    WorktreeCheckout::NewBranch { name, start } => {
                        reference(&name)?;
                        self.run(&["check-ref-format", "--branch", &name])?;
                        args.extend(arguments(&["-b", &name]));
                        start
                    }
                    WorktreeCheckout::ExistingBranch(name) => {
                        reference(&name)?;
                        self.run(&["check-ref-format", "--branch", &name])?;
                        self.run(&["show-ref", "--verify", &format!("refs/heads/{name}")])?;
                        // A full refs/heads name makes worktree add detach HEAD; checkout needs the short name.
                        name
                    }
                    WorktreeCheckout::Detached(revision) => {
                        args.push("--detach".into());
                        revision
                    }
                };
                reference(&revision)?;
                self.run(&[
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    &format!("{revision}^{{commit}}"),
                ])?;
                args.push("--".into());
                args.push(path.into_os_string());
                args.push(revision.into());
            }
            WorktreeOperation::Remove(expected) => {
                let tree = self.worktree_target(&expected)?;
                if tree.main || tree.bare || tree.path == self.root || tree.locked.is_some() {
                    return Err("Cannot remove the main, current, bare or locked worktree".into());
                }
                // Git rechecks tracked/untracked edits and submodules. Never force deletion.
                args = arguments(&["worktree", "remove", "--"]);
                args.push(tree.path.into_os_string());
            }
            WorktreeOperation::Lock { tree, reason } => {
                let tree = self.worktree_target(&tree)?;
                if tree.main || tree.locked.is_some() {
                    return Err("Worktree cannot be locked".into());
                }
                if reason.contains('\0') {
                    return Err("Invalid lock reason".into());
                }
                args = arguments(&["worktree", "lock", "--reason", &reason, "--"]);
                args.push(tree.path.into_os_string());
            }
            WorktreeOperation::Unlock(tree) => {
                let tree = self.worktree_target(&tree)?;
                if tree.main || tree.locked.is_none() {
                    return Err("Worktree is not locked".into());
                }
                args = arguments(&["worktree", "unlock", "--"]);
                args.push(tree.path.into_os_string());
            }
            WorktreeOperation::Prune(expected) => {
                let targets: Vec<_> = self
                    .worktrees()?
                    .into_iter()
                    .filter(|tree| tree.prunable.is_some() && tree.locked.is_none() && !tree.main)
                    .collect();
                // Do not prune additional stale registrations that appeared after confirmation.
                if targets.is_empty() || targets != expected {
                    return Err("Missing worktree records changed; refresh before pruning".into());
                }
                args = arguments(&["worktree", "prune", "--verbose", "--expire=now"]);
            }
        }
        command::run(&self.root, &args).map(text)
    }

    fn worktree_target(&self, expected: &Worktree) -> Result<Worktree, String> {
        self.worktrees()?
            .into_iter()
            .find(|tree| {
                tree.path == expected.path
                    && tree.head == expected.head
                    && tree.branch == expected.branch
            })
            .ok_or_else(|| "Worktree changed since it was selected; refresh before retrying".into())
    }
}

fn parse(output: &[u8]) -> Result<Vec<Worktree>, String> {
    let mut result = Vec::new();
    let mut current = None;
    for field in output.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(tree) = current.take() {
                result.push(tree);
            }
            current = Some(Worktree {
                path: status::path(path)?,
                main: result.is_empty(),
                ..Worktree::default()
            });
        } else if let Some(tree) = current.as_mut() {
            let text = String::from_utf8_lossy(field);
            if let Some(head) = text.strip_prefix("HEAD ") {
                tree.head = head.into();
            } else if let Some(branch) = text.strip_prefix("branch refs/heads/") {
                tree.branch = Some(branch.into());
            } else if text == "bare" {
                tree.bare = true;
            } else if text == "detached" {
                tree.detached = true;
            } else if let Some(reason) = text.strip_prefix("locked") {
                tree.locked = Some(reason.trim_start_matches(' ').into());
            } else if let Some(reason) = text.strip_prefix("prunable") {
                tree.prunable = Some(reason.trim_start_matches(' ').into());
            }
        }
    }
    if let Some(tree) = current {
        result.push(tree);
    }
    Ok(result)
}
