use super::super::controls::{icon, icon_button};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;

#[derive(Clone)]
enum SidebarRow {
    Navigation(usize, Mode, &'static str, IconName, usize),
    Section(&'static str, usize),
    Worktree(usize),
    Branch(usize),
    Tag(usize),
    Hint(&'static str),
}

impl Tervia {
    pub(super) fn git_sidebar(&self, snapshot: &Arc<Snapshot>, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let mut rows = Vec::new();
        rows.extend([
            SidebarRow::Navigation(
                0,
                Mode::Changes,
                "git_working_copy",
                IconName::FileDiff,
                snapshot.status.files.len(),
            ),
            SidebarRow::Navigation(
                1,
                Mode::History,
                "commit_history",
                IconName::GitCommitHorizontal,
                0,
            ),
            SidebarRow::Navigation(
                2,
                Mode::Stash,
                "git_stash",
                IconName::Archive,
                snapshot.stashes.len(),
            ),
            SidebarRow::Navigation(
                3,
                Mode::Remotes,
                "git_remotes",
                IconName::Globe,
                snapshot.remotes.len(),
            ),
        ]);
        rows.push(SidebarRow::Section(
            "git_worktrees",
            snapshot.worktrees.len(),
        ));
        if !ui.collapsed_sections.contains("git_worktrees") {
            rows.extend((0..snapshot.worktrees.len()).map(SidebarRow::Worktree));
        }
        for (section, remote) in [("git_local_branches", false), ("git_remote_branches", true)] {
            let count = snapshot
                .branches
                .iter()
                .filter(|branch| branch.remote == remote)
                .count();
            if remote && count == 0 {
                continue;
            }
            rows.push(SidebarRow::Section(section, count));
            if !ui.collapsed_sections.contains(section) {
                rows.extend(
                    snapshot
                        .branches
                        .iter()
                        .enumerate()
                        .filter(|(_, branch)| branch.remote == remote)
                        .map(|(index, _)| SidebarRow::Branch(index)),
                );
            }
        }
        rows.push(SidebarRow::Section("git_tags", snapshot.tags.len()));
        if !ui.collapsed_sections.contains("git_tags") {
            rows.extend((0..snapshot.tags.len()).map(SidebarRow::Tag));
            if snapshot.tags.is_empty() {
                rows.push(SidebarRow::Hint("git_no_tags"));
            }
        }
        let snapshot = snapshot.clone();
        div()
            .w(px(174.0))
            .flex_shrink_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .child(
                uniform_list(
                    ("git-sidebar-rows", project),
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.git_sidebar_row(project, &snapshot, &rows[index], cx))
                            .collect()
                    }),
                )
                .size_full()
                .track_scroll(&ui.sidebar_scroll),
            )
    }

    fn git_sidebar_row(
        &self,
        project: usize,
        snapshot: &Snapshot,
        row: &SidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = &self.git_states[project];
        let p = self.git_p();
        match row {
            SidebarRow::Navigation(index, mode, key, glyph, count) => {
                let mode = *mode;
                let active = ui.mode == mode;
                label_button(("git-mode", *index), "", p)
                    .h(px(32.0))
                    .w_full()
                    .rounded(px(0.0))
                    .bg(if active { p.selected } else { p.sidebar })
                    .accessibility_label(self.t(key))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(4.0))
                            .child(icon(*glyph, if active { p.accent } else { p.muted }))
                            .child(div().flex_1().child(self.t(key)))
                            .when(*count > 0, |row| {
                                row.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(p.muted)
                                        .child(count.to_string()),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_git_mode(project, mode, window, cx)
                    }))
                    .into_any_element()
            }
            SidebarRow::Section(key, count) => {
                let key = *key;
                let folded = ui.collapsed_sections.contains(key);
                div()
                    .id(format!("git-sidebar-section-{key}"))
                    .w_full()
                    .h(px(32.0))
                    .hover(|style| style.bg(p.raised))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        label_button(key, "", p)
                            .text()
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .rounded(px(0.0))
                            .accessibility_label(format!(
                                "{} {}",
                                self.t(if folded { "git_expand" } else { "git_collapse" }),
                                self.t(key)
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .pr(px(8.0))
                                    .text_size(px(11.0))
                                    .text_color(p.muted)
                                    .child(icon(
                                        if folded {
                                            IconName::ChevronRight
                                        } else {
                                            IconName::ChevronDown
                                        },
                                        p.muted,
                                    ))
                                    .child(
                                        div().flex_1().min_w_0().text_ellipsis().child(self.t(key)),
                                    )
                                    .child(count.to_string()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let folded = &mut this.git_states[project].collapsed_sections;
                                if !folded.remove(key) {
                                    folded.insert(key);
                                }
                                cx.notify();
                            })),
                    )
                    .when(key == "git_local_branches", |row| {
                        row.child(
                            icon_button(
                                "git-branch-manage",
                                IconName::Plus,
                                self.t("git_branches"),
                                p,
                                cx,
                            )
                            .accessibility_label(self.t("git_branches"))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.select_git_mode(project, Mode::Branches, window, cx)
                                },
                            )),
                        )
                    })
                    .when(key == "git_worktrees", |row| {
                        row.child(
                            icon_button(
                                "git-worktree-manage",
                                IconName::Plus,
                                self.t("worktree_manage"),
                                p,
                                cx,
                            )
                            .accessibility_label(self.t("worktree_manage"))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.select_git_mode(project, Mode::Worktrees, window, cx)
                                },
                            )),
                        )
                    })
                    .into_any_element()
            }
            SidebarRow::Branch(index) => {
                let branch = &snapshot.branches[*index];
                let name = branch.name.clone();
                let selected = ui.mode == Mode::Branches
                    && navigation::branch_target(ui.branch.as_deref(), snapshot)
                        .is_some_and(|target| target.name == name);
                label_button(("git-sidebar-branch", *index), "", p)
                    .h(px(32.0))
                    .w_full()
                    .rounded(px(0.0))
                    .bg(if selected { p.selected } else { p.sidebar })
                    .accessibility_label(name.clone())
                    .tooltip(name.clone())
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(4.0))
                            .min_w_0()
                            .child(icon(
                                IconName::GitBranch,
                                if branch.current { p.accent } else { p.muted },
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_color(if branch.current { p.text } else { p.secondary })
                                    .child(name.clone()),
                            )
                            .when(branch.current, |row| {
                                row.child(icon(IconName::Check, p.accent))
                            }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_git_mode(project, Mode::Branches, window, cx);
                        this.git_states[project].branch = Some(name.clone());
                        cx.notify();
                    }))
                    .into_any_element()
            }
            SidebarRow::Worktree(index) => {
                let tree = snapshot.worktrees[*index].clone();
                let current = tree.path == self.state.projects[project].root;
                let caption = self.worktree_caption(&tree);
                label_button(("git-sidebar-worktree", *index), "", p)
                    .h(px(32.0))
                    .w_full()
                    .rounded(px(0.0))
                    .bg(if current { p.selected } else { p.sidebar })
                    .accessibility_label(format!("{} · {}", self.t("worktree_open"), caption))
                    .tooltip(format!("{}\n{}", caption, tree.path.display()))
                    .disabled(tree.bare || tree.missing || tree.prunable.is_some())
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .px(px(4.0))
                            .child(icon(
                                IconName::FolderGit2,
                                if current { p.accent } else { p.muted },
                            ))
                            .child(div().flex_1().min_w_0().text_ellipsis().child(caption))
                            .when(tree.locked.is_some(), |row| {
                                row.child(icon(IconName::Lock, p.muted))
                            })
                            .when(current, |row| row.child(icon(IconName::Check, p.accent))),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if current {
                            this.select_git_mode(project, Mode::Worktrees, window, cx);
                        } else {
                            this.open_worktree(project, tree.clone(), window, cx);
                        }
                    }))
                    .into_any_element()
            }
            SidebarRow::Tag(index) => {
                let tag = &snapshot.tags[*index];
                label_button(("git-sidebar-tag", *index), "", p)
                    .h(px(32.0))
                    .w_full()
                    .rounded(px(0.0))
                    .accessibility_label(tag.clone())
                    .tooltip(tag.clone())
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(4.0))
                            .child(icon(IconName::Tag, p.muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_size(px(12.0))
                                    .child(tag.clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_git_mode(project, Mode::Branches, window, cx)
                    }))
                    .into_any_element()
            }
            SidebarRow::Hint(key) => div()
                .h(px(32.0))
                .px(px(14.0))
                .flex()
                .items_center()
                .text_size(px(11.0))
                .text_color(p.muted)
                .child(self.t(key))
                .into_any_element(),
        }
    }
}
