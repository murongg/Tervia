use super::super::controls::badge;
use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use tervia::git::graph::VisibleRow;

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Reference {
    Branch { name: String, remote: bool },
    Tag(String),
}
impl Reference {
    pub fn name(&self) -> &str {
        match self {
            Self::Branch { name, .. } | Self::Tag(name) => name,
        }
    }
    pub fn qualified(&self) -> String {
        match self {
            Self::Branch { name, remote } => {
                format!("refs/{}/{name}", if *remote { "remotes" } else { "heads" })
            }
            Self::Tag(name) => format!("refs/tags/{name}"),
        }
    }
    pub fn exists(&self, snapshot: &Snapshot) -> bool {
        match self {
            Self::Branch { name, remote } => snapshot
                .branches
                .iter()
                .any(|branch| branch.name == *name && branch.remote == *remote),
            Self::Tag(name) => snapshot.tags.contains(name),
        }
    }
}

#[derive(Default)]
pub(super) struct History {
    pub target: Option<Reference>,
    pub commits: Arc<Vec<tervia::git::Commit>>,
    pub graph: Arc<tervia::git::graph::Graph>,
    pub loading: bool,
    pub error: Option<tervia::git::Error>,
    generation: u64,
}
impl History {
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.loading = false;
    }
    pub fn select(&mut self, target: Option<Reference>) -> bool {
        if self.target == target {
            return false;
        }
        self.cancel();
        self.target = target;
        self.commits = Arc::default();
        self.graph = Arc::default();
        self.error = None;
        true
    }
    pub fn begin(&mut self) -> Option<u64> {
        if self.loading {
            return None;
        }
        self.generation += 1;
        self.loading = true;
        Some(self.generation)
    }
    pub fn install(&mut self, commits: Vec<tervia::git::Commit>) {
        if self.commits.len() != commits.len()
            || !self
                .commits
                .iter()
                .zip(&commits)
                .all(|(old, new)| old.hash == new.hash && old.parents == new.parents)
        {
            // Virtual rows reuse the full page's DAG so scrolling cannot reset graph lanes.
            self.graph = Arc::new(tervia::git::graph::Graph::new(&commits));
        }
        self.commits = Arc::new(commits);
    }
    pub fn finish(
        &mut self,
        generation: u64,
        result: Result<Vec<tervia::git::Commit>, tervia::git::Error>,
    ) -> bool {
        // Both successful and failed reads belong to the scope that started them.
        if self.generation != generation {
            return false;
        }
        self.loading = false;
        match result {
            Ok(commits) => {
                self.install(commits);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        true
    }
}

impl Tervia {
    pub(super) fn git_history(&self, snapshot: &Arc<Snapshot>, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let filter = ui.first.read(cx).value().to_lowercase();
        let commits = ui.history.commits.clone();
        let indices: Vec<_> = commits
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
        let visible = ui.history.graph.visible(&indices);
        let graph = ui.history.graph.clone();
        let graph_width = graph::width(&graph);
        let rows = uniform_list(
            ("git-history-rows", project),
            indices.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let row = visible[row];
                        this.git_history_row(project, row, &commits[row.index], &graph, cx)
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .scrollbar(&ui.history_scroll);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.background)
            .child(
                div()
                    .min_h(px(40.0))
                    .flex_wrap()
                    .py(px(4.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .bg(p.toolbar)
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .max_w(px(160.0))
                            .min_w_0()
                            .text_ellipsis()
                            .debug_selector(|| "git-history-scope".into())
                            .child(
                                ui.history
                                    .target
                                    .as_ref()
                                    .map(Reference::name)
                                    .unwrap_or(self.t("commit_history"))
                                    .to_owned(),
                            ),
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
                    .child(self.history_reference_actions(project, snapshot, cx))
                    .child(div().flex_1())
                    .child(
                        Input::new(&ui.first)
                            .aria_label(self.t("git_filter"))
                            .w(px(160.0)),
                    )
                    .child(
                        label_button("git-more-history", self.t("git_load_more"), p)
                            .disabled(ui.busy || ui.history.loading || ui.limit >= 2000)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.git_states[project].limit =
                                    (this.git_states[project].limit + 100).min(2000);
                                if this.git_states[project].history.target.is_some() {
                                    this.load_git_history(project, window, cx);
                                } else {
                                    this.refresh_git(project, window, cx);
                                }
                            })),
                    ),
            )
            .when(
                ui.history.loading && ui.history.commits.is_empty(),
                |panel| {
                    panel.child(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .text_color(p.secondary)
                            .child(self.t("git_running")),
                    )
                },
            )
            .when_some(ui.history.error.as_ref(), |panel, error| {
                panel.child(
                    div()
                        .px(px(12.0))
                        .py(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_color(p.secondary)
                        .child(self.t("git_history_failed"))
                        .child(
                            label_button("git-history-retry", self.t("files_retry"), p)
                                .tooltip(error.to_string())
                                .disabled(ui.history.loading)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.load_git_history(project, window, cx)
                                })),
                        ),
                )
            })
            .child(
                div()
                    .id("git-history-table")
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scrollbar()
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
                        self.appearance.theme.is_light(),
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
                                            self.appearance.theme.is_light(),
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

impl Tervia {
    pub(super) fn select_git_reference(
        &mut self,
        project: usize,
        target: Reference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_git_mode(project, Mode::History, window, cx);
        let ui = &mut self.git_states[project];
        ui.history.select(Some(target));
        ui.clear_detail(cx);
        ui.history_scroll.scroll_to_item(0, ScrollStrategy::Top);
        ui.first
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.load_git_history(project, window, cx);
        cx.notify();
    }

    pub(super) fn load_git_history(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.git_states[project];
        let Some(target) = ui.history.target.clone() else {
            return;
        };
        let Some(repository) = ui.repository.clone() else {
            return;
        };
        let Some(generation) = ui.history.begin() else {
            return;
        };
        let limit = ui.limit;
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { repository.history_for(&target.qualified(), limit) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let ui = &mut this.git_states[project];
                if !ui.history.finish(generation, result) {
                    return;
                }
                if ui.mode == Mode::History
                    && ui.selected.is_none()
                    && ui.history.error.is_none()
                    && let Some(commit) = ui.history.commits.first()
                {
                    let detail = Detail::Commit(commit.hash.clone());
                    this.git_detail(project, detail, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
