use super::*;
use gpui_kit::component::diff::{DiffLineRange, DiffSide};
use std::collections::VecDeque;

pub(super) struct CachedDiff {
    target: Detail,
    pub patch: String,
    pub diff: Entity<DiffState>,
    pub file: Option<SharedString>,
    pub range: Option<DiffLineRange>,
}

pub(super) struct FileJump {
    origin: usize,
    path: PathBuf,
    selection: usize,
    line: usize,
    range: Option<DiffLineRange>,
    diff: Entity<DiffState>,
    patch: String,
}

#[derive(Default)]
pub(super) struct Review {
    pub commit_expanded: bool,
    pub focused: bool,
    cache: VecDeque<CachedDiff>,
    pub restore: Option<CachedDiff>,
    pub files: Vec<Detail>,
    pub range: Option<DiffLineRange>,
    jump: Option<FileJump>,
    observation: Option<Subscription>,
}

pub(super) fn files(snapshot: &Snapshot) -> Vec<Detail> {
    changes::change_rows(
        &snapshot.status.files,
        &Default::default(),
        &Default::default(),
    )
    .into_iter()
    .filter_map(|row| {
        let changes::ChangeRow::File { group, index, .. } = row else {
            return None;
        };
        let file = &snapshot.status.files[index];
        Some(if group == 0 {
            Detail::Conflict(file.path.clone())
        } else {
            Detail::File(
                file.clone(),
                if group == 2 {
                    Side::Index
                } else {
                    Side::Worktree
                },
            )
        })
    })
    .collect()
}

fn same_target(left: &Detail, right: &Detail) -> bool {
    match (left, right) {
        (Detail::File(left, a), Detail::File(right, b)) => left.path == right.path && a == b,
        _ => left == right,
    }
}

impl GitUi {
    pub(super) fn observe_diff(&mut self, cx: &mut Context<Tervia>) {
        self.review.observation = Some(cx.observe(&self.diff, |this, diff, cx| {
            // Toolbar focus clears the live selection before click; keep its source bookmark.
            if let Some(range) = diff.read(cx).selected_lines()
                && let Some(ui) = this.git_states.iter_mut().find(|ui| ui.diff == diff)
            {
                ui.review.range = Some(range);
            }
            cx.notify();
        }));
    }

    pub(super) fn remember_diff(&mut self) {
        let Some(target) = self.selected.clone() else {
            return;
        };
        if self.detail.is_empty() {
            return;
        }
        self.review
            .cache
            .retain(|entry| !same_target(&entry.target, &target));
        self.review.cache.push_back(CachedDiff {
            target,
            patch: self.detail.clone(),
            diff: self.diff.clone(),
            file: self.commit_file.clone(),
            range: self.review.range.clone(),
        });
        // Bound retained documents; their native viewer owns scroll, folds and selection.
        while self.review.cache.len() > 8 {
            self.review.cache.pop_front();
        }
    }

    pub(super) fn prepare_diff(&mut self, target: Option<&Detail>, cx: &mut Context<Tervia>) {
        self.remember_diff();
        self.review.range = None;
        let mode = self.diff.read(cx).mode();
        self.review.restore = target.and_then(|target| {
            let index = self
                .review
                .cache
                .iter()
                .position(|entry| same_target(&entry.target, target))?;
            self.review.cache.remove(index)
        });
        self.diff = cx.new(|cx| DiffState::new([], cx).with_mode(mode));
        self.observe_diff(cx);
    }

    pub(super) fn file_position(&self, cx: &App) -> (usize, usize) {
        if self.mode == Mode::Changes {
            let files = &self.review.files;
            let index = self
                .selected
                .as_ref()
                .and_then(|target| files.iter().position(|file| same_target(file, target)));
            (index.unwrap_or(0), files.len())
        } else {
            let files = self.diff.read(cx).files();
            let index = self
                .commit_file
                .as_ref()
                .and_then(|path| files.iter().position(|file| file.path() == path));
            (index.unwrap_or(0), files.len())
        }
    }
}

impl Tervia {
    pub(super) fn navigate_git_file(
        &mut self,
        project: usize,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &self.git_states[project];
        let (index, count) = ui.file_position(cx);
        let target = if forward {
            index.checked_add(1).filter(|index| *index < count)
        } else {
            index.checked_sub(1)
        };
        let Some(target) = target else { return };
        if ui.mode == Mode::Changes {
            let detail = ui.review.files[target].clone();
            self.git_detail(project, detail.clone(), window, cx);
            self.git_states[project].reveal_change(&detail);
        } else {
            let ui = &mut self.git_states[project];
            let path = ui.diff.read(cx).files()[target].path().clone();
            ui.commit_file = Some(path.clone());
            ui.commit_files_scroll
                .scroll_to_item(target, ScrollStrategy::Center);
            ui.diff.update(cx, |state, cx| {
                state.set_file_collapsed(&path, false, cx);
                state.scroll_to_file(&path, cx);
            });
        }
        cx.notify();
    }

    pub(in crate::ui) fn reveal_git_file(
        &mut self,
        project: usize,
        selection: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(jump) = &self.git_states[project].review.jump else {
            return;
        };
        if jump.selection != selection
            || jump.path != self.state.projects[project].files[index].disk_path
        {
            return;
        }
        let line = jump.line.saturating_sub(1) as u32;
        if self.state.projects[project].files[index].is_image() {
            return;
        }
        self.show_file_source(project, index);
        self.editors[project][index].update(cx, |editor, cx| {
            editor.set_cursor_position(
                gpui_kit::component::input::Position::new(line, 0),
                window,
                cx,
            );
        });
    }

    pub(in crate::ui) fn git_return_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        let project = self.state.active_project;
        let jump = self.git_states[project].review.jump.as_ref()?;
        if self.state.projects[project].current_file()?.disk_path != jump.path {
            return None;
        }
        Some(
            label_button(
                "git-return-to-diff",
                self.t("git_return_to_diff"),
                self.git_p(),
            )
            .debug_selector(|| "git-return-to-diff".into())
            .icon(super::super::controls::icon(
                gpui_kit::assets::IconName::ArrowLeft,
                self.git_p().secondary,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                let Some(jump) = this.git_states[project].review.jump.take() else {
                    return;
                };
                if this.state.active_project != jump.origin {
                    this.change_project(jump.origin, window, cx);
                }
                let ui = &mut this.git_states[jump.origin];
                if ui.diff == jump.diff && ui.detail == jump.patch {
                    ui.diff
                        .update(cx, |state, cx| state.set_selected_lines(jump.range, cx));
                }
                this.change_tab(Tab::Git, window, cx);
            })),
        )
    }

    pub(super) fn remember_git_file_jump(
        &mut self,
        origin: usize,
        owner: usize,
        path: PathBuf,
        range: Option<DiffLineRange>,
    ) {
        let ui = &self.git_states[origin];
        let range = range.filter(|range| range.path().as_str() == path.to_string_lossy());
        let line = range
            .as_ref()
            .filter(|range| range.side() == DiffSide::Modified)
            .map_or(1, |range| range.start());
        let diff = ui.diff.clone();
        let patch = ui.detail.clone();
        self.git_states[owner].review.jump = Some(FileJump {
            origin,
            path,
            line,
            selection: self.file_reads.selection(owner),
            range,
            diff,
            patch,
        });
    }
}
