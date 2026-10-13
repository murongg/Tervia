use super::super::controls::{icon, icon_button};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::diff::Diff;

pub(super) fn commit_patch(detail: &str) -> &str {
    detail
        .find("\ndiff --git ")
        .map(|offset| &detail[offset + 1..])
        .unwrap_or_default()
}

impl Tervia {
    pub(super) fn git_viewer(&self, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let mut header = div()
            .h(px(36.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(14.0))
            .gap(px(6.0))
            .border_b_1()
            .border_color(p.border)
            .bg(p.toolbar);
        if let Some(Detail::File(file, side)) = &ui.selected {
            let path = file.path.clone();
            header = header
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .child(file.path.to_string_lossy().into_owned()),
                )
                .child(div().text_size(px(10.0)).text_color(p.muted).child(self.t(
                    if *side == Side::Index {
                        "staged"
                    } else {
                        "unstaged"
                    },
                )))
                .child(
                    icon_button(
                        "git-edit-file",
                        IconName::SquarePen,
                        self.t("edit_in_files"),
                        p,
                        cx,
                    )
                    .accessibility_label(self.t("edit_in_files"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_git_file(project, path.clone(), window, cx)
                    })),
                );
        } else if let Some(Detail::Conflict(path)) = &ui.selected {
            let path = path.clone();
            header = header
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_size(px(11.0))
                        .child(self.t("git_conflict_hint")),
                )
                .child(
                    label_button("git-edit-conflict", self.t("git_edit_result"), p).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.open_git_file(project, path.clone(), window, cx)
                        }),
                    ),
                );
        } else {
            header = header.child(
                div()
                    .flex_1()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child("Diff"),
            );
        }
        let mut tools = div()
            .h(px(32.0))
            .flex_shrink_0()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(3.0))
            .bg(p.toolbar)
            .border_b_1()
            .border_color(p.border)
            .child(div().flex_1());
        for (index, mode, key) in [
            (0usize, DiffMode::Unified, "diff_inline"),
            (1, DiffMode::Split, "diff_split"),
        ] {
            tools = tools.child(
                label_button(("real-diff-mode", index), self.t(key), p)
                    .h(px(24.0))
                    .text_size(px(11.0))
                    .bg(if ui.diff.read(cx).mode() == mode {
                        p.selected
                    } else {
                        p.toolbar
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.git_states[project]
                            .diff
                            .update(cx, |state, cx| state.set_mode(mode, cx));
                        cx.notify();
                    })),
            );
        }
        tools = tools.child(
            icon_button("git-copy-detail", IconName::Copy, self.t("git_copy"), p, cx)
                .accessibility_label(self.t("git_copy"))
                .disabled(ui.detail.is_empty())
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        this.git_states[project].detail.clone(),
                    ))
                })),
        );
        let content = if ui.selected.is_some()
            && !ui.diff.read(cx).files().is_empty()
            && !matches!(ui.selected, Some(Detail::Conflict(_)))
        {
            Diff::new(&ui.diff)
                .bg(p.background)
                .border_0()
                .into_any_element()
        } else if ui.detail.is_empty() {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .child(icon(
                    if ui.detail_loading {
                        IconName::FileDiff
                    } else {
                        IconName::CircleCheck
                    },
                    p.muted,
                ))
                .child(
                    div().text_size(px(13.0)).text_color(p.secondary).child(
                        self.t(if ui.detail_loading {
                            "git_running"
                        } else if ui.mode == Mode::Changes
                            && ui
                                .snapshot
                                .as_ref()
                                .is_some_and(|snapshot| snapshot.status.files.is_empty())
                        {
                            "git_diff_empty_title"
                        } else {
                            "git_select_hint"
                        }),
                    ),
                )
                .when(
                    !ui.detail_loading
                        && ui.mode == Mode::Changes
                        && ui
                            .snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.status.files.is_empty()),
                    |view| {
                        view.child(
                            div()
                                .max_w(px(320.0))
                                .text_center()
                                .text_size(px(12.0))
                                .text_color(p.muted)
                                .child(self.t("git_diff_empty_hint")),
                        )
                    },
                )
                .into_any_element()
        } else if ui.raw_lines.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(p.muted)
                .child(self.t("git_no_text_diff"))
                .into_any_element()
        } else {
            let lines = ui.raw_lines.clone();
            div()
                .id("git-raw-detail")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_x_scroll()
                .p(px(12.0))
                .font_family(self.appearance.font_family.clone())
                .text_size(px(self.appearance.code_size as f32))
                .child(
                    uniform_list(
                        ("git-raw-lines", project),
                        lines.len(),
                        move |range: std::ops::Range<usize>, _, _| {
                            range
                                .map(|index| {
                                    div()
                                        .h(px(23.0))
                                        .min_w(px(600.0))
                                        .child(lines[index].clone())
                                })
                                .collect()
                        },
                    )
                    .flex_1()
                    .min_h_0()
                    .track_scroll(&ui.raw_scroll),
                )
                .into_any_element()
        };
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .size_full()
            .bg(p.background)
            .child(header)
            .child(tools)
            .child(self.git_hunks(cx))
            .when(matches!(ui.selected, Some(Detail::Commit(_))), |viewer| {
                let lines = ui.metadata_lines.clone();
                viewer.child(
                    div()
                        .h(px((lines.len() as f32 * 18.0 + 20.0).min(105.0)))
                        .flex_shrink_0()
                        .px(px(16.0))
                        .py(px(10.0))
                        .text_size(px(11.0))
                        .text_color(p.secondary)
                        .child(
                            uniform_list(
                                ("git-commit-metadata", project),
                                lines.len(),
                                move |range: std::ops::Range<usize>, _, _| {
                                    range
                                        .map(|index| div().h(px(18.0)).child(lines[index].clone()))
                                        .collect()
                                },
                            )
                            .size_full()
                            .track_scroll(&ui.metadata_scroll),
                        ),
                )
            })
            .child(content)
    }

    fn git_hunks(&self, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let Some(Detail::File(file, side)) = &ui.selected else {
            return div();
        };
        if file.conflicted || (*side == Side::Worktree && file.worktree == '?') {
            return div();
        }
        let expected: std::sync::Arc<str> = ui.detail.as_str().into();
        let mut bar = div()
            .id("git-hunks")
            .flex()
            .gap(px(5.0))
            .px(px(12.0))
            .py(px(5.0))
            .bg(self.git_p().toolbar)
            .overflow_x_scroll();
        for (index, _) in ui
            .detail
            .lines()
            .filter(|line| line.starts_with("@@ "))
            .enumerate()
        {
            let operation = Operation::Hunk {
                file: file.clone(),
                side: *side,
                index,
                expected: expected.clone(),
            };
            bar = bar.child(
                self.git_action(
                    ("git-hunk", index),
                    if *side == Side::Index {
                        "git_unstage_hunk"
                    } else {
                        "git_stage_hunk"
                    },
                    operation,
                    cx,
                )
                .label(format!(
                    "{} {}",
                    self.t(if *side == Side::Index {
                        "git_unstage_hunk"
                    } else {
                        "git_stage_hunk"
                    }),
                    index + 1
                )),
            );
        }
        div().flex_shrink_0().child(bar)
    }
}
