use super::*;

impl Repository {
    pub fn execute(&self, operation: Operation) -> Result<String, Error> {
        let mut args: Vec<OsString>;
        match operation {
            Operation::Hunk {
                file,
                side,
                index,
                expected,
            } => return self.apply_hunk(&file, side, index, &expected),
            Operation::Resolve(path) => {
                let inspect = match std::fs::symlink_metadata(self.root.join(&path)) {
                    Ok(metadata) => !metadata.file_type().is_symlink(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error.into()),
                };
                if inspect {
                    let content = self.read_file(&path)?;
                    // A bare ======= can be a Markdown heading. Labelled markers
                    // distinguish unresolved conflicts from legitimate separators.
                    if String::from_utf8_lossy(&content).lines().any(|line| {
                        ['<', '>', '|'].into_iter().any(|marker| {
                            let count = line
                                .chars()
                                .take_while(|character| *character == marker)
                                .count();
                            count >= 7 && line[count..].starts_with(' ')
                        })
                    }) {
                        return Err(
                            "Remove conflict markers and save the result before marking resolved"
                                .into(),
                        );
                    }
                }
                return self.run_path(&["add", "--"], &path).map(text);
            }
            Operation::Stage(path) => {
                // The terminal may have started a merge since the last UI refresh.
                if self
                    .status()?
                    .files
                    .iter()
                    .any(|file| file.path == path && file.conflicted)
                {
                    return Err(
                        "This file has a conflict. Save the resolved result and mark it resolved."
                            .into(),
                    );
                }
                return self.run_path(&["add", "--"], &path).map(text);
            }
            Operation::StageFiles(paths) => {
                if paths.is_empty() {
                    return Ok(String::new());
                }
                let selected: std::collections::BTreeSet<_> = paths.iter().collect();
                if self
                    .status()?
                    .files
                    .iter()
                    .any(|file| file.conflicted && selected.contains(&file.path))
                {
                    return Err(
                        "This file has a conflict. Save the resolved result and mark it resolved."
                            .into(),
                    );
                }
                // NUL-delimited stdin keeps large groups within OS argument limits and paths literal.
                let mut input = Vec::new();
                for path in paths {
                    let bytes = path.as_os_str().as_encoded_bytes();
                    if bytes.contains(&0) {
                        return Err("Invalid Git file path".into());
                    }
                    input.extend_from_slice(bytes);
                    input.push(0);
                }
                return command::run_input(
                    &self.root,
                    &arguments(&["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"]),
                    Some(input),
                )
                .map(text);
            }
            Operation::Unstage(path) => {
                let status = self.status()?;
                if status.unborn {
                    // With no HEAD, removing the index entry is the only unstage
                    // operation. --cached must stay: even a newer worktree draft survives.
                    return self
                        .run_path(&["rm", "--cached", "-f", "--"], &path)
                        .map(text);
                }
                args = arguments(&["restore", "--staged", "--"]);
                args.push(path.as_os_str().into());
                if let Some(file) = status.files.iter().find(|file| file.path == path)
                    && let Some(original) = &file.original
                {
                    args.push(original.as_os_str().into());
                }
            }
            Operation::StageAll => {
                if self.status()?.files.iter().any(|file| file.conflicted) {
                    return Err("Resolve conflicts individually before staging all changes".into());
                }
                args = arguments(&["add", "-A", "--", "."]);
            }
            Operation::UnstageAll => {
                args = if self.status()?.unborn {
                    arguments(&["rm", "--cached", "-r", "-f", "--", "."])
                } else {
                    arguments(&["restore", "--staged", "--", "."])
                }
            }
            Operation::Commit { message, amend } => {
                if message.trim().is_empty() {
                    return Err("A commit message is required".into());
                }
                let status = self.status()?;
                if status.files.iter().any(|file| file.conflicted) {
                    return Err("Resolve conflicts before committing".into());
                }
                if !amend
                    && !status.files.iter().any(|file| file.index != '.')
                    && status.operation.is_none()
                {
                    return Err("Stage changes before committing".into());
                }
                args = arguments(&["commit"]);
                if amend {
                    args.push("--amend".into());
                }
                args.extend(["-m".into(), message.into()]);
            }
            Operation::CreateBranch(name) => {
                reference(&name)?;
                args = arguments(&["branch", &name]);
            }
            Operation::Switch(name) => {
                reference(&name)?;
                args = arguments(&["switch", &name]);
            }
            Operation::Track(name) => {
                reference(&name)?;
                args = arguments(&["switch", "--track", &name]);
            }
            Operation::RenameBranch { old, new } => {
                reference(&old)?;
                reference(&new)?;
                args = arguments(&["branch", "-m", &old, &new]);
            }
            Operation::DeleteBranch(name) => {
                reference(&name)?;
                args = arguments(&["branch", "-d", &name]);
            }
            Operation::CreateTag(name) => {
                reference(&name)?;
                args = arguments(&["tag", &name]);
            }
            Operation::DeleteTag(name) => {
                reference(&name)?;
                args = arguments(&["tag", "-d", &name]);
            }
            Operation::AddRemote { name, url } => {
                reference(&name)?;
                reference(&url)?;
                args = arguments(&["remote", "add", &name, &url]);
            }
            Operation::RemoveRemote(name) => {
                reference(&name)?;
                args = arguments(&["remote", "remove", &name]);
            }
            Operation::SetRemoteUrl { name, url } => {
                reference(&name)?;
                reference(&url)?;
                args = arguments(&["remote", "set-url", &name, &url]);
            }
            Operation::Fetch => args = arguments(&["fetch", "--all", "--prune"]),
            Operation::Pull => args = arguments(&["pull", "--ff-only"]),
            Operation::PullMerge => args = arguments(&["pull", "--no-rebase", "--no-edit"]),
            Operation::PullRebase => args = arguments(&["pull", "--rebase"]),
            Operation::Push {
                remote,
                set_upstream,
            } => {
                reference(&remote)?;
                args = arguments(&["push"]);
                if set_upstream {
                    args.push("--set-upstream".into());
                }
                args.push(remote.into());
                if set_upstream {
                    args.push("HEAD".into());
                }
            }
            Operation::StashPush(message) => {
                args = arguments(&["stash", "push", "--include-untracked", "-m", &message])
            }
            Operation::StashApply(name) => {
                reference(&name)?;
                args = arguments(&["stash", "apply", &name]);
            }
            Operation::StashPop {
                reference: name,
                expected,
            } => {
                self.verify_stash(&name, &expected)?;
                // Apply the immutable object; a terminal can shift stash@{n}
                // while a native confirmation dialog is open.
                self.run(&["stash", "apply", &expected])?;
                return self
                    .drop_stash_locked(&name, &expected, || Ok(()))
                    .map_err(|error| {
                        error.with_guidance("Stash applied; automatic removal did not complete")
                    });
            }
            Operation::StashDrop {
                reference: name,
                expected,
            } => {
                self.verify_stash(&name, &expected)?;
                return self.drop_stash_locked(&name, &expected, || Ok(()));
            }
            Operation::Merge(name) => {
                reference(&name)?;
                args = arguments(&["merge", "--no-edit", &name]);
            }
            Operation::Rebase(name) => {
                reference(&name)?;
                args = arguments(&["rebase", &name]);
            }
            Operation::Continue => {
                let operation = self.operation().ok_or("No operation to continue")?;
                args = arguments(&[&operation, "--continue"]);
            }
            Operation::Abort => {
                let operation = self.operation().ok_or("No operation to abort")?;
                args = arguments(&[&operation, "--abort"]);
            }
        }
        command::run(&self.root, &args).map(text)
    }

    fn verify_stash(&self, name: &str, expected: &str) -> Result<(), Error> {
        reference(name)?;
        reference(expected)?;
        if text(self.run(&["rev-parse", "--verify", name])?) != expected {
            return Err(
                "The stash list changed. Refresh it before applying or deleting this stash.".into(),
            );
        }
        Ok(())
    }
}
