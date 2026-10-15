use super::*;

impl Tervia {
    pub(in crate::ui) fn request_project_close(
        &mut self,
        owner: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.closing || !self.state.project_rows().contains(&owner) {
            return;
        }
        if !self.can_close_project() {
            self.state.notice = self.t("project_busy").into();
            cx.notify();
            return;
        }
        let members = self.state.project_members(owner);
        if members
            .iter()
            .any(|index| self.worktree_removing(&self.state.projects[*index].root))
        {
            return;
        }
        let terminals = members
            .iter()
            .map(|index| self.terminals[*index].iter().flatten().count())
            .sum::<usize>();
        if terminals == 0 {
            self.finish_project_close(owner, window, cx);
            return;
        }
        let hint = self.f(
            "project_close_hint",
            &[
                ("path", &self.state.projects[owner].path),
                ("count", &terminals.to_string()),
            ],
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            self.t("project_close"),
            Some(&hint),
            &[self.t("git_cancel"), self.t("git_confirm")],
            cx,
        );
        self.navigation.closing = true;
        cx.spawn_in(window, async move |weak, cx| {
            let answer = answer.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.navigation.closing = false;
                if answer == Ok(1) {
                    let current = this.state.project_members(owner);
                    let count = current
                        .iter()
                        .map(|index| this.terminals[*index].iter().flatten().count())
                        .sum::<usize>();
                    if current != members || count != terminals {
                        this.request_project_close(owner, window, cx);
                    } else {
                        this.finish_project_close(owner, window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn can_close_project(&self) -> bool {
        !self.git_mutating
            && !self.saving_file
            && !self.file_closing
            && !self.file_reads.has_reloads()
    }

    fn finish_project_close(&mut self, owner: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_close_project() {
            self.state.notice = self.t("project_busy").into();
            cx.notify();
            return;
        }
        self.capture_reading_positions(cx);
        let previous = self.state.clone();
        let Some(members) = self.state.close_project(owner) else {
            return;
        };
        // Persist the closed catalog and drafts before terminating any owned process.
        if let Some(store) = &mut self.workspace
            && let Err(error) = store.save(&self.state)
        {
            self.state = previous;
            self.state.notice = format!("{}\n{error}", self.t("workspace_error"));
            cx.notify();
            return;
        }
        if self.navigation.switcher.open {
            self.close_project_switcher(window, cx);
        }
        self.navigation.cancel();
        for index in members {
            self.cancel_repository_scan(index);
            self.git_states[index].close_branch_picker();
            self.git_states[index].cancel_worktree_navigation();
            self.file_reads.select(index);
            self.stop_project_monitor(index, cx);
            for terminal in &mut self.terminals[index] {
                if let Some(console) = terminal.take() {
                    console.update(cx, |console, _| console.shutdown());
                }
            }
        }
        if !self.state.project_rows().is_empty() {
            self.change_project(self.state.active_project, window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(in crate::ui) fn reopen_closed_project(
        &mut self,
        owner: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reopen_project_target(owner, None, window, cx);
    }

    pub(super) fn reopen_project_target(
        &mut self,
        owner: usize,
        target: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.project_closed(owner) {
            return;
        }
        let target = target.unwrap_or_else(|| self.state.project_target(owner));
        let root = self.state.projects[target].root.clone();
        self.navigation.cancel();
        let generation = self.navigation.generation;
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { tervia::files::directory(&root, std::path::Path::new("")) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this.navigation.generation != generation || !this.state.project_closed(owner) {
                    return;
                }
                match result {
                    Ok(_) => {
                        this.state.reopen_project(owner);
                        this.change_project(target, window, cx);
                        for index in this.state.project_members(owner) {
                            this.restore_documents(index, window, cx);
                        }
                        this.rescan_repositories(owner, window, cx);
                    }
                    Err(error) => {
                        this.state.notice = error;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}
