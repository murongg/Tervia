use super::super::controls::{icon, icon_button};
use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::diff::{Diff, DiffSide};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::scroll::ScrollableElement as _;

pub(super) fn commit_patch(detail: &str) -> &str {
    detail
        .find("\ndiff --git ")
        .map(|offset| &detail[offset + 1..])
        .unwrap_or_default()
}

pub(super) fn hunk_lines(header: &str) -> Option<(DiffSide, usize, usize)> {
    let mut fields = header.strip_prefix("@@ ")?.split_whitespace();
    let parse = |field: &str, prefix| -> Option<(usize, usize)> {
        let field = field.strip_prefix(prefix)?;
        let (start, count) = field.split_once(',').unwrap_or((field, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    };
    let original = parse(fields.next()?, '-')?;
    let modified = parse(fields.next()?, '+')?;
    if fields.next()? != "@@" {
        return None;
    }
    // A hunk with no remaining lines must use its original line numbers.
    let (side, (start, count)) = if modified.1 == 0 {
        (DiffSide::Original, original)
    } else {
        (DiffSide::Modified, modified)
    };
    Some((side, start, start.checked_add(count.checked_sub(1)?)?))
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
            // Button focus clears the live text selection before its click callback.
            let range = ui
                .diff
                .read(cx)
                .selected_lines()
                .or_else(|| ui.review.range.clone());
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
                    } else if file.worktree == '?' {
                        "untracked"
                    } else {
                        "unstaged"
                    },
                )))
                .child(
                    icon_button(
                        "git-edit-file",
                        IconName::FileCode,
                        self.t("git_open_full_file"),
                        p,
                        cx,
                    )
                    .debug_selector(|| "git-open-full-file".into())
                    .accessibility_label(self.t("git_open_full_file"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_git_file(project, path.clone(), range.clone(), window, cx)
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
                            this.open_git_file(project, path.clone(), None, window, cx)
                        }),
                    ),
                );
        } else if let Some(Detail::Commit(hash)) = &ui.selected {
            let copied_hash = hash.clone();
            header = header
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(
                            ui.commit
                                .as_ref()
                                .map_or_else(|| hash.clone(), |commit| commit.message.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.accent)
                        .font_family(self.appearance.font_family.clone())
                        .child(hash.chars().take(7).collect::<String>()),
                )
                .child(
                    icon_button("git-copy-commit", IconName::Copy, self.t("git_copy"), p, cx)
                        .accessibility_label(self.t("git_copy"))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copied_hash.clone()));
                        })),
                );
        } else {
            header = header.child(
                div()
                    .flex_1()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(if matches!(ui.selected, Some(Detail::Checkpoint(_))) {
                        self.t("git_checkpoint_title")
                    } else {
                        "Diff"
                    }),
            );
        }
        if matches!(ui.selected, Some(Detail::Checkpoint(_))) {
            header = header
                .debug_selector(|| "git-checkpoint-view".into())
                .child(
                    label_button("git-checkpoint-back", self.t("git_working_copy"), p).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.return_working_diff(project, window, cx)
                        }),
                    ),
                )
                .child(
                    label_button(
                        "git-checkpoint-refresh",
                        self.t("git_checkpoint_compare"),
                        p,
                    )
                    .disabled(ui.detail_loading || ui.review.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_checkpoint(project, window, cx)
                    })),
                );
        } else if let Some(Detail::File(file, side)) = &ui.selected {
            let reviewed = ui
                .review
                .marks
                .contains(&tervia::git::ReviewKey::new(file.path.clone(), *side));
            header =
                header.child(
                    label_button(
                        "git-mark-reviewed",
                        self.t(if reviewed {
                            "git_reviewed"
                        } else {
                            "git_mark_reviewed"
                        }),
                        p,
                    )
                    .debug_selector(|| "git-mark-reviewed".into())
                    .disabled(ui.detail_loading || ui.busy || ui.review.busy || file.conflicted)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_reviewed(project, window, cx)
                    })),
                );
        }
        let mut tools = div()
            .min_h(px(32.0))
            .flex_shrink_0()
            .px(px(10.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(3.0))
            .bg(p.toolbar)
            .border_b_1()
            .border_color(p.border);
        let (file_index, file_count) = ui.file_position(cx);
        for (id, key, glyph, forward) in [
            (
                "git-previous-file",
                "git_previous_file",
                IconName::ChevronLeft,
                false,
            ),
            (
                "git-next-file",
                "git_next_file",
                IconName::ChevronRight,
                true,
            ),
        ] {
            tools = tools.child(
                icon_button(id, glyph, self.t(key), p, cx)
                    .debug_selector(move || id.into())
                    .accessibility_label(self.t(key))
                    .disabled(
                        file_count == 0
                            || if forward {
                                file_index + 1 >= file_count
                            } else {
                                file_index == 0
                            },
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_git_file(project, forward, window, cx)
                    })),
            );
        }
        tools = tools.child(div().text_size(px(11.0)).text_color(p.muted).child(format!(
            "{} / {file_count}",
            if file_count == 0 { 0 } else { file_index + 1 }
        )));
        let has_changes = !ui.detail_loading
            && ui
                .diff
                .read(cx)
                .files()
                .iter()
                .any(|file| file.has_changes());
        for (id, key, glyph, forward) in [
            (
                "git-previous-change",
                "git_previous_change",
                IconName::ArrowUp,
                false,
            ),
            (
                "git-next-change",
                "git_next_change",
                IconName::ArrowDown,
                true,
            ),
        ] {
            tools = tools.child(
                icon_button(id, glyph, self.t(key), p, cx)
                    .debug_selector(move || id.into())
                    .accessibility_label(self.t(key))
                    .disabled(!has_changes)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.git_states[project].diff.update(cx, |state, cx| {
                            if forward {
                                state.next_change(cx);
                            } else {
                                state.previous_change(cx);
                            }
                        });
                        cx.notify();
                    })),
            );
        }
        tools = tools.child(div().flex_1());
        tools = tools
            .child(
                label_button(
                    "git-capture-checkpoint",
                    self.t("git_checkpoint_capture"),
                    p,
                )
                .debug_selector(|| "git-capture-checkpoint".into())
                .h(px(24.0))
                .text_size(px(11.0))
                .disabled(
                    ui.repository.is_none()
                        || ui.busy
                        || ui.review.busy
                        || self.git_mutating
                        || ui
                            .snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.status.operation.is_some()),
                )
                .tooltip(self.t("git_checkpoint_hint"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.capture_checkpoint(project, window, cx)
                })),
            )
            .when(
                ui.review.checkpoint.is_some()
                    && !matches!(ui.selected, Some(Detail::Checkpoint(_))),
                |tools| {
                    tools.child(
                        label_button(
                            "git-compare-checkpoint",
                            self.t("git_checkpoint_compare"),
                            p,
                        )
                        .h(px(24.0))
                        .text_size(px(11.0))
                        .disabled(ui.busy || ui.review.busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.show_checkpoint(project, window, cx),
                        )),
                    )
                },
            );
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
            icon_button(
                "git-focus-diff",
                if ui.review.focused {
                    IconName::Minimize
                } else {
                    IconName::Maximize
                },
                self.t(if ui.review.focused {
                    "git_restore_layout"
                } else {
                    "git_focus_diff"
                }),
                p,
                cx,
            )
            .debug_selector(|| "git-focus-diff".into())
            .accessibility_label(self.t(if ui.review.focused {
                "git_restore_layout"
            } else {
                "git_focus_diff"
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                let review = &mut this.git_states[project].review;
                review.focused = !review.focused;
                cx.notify();
            })),
        );
        tools = tools.child({
            let context = super::super::context::diff_context(ui.diff.read(cx), cx);
            let pressed_context = context.clone();
            label_button("git-copy-code-context", self.t("copy_code_context"), p)
                .h(px(24.0))
                .text_size(px(11.0))
                .debug_selector(|| "git-copy-code-context".into())
                .disabled(context.is_none() && ui.review.copying.is_none())
                .tooltip(
                    tervia::platform::Platform::CURRENT.hint(self.t("copy_code_context_shortcut")),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        // Window text selection clears on press; retain only this copy gesture's source.
                        this.git_states[project].review.copying = pressed_context.clone();
                        cx.notify();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.git_states[project].review.copying = None;
                        cx.notify();
                    }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(context) = this.git_states[project]
                        .review
                        .copying
                        .take()
                        .or_else(|| context.clone())
                    {
                        this.copy_context(context, cx);
                    }
                }))
        });
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
            let diff = self.git_diff(project, cx);
            if matches!(ui.selected, Some(Detail::Commit(_))) && !ui.review.focused {
                h_resizable(("git-commit-diff-layout", project))
                    .child(
                        resizable_panel()
                            .size(px(210.0))
                            .size_range(px(150.0)..px(300.0))
                            .flex_none()
                            .child(self.git_commit_files(project, cx)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(250.0)..Pixels::MAX)
                            .child(diff),
                    )
                    .into_any_element()
            } else {
                diff.into_any_element()
            }
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
                        } else if matches!(ui.selected, Some(Detail::Checkpoint(_))) {
                            "git_checkpoint_empty"
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
                .overflow_x_scrollbar()
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
                    .scrollbar(&ui.raw_scroll),
                )
                .into_any_element()
        };
        div()
            .debug_selector(|| "git-diff-viewer".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .size_full()
            .bg(p.background)
            .child(header)
            .child(tools)
            .when_some(ui.commit.as_ref(), |viewer, commit| {
                viewer.child(self.git_commit_metadata(project, commit, cx))
            })
            .when(
                matches!(ui.selected, Some(Detail::Commit(_)))
                    && !ui.collapsed_sections.contains("git_commit_metadata"),
                |viewer| {
                    let lines = ui.metadata_lines.clone();
                    viewer.child(
                        div()
                            .id("git-commit-metadata")
                            .debug_selector(|| "git-commit-metadata".into())
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
                                            .map(|index| {
                                                div().h(px(18.0)).child(lines[index].clone())
                                            })
                                            .collect()
                                    },
                                )
                                .size_full()
                                .scrollbar(&ui.metadata_scroll),
                            ),
                    )
                },
            )
            .child(content)
    }

    fn git_diff(&self, project: usize, cx: &mut Context<Self>) -> Diff {
        let ui = &self.git_states[project];
        let p = self.git_p();
        let diff = Diff::new(&ui.diff).bg(p.background).border_0();
        let Some(Detail::File(file, side)) = &ui.selected else {
            return diff;
        };
        if file.conflicted || (*side == Side::Worktree && file.worktree == '?') {
            return diff;
        }
        let Some(path) = ui
            .diff
            .read(cx)
            .files()
            .first()
            .map(|file| file.path().clone())
        else {
            return diff;
        };
        let key = if *side == Side::Index {
            "git_unstage_hunk"
        } else {
            "git_stage_hunk"
        };
        let hint = self.t(if *side == Side::Index {
            "git_unstage_hunks_hint"
        } else {
            "git_stage_hunks_hint"
        });
        let action = self.t(key);
        let expected: std::sync::Arc<str> = ui.detail.as_str().into();
        // Stage indices belong to the raw patch, before an invalid header is skipped.
        let actions: Vec<_> = ui
            .detail
            .lines()
            .filter(|line| line.starts_with("@@ "))
            .enumerate()
            .filter_map(|(index, header)| {
                let (line_side, start, end) = hunk_lines(header)?;
                let range = if start == end {
                    start.to_string()
                } else {
                    format!("{start}–{end}")
                };
                let lines = self.appearance.language.format(
                    if line_side == DiffSide::Original {
                        "git_hunk_original_lines"
                    } else {
                        "git_hunk_lines"
                    },
                    &[("range", &range)],
                );
                let label = self.appearance.language.format(
                    "git_hunk_action_label",
                    &[("action", action), ("lines", &lines)],
                );
                let operation = Operation::Hunk {
                    file: file.clone(),
                    side: *side,
                    index,
                    expected: expected.clone(),
                };
                Some((index, operation, label))
            })
            .collect();
        let owner = cx.entity().downgrade();
        let glyph = if *side == Side::Index {
            IconName::Minus
        } else {
            IconName::Plus
        };
        diff.render_hunk_suffix(move |file, index, _, cx| {
            if file.path() != &path {
                return div();
            }
            let Some((_, operation, label)) = actions.iter().find(|action| action.0 == index)
            else {
                return div();
            };
            let button = owner.update(cx, |this, cx| {
                let button = icon_button(
                    ("git-hunk", index),
                    glyph,
                    format!("{label}\n{hint}"),
                    p,
                    cx,
                )
                .debug_selector(move || format!("git-hunk-{index}"))
                .accessibility_label(label.clone());
                this.git_button(project, button, key, operation.clone(), cx)
            });
            div()
                .flex()
                .flex_shrink_0()
                .font_family(tervia::platform::Platform::CURRENT.ui_font())
                .text_size(px(12.0))
                .when_some(button.ok(), |row, button| row.child(button))
        })
    }
}
