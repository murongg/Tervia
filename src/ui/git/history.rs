use super::super::controls::badge;
use super::*;
use tervia::git::graph::VisibleRow;

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
        let visible = ui.history_graph.visible(&indices);
        let graph = ui.history_graph.clone();
        let graph_width = graph::width(&graph);
        let rows = uniform_list(
            ("git-history-rows", project),
            indices.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let row = visible[row];
                        this.git_history_row(project, row, &captured.commits[row.index], &graph, cx)
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
                    .when(!filter.is_empty(), |toolbar| {
                        toolbar.child(
                            div()
                                .max_w(px(180.0))
                                .text_ellipsis()
                                .text_size(px(11.0))
                                .text_color(p.muted)
                                .child(self.t("git_graph_filtered")),
                        )
                    })
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
                    .id("git-history-table")
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scroll()
                    .child(
                        div()
                            .w_full()
                            .min_w(px(graph_width + 535.0))
                            .h_full()
                            .flex()
                            .flex_col()
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
                                    .child(
                                        div()
                                            .w(px(graph_width))
                                            .flex_shrink_0()
                                            .child(self.t("git_graph")),
                                    )
                                    .child(div().w(px(66.0)).flex_shrink_0().child("SHA"))
                                    .child(div().flex_1().child(self.t("commit_message")))
                                    .child(
                                        div()
                                            .w(px(130.0))
                                            .flex_shrink_0()
                                            .child(self.t("git_author")),
                                    )
                                    .child(
                                        div().w(px(95.0)).flex_shrink_0().child(self.t("git_date")),
                                    ),
                            )
                            .child(rows),
                    ),
            )
    }

    fn git_history_row(
        &self,
        project: usize,
        row: VisibleRow,
        commit: &tervia::git::Commit,
        graph: &tervia::git::graph::Graph,
        cx: &mut Context<Self>,
    ) -> Button {
        let ui = &self.git_states[project];
        let p = self.git_p();
        #[cfg(all(test, feature = "ui-tests"))]
        ui.rendered_rows.set(ui.rendered_rows.get() + 1);
        let hash = commit.hash.clone();
        let selected = matches!(&ui.selected, Some(Detail::Commit(selected)) if selected == &hash);
        label_button(("git-commit-row", row.index), "", p)
            .w_full()
            .h(px(32.0))
            .min_h(px(32.0))
            .p(px(0.0))
            .rounded(px(0.0))
            .bg(if selected { p.selected } else { p.background })
            .accessibility_label(commit.message.clone())
            .tooltip(format!("{}\n{}", commit.message, commit.refs))
            .child(
                div()
                    .w_full()
                    .h(px(32.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .text_size(px(12.0))
                    .text_color(p.text)
                    .child(graph::cell(
                        graph,
                        row,
                        selected,
                        self.appearance.light_theme,
                    ))
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
                    .when(!commit.refs.is_empty(), |view| {
                        view.child(
                            div()
                                .max_w(px(170.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(10.0))
                                .text_color(p.accent)
                                .children(commit.refs.split(", ").map(|reference| {
                                    badge(
                                        reference.replace("HEAD -> ", "HEAD · "),
                                        graph::color(
                                            graph.rows[row.index].color,
                                            self.appearance.light_theme,
                                        ),
                                        p,
                                    )
                                    .text_ellipsis()
                                })),
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

    pub(super) fn open_git_commit(
        &mut self,
        project: usize,
        hash: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.git_states[project];
        ui.first
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(index) = ui.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .commits
                .iter()
                .position(|commit| commit.hash == hash)
        }) {
            ui.history_scroll
                .scroll_to_item(index, ScrollStrategy::Center);
        }
        self.git_detail(project, Detail::Commit(hash), window, cx);
    }
}
