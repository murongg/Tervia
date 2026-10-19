use super::*;
use std::path::Path;

impl Tervia {
    pub(in crate::ui) fn worktree_removing(&self, path: &Path) -> bool {
        self.git_states.iter().any(|ui| {
            ui.worktrees
                .removing
                .as_ref()
                .is_some_and(|root| path.starts_with(root))
        })
    }

    pub(in crate::ui::git) fn worktree_removal_safe(&self, path: &Path) -> bool {
        self.state.projects.iter().all(|project| {
            project.files.iter().enumerate().all(|(index, file)| {
                if !project.root.join(&file.disk_path).starts_with(path) {
                    return true;
                }
                // Ancestor-folder tabs remain editable after a child context is hidden; close those first.
                !file.dirty()
                    && (project.root.starts_with(path) || !project.open_files.contains(&index))
            })
        })
    }

    pub(in crate::ui::git) fn activate_worktree(
        &mut self,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_removing(&root) {
            return;
        }
        let repository = self.state.project().repository.clone();
        let count = self.state.projects.len();
        let index = self.state.open_directory(root);
        if index == count {
            self.init_project(index, window, cx);
        }
        if let Some(identity) = repository {
            self.state.associate_repository(index, identity, None);
        }
        self.change_project(index, window, cx);
    }

    pub(in crate::ui::git) fn retire_worktree(&mut self, path: &Path, cx: &mut Context<Self>) {
        for ui in &mut self.git_states {
            if let Some(group) = &mut ui.workspace {
                std::sync::Arc::make_mut(group)
                    .repositories
                    .retain(|root| !root.starts_with(path));
                std::sync::Arc::make_mut(group)
                    .identities
                    .retain(|root, _| !root.starts_with(path));
            }
            if let Some(snapshot) = &mut ui.snapshot {
                std::sync::Arc::make_mut(snapshot)
                    .worktrees
                    .retain(|tree| tree.path != path);
            }
        }
        for index in 0..self.state.projects.len() {
            let root = &self.state.projects[index].root;
            if root.starts_with(path) {
                self.file_reads.invalidate_tree(index, Path::new(""));
            } else if let Ok(relative) = path.strip_prefix(root) {
                self.file_reads.invalidate_tree(index, relative);
            }
            if !self.state.projects[index].root.starts_with(path) {
                continue;
            }
            self.stop_project_monitor(index, cx);
            self.explorers[index].definitions.shutdown();
            let connections: Vec<_> = self.state.projects[index]
                .terminal
                .sessions
                .iter_mut()
                .enumerate()
                .filter_map(|(slot, session)| {
                    session.scroll_offset = 0;
                    let connection = session.connection.take();
                    self.terminals[index]
                        .get(slot)
                        .is_none_or(Option::is_none)
                        .then_some(connection)
                        .flatten()
                })
                .collect();
            for id in connections {
                self.end_session(id, cx);
            }
            self.file_reads.select(index);
            for file in &self.state.projects[index].files {
                self.file_reads.invalidate(index, &file.disk_path);
            }
            let connection_error = self.t("connection_error");
            for terminal in &mut self.terminals[index] {
                if let Some(console) = terminal.take()
                    && let Err(error) = console.update(cx, |console, _| console.shutdown())
                {
                    self.state.notice = format!("{connection_error}: {error}");
                }
            }
            self.state.retire_project(index);
            self.git_states[index].cancel_worktree_navigation();
        }
    }

    pub(in crate::ui::git) fn open_worktree(
        &mut self,
        owner: usize,
        tree: Worktree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tree.bare || self.worktree_removing(&tree.path) {
            return;
        }
        let Some(source) = self.git_states[owner].repository.clone() else {
            return;
        };
        let token = self.git_states[owner].worktrees.navigation.begin();
        cx.spawn_in(window, async move |weak, cx| {
            let result: Result<PathBuf, tervia::git::Error> = cx
                .background_executor()
                .spawn(async move {
                    let target = Repository::open(&tree.path)?;
                    if target.common_directory()? != source.common_directory()? {
                        return Err("Worktree registration changed; refresh the list".into());
                    }
                    Ok(target.root)
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this.state.active_project != owner
                    || !this.git_states[owner].worktrees.navigation.current(token)
                {
                    return;
                }
                match result {
                    Ok(root) => this.activate_worktree(root, window, cx),
                    Err(error) => this.git_states[owner].error.action(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn worktree_busy(&self, owner: usize) -> bool {
        self.git_states[owner].busy
            || self.git_mutating
            || self.saving_file
            || self.file_closing
            || self.file_reads.has_reloads()
    }

    pub(super) fn choose_worktree_path(
        &mut self,
        owner: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_busy(owner) {
            return;
        }
        let root = self.git_states[owner]
            .repository
            .as_ref()
            .map(|repo| repo.root.clone())
            .unwrap_or_else(|| self.state.projects[owner].root.clone());
        let parent = root.parent().unwrap_or(&root);
        let destination = cx.prompt_for_new_path(parent, Some("worktree"));
        cx.spawn_in(window, async move |weak, cx| {
            if let Ok(Ok(Some(path))) = destination.await {
                let _ = weak.update_in(cx, |this, window, cx| {
                    this.git_states[owner]
                        .worktrees
                        .path
                        .update(cx, |input, cx| {
                            input.set_value(path.to_string_lossy(), window, cx)
                        });
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn create_worktree(
        &mut self,
        owner: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_busy(owner) {
            return;
        }
        let ui = &self.git_states[owner].worktrees;
        let path = PathBuf::from(ui.path.read(cx).value().as_ref());
        let branch = ui.branch.read(cx).value().to_string();
        let revision = ui.revision.read(cx).value().to_string();
        let checkout = match ui.checkout {
            Checkout::New => WorktreeCheckout::NewBranch {
                name: branch,
                start: revision,
            },
            Checkout::Existing => WorktreeCheckout::ExistingBranch(branch),
            Checkout::Detached => WorktreeCheckout::Detached(revision),
        };
        self.worktree_job(owner, WorktreeOperation::Add { path, checkout }, window, cx);
    }

    pub(in crate::ui::git) fn request_worktree_operation(
        &mut self,
        owner: usize,
        operation: WorktreeOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_busy(owner) {
            return;
        }
        let (title, description) = match &operation {
            WorktreeOperation::Remove(tree) => {
                if !self.worktree_removable(owner, tree) {
                    return;
                }
                if !self.worktree_removal_safe(&tree.path) {
                    let message = self.t("worktree_unsaved");
                    self.git_states[owner].error.action(message);
                    cx.notify();
                    return;
                }
                self.git_states[owner].worktrees.removing = Some(tree.path.clone());
                (
                    self.f(
                        "worktree_remove_target",
                        &[("path", &tree.path.to_string_lossy())],
                    ),
                    self.t("worktree_remove_hint").to_owned(),
                )
            }
            WorktreeOperation::Prune(targets) => {
                let root = self.git_states[owner]
                    .repository
                    .as_ref()
                    .map(|repo| repo.root.as_path())
                    .unwrap_or(&self.state.projects[owner].root);
                (
                    self.f(
                        "worktree_prune_target",
                        &[("repository", &self.state.project_label(owner))],
                    ),
                    prune_description(root, targets, self.t("worktree_prune_hint")),
                )
            }
            _ => {
                self.worktree_job(owner, operation, window, cx);
                return;
            }
        };
        let receiver = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(&description),
            &[self.t("git_cancel"), self.t("git_confirm")],
            cx,
        );
        self.git_mutating = true;
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let accepted = receiver.await == Ok(1);
            let _ = weak.update_in(cx, |this, window, cx| {
                this.git_mutating = false;
                this.git_states[owner].worktrees.removing = None;
                if accepted {
                    if let WorktreeOperation::Remove(tree) = &operation
                        && (!this.worktree_removal_safe(&tree.path)
                            || this.state.project().root.starts_with(&tree.path))
                    {
                        let message = this.t("worktree_unsaved");
                        this.git_states[owner].error.action(message);
                    } else {
                        this.worktree_job(owner, operation, window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn worktree_job(
        &mut self,
        owner: usize,
        operation: WorktreeOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_busy(owner) {
            return;
        }
        let Some(repository) = self.git_states[owner].repository.clone() else {
            return;
        };
        let removed = match &operation {
            WorktreeOperation::Remove(tree) => Some(tree.path.clone()),
            _ => None,
        };
        if let Some(path) = &removed {
            if !self.worktree_removal_safe(path) || self.state.project().root.starts_with(path) {
                return;
            }
            self.git_states[owner].worktrees.removing = Some(path.clone());
        }
        self.git_mutating = true;
        self.git_states[owner].busy = true;
        self.git_states[owner].error.clear();
        let limit = self.git_states[owner].limit;
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let (result, snapshot) = cx
                .background_executor()
                .spawn(async move {
                    let result = repository.execute_worktree(operation);
                    let snapshot = repository.snapshot(limit);
                    (result, snapshot)
                })
                .await;
            let _ = weak.update_in(cx, |this, _, cx| {
                this.git_mutating = false;
                this.git_states[owner].busy = false;
                this.git_states[owner].worktrees.removing = None;
                match result {
                    Ok(output) => {
                        this.git_states[owner].output = output;
                        if let Some(path) = &removed {
                            this.retire_worktree(path, cx);
                        }
                    }
                    Err(error) => this.git_states[owner].error.action(error),
                }
                match snapshot {
                    Ok(snapshot) => {
                        this.git_states[owner].reconcile_snapshot(&snapshot, cx);
                        this.git_states[owner].snapshot = Some(Arc::new(snapshot));
                        this.git_states[owner].error.recovered();
                    }
                    Err(error) => this.git_states[owner].fail_snapshot(error, error::Origin::Probe),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
