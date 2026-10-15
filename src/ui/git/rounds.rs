use super::*;
use tervia::git::ReviewKey;

impl Tervia {
    pub(super) fn toggle_reviewed(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.git_states[project];
        if ui.review.busy || ui.busy || ui.detail_loading {
            return;
        }
        let (Some(repository), Some(Detail::File(file, side))) =
            (ui.repository.clone(), ui.selected.clone())
        else {
            return;
        };
        if file.conflicted {
            return;
        }
        let expected = ui.detail.clone();
        let selected = ui.selected.clone();
        ui.review.busy = true;
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let files = [(file.clone(), side)];
                    let before = repository.review_tokens(&files)?;
                    let patch = repository.diff(&file, side)?;
                    let after = repository.review_tokens(&files)?;
                    if before != after || patch != expected {
                        return Err(tervia::git::Error::from(
                            "Diff changed; refresh before marking it reviewed",
                        ));
                    }
                    Ok((
                        ReviewKey::new(file.path, side),
                        *after.values().next().ok_or("Review version unavailable")?,
                        patch,
                    ))
                })
                .await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let ui = &mut this.git_states[project];
                ui.review.busy = false;
                match result {
                    Ok((key, token, patch)) if ui.selected == selected && ui.detail == patch => {
                        ui.review.marks.toggle(key, token)
                    }
                    Ok(_) => {}
                    Err(error) => ui.error.action(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn capture_checkpoint(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.git_states[project];
        if ui.review.busy || ui.busy || self.git_mutating {
            return;
        }
        let Some(repository) = ui.repository.clone() else {
            return;
        };
        ui.review.busy = true;
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { repository.checkpoint() })
                .await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let ui = &mut this.git_states[project];
                ui.review.busy = false;
                match result {
                    Ok(checkpoint) => {
                        ui.review.checkpoint = Some(checkpoint);
                        this.state.notice = "git_checkpoint_captured".into();
                    }
                    Err(error) => ui.error.action(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn show_checkpoint(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(checkpoint) = self.git_states[project].review.checkpoint.clone() else {
            return;
        };
        let ui = &mut self.git_states[project];
        if ui.review.busy || ui.busy {
            return;
        }
        ui.mode = Mode::Changes;
        ui.review.focused = true;
        self.git_detail(project, Detail::Checkpoint(checkpoint), window, cx);
        cx.notify();
    }

    pub(super) fn return_working_diff(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.git_states[project];
        ui.review.focused = false;
        let detail = ui
            .snapshot
            .as_ref()
            .and_then(|snapshot| navigation::choose_detail(Mode::Changes, None, snapshot));
        if let Some(detail) = detail {
            self.git_detail(project, detail, window, cx);
        } else {
            self.git_states[project].clear_detail(cx);
        }
        cx.notify();
    }

    pub(super) fn refresh_review_marks(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &self.git_states[project];
        let Some(repository) = ui.repository.clone() else {
            return;
        };
        let Some(snapshot) = &ui.snapshot else {
            return;
        };
        let keys: Vec<_> = ui.review.marks.keys().cloned().collect();
        let revision = ui.review.marks.revision();
        if keys.is_empty() {
            return;
        }
        let files: Vec<_> = keys
            .iter()
            .filter_map(|key| {
                snapshot
                    .status
                    .files
                    .iter()
                    .find(|file| file.path == key.path)
                    .map(|file| {
                        (
                            file.clone(),
                            if key.staged {
                                Side::Index
                            } else {
                                Side::Worktree
                            },
                        )
                    })
            })
            .collect();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { repository.review_tokens(&files) })
                .await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let ui = &mut this.git_states[project];
                if ui.review.marks.revision() == revision {
                    // The same keys can be reviewed again while an older validation is still running.
                    ui.review
                        .marks
                        .reconcile_at(revision, &result.unwrap_or_default());
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
