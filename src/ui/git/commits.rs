use super::super::controls::icon_button;
use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::diff::DiffFileStatus;

impl Tervia {
    pub(super) fn git_commit_files(&self, project: usize, cx: &mut Context<Self>) -> Div {
        let ui = &self.git_states[project];
        let p = self.git_p();
        let files: Vec<_> = ui
            .diff
            .read(cx)
            .files()
            .iter()
            .map(|file| {
                (
                    file.path().clone(),
                    file.status(),
                    file.additions(),
                    file.deletions(),
                )
            })
            .collect();
        let count = files.len().to_string();
        let rows = uniform_list(
            ("git-commit-files", project),
            files.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| {
                        let (path, status, added, removed) = &files[index];
                        let target = path.clone();
                        let (mark, color) = match status {
                            DiffFileStatus::Added => ("A", p.green),
                            DiffFileStatus::Deleted => ("D", p.red),
                            DiffFileStatus::Renamed => ("R", p.accent),
                            DiffFileStatus::Copied => ("C", p.accent),
                            DiffFileStatus::Conflicted => ("U", p.red),
                            DiffFileStatus::Modified => ("M", p.accent),
                            _ => ("", p.muted),
                        };
                        list_row(
                            ("git-commit-file", index),
                            this.git_states[project].commit_file.as_ref() == Some(path),
                            p,
                            cx,
                        )
                        .debug_selector(move || format!("git-commit-file-{index}"))
                        .rounded(px(0.0))
                        .accessibility_label(path.clone())
                        .tooltip(path.clone())
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(11.0))
                                .child(div().text_color(color).child(mark))
                                .child(div().flex_1().min_w_0().text_ellipsis().child(path.clone()))
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(p.green)
                                        .child(format!("+{added}")),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(p.red)
                                        .child(format!("−{removed}")),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let ui = &mut this.git_states[project];
                            ui.commit_file = Some(target.clone());
                            ui.diff.update(cx, |state, cx| {
                                state.set_file_collapsed(&target, false, cx);
                                state.scroll_to_file(&target, cx);
                            });
                            cx.notify();
                        }))
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .scrollbar(&ui.commit_files_scroll);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .h(px(30.0))
                    .flex_shrink_0()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.f("changed_files", &[("count", &count)])),
            )
            .child(rows)
    }

    pub(super) fn git_commit_metadata(
        &self,
        project: usize,
        commit: &tervia::git::Commit,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.git_p();
        let expanded = !self.git_states[project]
            .collapsed_sections
            .contains("git_commit_metadata");
        let mut metadata = div()
            .flex_shrink_0()
            .min_h(px(32.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(p.border)
            .text_size(px(11.0))
            .text_color(p.secondary)
            .child(div().text_color(p.muted).child(self.t("git_author")))
            .child(
                div()
                    .max_w(px(140.0))
                    .text_ellipsis()
                    .child(commit.author.clone()),
            )
            .child(
                div().text_color(p.muted).child(
                    commit
                        .when
                        .replace('T', " ")
                        .chars()
                        .take(19)
                        .collect::<String>(),
                ),
            )
            .child(
                div()
                    .ml(px(6.0))
                    .text_color(p.muted)
                    .child(self.t("git_parents")),
            );
        if commit.parents.is_empty() {
            metadata = metadata.child(div().text_color(p.muted).child(self.t("git_root_commit")));
        }
        for (index, parent) in commit.parents.split_whitespace().enumerate() {
            let hash = parent.to_owned();
            metadata = metadata.child(
                label_button(
                    ("git-parent-commit", index),
                    &parent.chars().take(7).collect::<String>(),
                    p,
                    cx,
                )
                .h(px(22.0))
                .text_size(px(11.0))
                .font_family(self.appearance.font_family.clone())
                .text_color(p.accent)
                .tooltip(parent.to_owned())
                .accessibility_label(format!("{} {parent}", self.t("git_parents")))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_git_commit(project, hash.clone(), window, cx);
                })),
            );
        }
        metadata.child(div().flex_1()).child(
            icon_button(
                "git-toggle-commit-metadata",
                if expanded {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                },
                self.t("git_commit_metadata"),
                p,
                cx,
            )
            .debug_selector(|| "git-toggle-commit-metadata".into())
            .accessibility_label(self.t("git_commit_metadata"))
            .on_click(cx.listener(move |this, _, _, cx| {
                let sections = &mut this.git_states[project].collapsed_sections;
                if !sections.remove("git_commit_metadata") {
                    sections.insert("git_commit_metadata");
                }
                cx.notify();
            })),
        )
    }
}
