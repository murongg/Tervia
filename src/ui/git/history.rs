use super::super::controls::icon;
use super::*;
use gpui_kit::assets::IconName;

impl Tervia {
    pub(super) fn git_history(&self, snapshot: &Arc<Snapshot>, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let filter = ui.first.read(cx).value().to_lowercase();
        let indices: Vec<_> = snapshot
            .commits
            .iter()
            .enumerate()
            .filter(|(_, commit)| {
                filter.is_empty()
                    || format!(
                        "{} {} {} {}",
                        commit.hash, commit.message, commit.author, commit.refs
                    )
                    .to_lowercase()
                    .contains(&filter)
            })
            .map(|(index, _)| index)
            .collect();
        let captured = snapshot.clone();
        let rows = uniform_list(
            ("git-history-rows", project),
            indices.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let index = indices[row];
                        this.git_history_row(project, index, &captured.commits[index], cx)
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .track_scroll(&ui.history_scroll);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.background)
            .child(
                div()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .bg(p.toolbar)
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.t("commit_history")),
                    )
                    .child(div().flex_1())
                    .child(
                        Input::new(&ui.first)
                            .aria_label(self.t("git_filter"))
                            .w(px(240.0)),
                    )
                    .child(
                        label_button("git-more-history", self.t("git_load_more"), p)
                            .disabled(ui.busy || ui.limit >= 2000)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.git_states[project].limit =
                                    (this.git_states[project].limit + 100).min(2000);
                                this.refresh_git(project, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .h(px(26.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(11.0))
                    .text_color(p.muted)
                    .bg(p.toolbar)
                    .child(div().w(px(89.0)).child("SHA"))
                    .child(div().flex_1().child(self.t("commit_message")))
                    .child(div().w(px(130.0)).child(self.t("git_author")))
                    .child(div().w(px(95.0)).child(self.t("git_date"))),
            )
            .child(rows)
    }

    fn git_history_row(
        &self,
        project: usize,
        index: usize,
        commit: &tervia::git::Commit,
        cx: &mut Context<Self>,
    ) -> Button {
        let ui = &self.git_states[project];
        let p = self.git_p();
        #[cfg(all(test, feature = "ui-tests"))]
        ui.rendered_rows.set(ui.rendered_rows.get() + 1);
        let hash = commit.hash.clone();
        let selected = matches!(&ui.selected, Some(Detail::Commit(selected)) if selected == &hash);
        label_button(("git-commit-row", index), "", p)
            .w_full()
            .h(px(32.0))
            .min_h(px(32.0))
            .rounded(px(0.0))
            .bg(if selected { p.selected } else { p.background })
            .accessibility_label(commit.message.clone())
            .tooltip(format!("{}\n{}", commit.message, commit.refs))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .text_size(px(12.0))
                    .text_color(p.text)
                    .child(icon(IconName::GitCommitHorizontal, p.accent))
                    .child(
                        div()
                            .w(px(66.0))
                            .flex_shrink_0()
                            .font_family(self.appearance.font_family.clone())
                            .text_size(px(11.0))
                            .text_color(p.muted)
                            .child(hash[..7.min(hash.len())].to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(commit.message.clone()),
                    )
                    .when(!commit.refs.is_empty(), |row| {
                        row.child(
                            div()
                                .max_w(px(170.0))
                                .text_ellipsis()
                                .text_size(px(10.0))
                                .text_color(p.accent)
                                .child(commit.refs.clone()),
                        )
                    })
                    .child(
                        div()
                            .w(px(130.0))
                            .flex_shrink_0()
                            .text_ellipsis()
                            .text_color(p.secondary)
                            .child(commit.author.clone()),
                    )
                    .child(
                        div()
                            .w(px(95.0))
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .text_color(p.muted)
                            .child(commit.when.chars().take(10).collect::<String>()),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.git_detail(project, Detail::Commit(hash.clone()), window, cx)
            }))
    }
}
