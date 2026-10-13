use super::super::controls::icon;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;

enum BranchRow {
    Section(&'static str),
    Branch(usize),
    Tag(usize),
}
impl Tervia {
    pub(super) fn git_branches(
        &self,
        snapshot: &Arc<Snapshot>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let name = ui.first.read(cx).value().to_string();
        let new = ui.second.read(cx).value().to_string();
        let selected = navigation::branch_target(ui.branch.as_deref(), snapshot);
        let mut panel = div()
            .id("real-git-branches")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.background)
            .child(self.git_panel_title("git_branches"))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        Input::new(&ui.first)
                            .aria_label(self.t("git_name"))
                            .w(px(220.0)),
                    )
                    .child(
                        self.git_action(
                            "git-create-branch",
                            "git_create_branch",
                            Operation::CreateBranch(name.clone()),
                            cx,
                        )
                        .icon(icon(IconName::Plus, p.secondary))
                        .disabled(ui.busy || self.git_mutating || name.is_empty()),
                    )
                    .child(
                        self.git_action(
                            "git-create-tag",
                            "git_create_tag",
                            Operation::CreateTag(name),
                            cx,
                        )
                        .icon(icon(IconName::Tag, p.secondary))
                        .disabled(
                            ui.busy || self.git_mutating || ui.first.read(cx).value().is_empty(),
                        ),
                    ),
            );
        if let Some(branch) = selected {
            let mut actions = div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .flex_wrap()
                .gap(px(5.0))
                .px(px(12.0))
                .py(px(10.0))
                .bg(p.toolbar)
                .child(icon(IconName::GitBranch, p.accent))
                .child(
                    div()
                        .mr(px(8.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(branch.name.clone()),
                );
            if !branch.current {
                actions = actions
                    .child(self.git_action(
                        "git-selected-switch",
                        if branch.remote {
                            "git_track"
                        } else {
                            "git_switch"
                        },
                        if branch.remote {
                            Operation::Track(branch.name.clone())
                        } else {
                            Operation::Switch(branch.name.clone())
                        },
                        cx,
                    ))
                    .child(self.git_action(
                        "git-selected-merge",
                        "git_merge",
                        Operation::Merge(branch.name.clone()),
                        cx,
                    ))
                    .child(self.git_action(
                        "git-selected-rebase",
                        "git_rebase",
                        Operation::Rebase(branch.name.clone()),
                        cx,
                    ));
                if !branch.remote {
                    actions = actions.child(
                        self.git_action(
                            "git-selected-delete",
                            "git_delete",
                            Operation::DeleteBranch(branch.name.clone()),
                            cx,
                        )
                        .text_color(p.red),
                    );
                }
            }
            panel = panel.child(actions);
            if !branch.remote {
                panel = panel.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .pb(px(12.0))
                        .bg(p.toolbar)
                        .child(
                            Input::new(&ui.second)
                                .aria_label(self.t("git_new_name"))
                                .w(px(220.0)),
                        )
                        .child(
                            self.git_action(
                                "git-rename-branch",
                                "git_rename",
                                Operation::RenameBranch {
                                    old: branch.name.clone(),
                                    new,
                                },
                                cx,
                            )
                            .disabled(
                                ui.busy
                                    || self.git_mutating
                                    || ui.second.read(cx).value().is_empty(),
                            ),
                        ),
                );
            }
        }
        let mut rows = Vec::new();
        for (key, remote) in [("git_local_branches", false), ("git_remote_branches", true)] {
            rows.push(BranchRow::Section(key));
            if !ui.collapsed_sections.contains(key) {
                rows.extend(
                    snapshot
                        .branches
                        .iter()
                        .enumerate()
                        .filter(|(_, branch)| branch.remote == remote)
                        .map(|(index, _)| BranchRow::Branch(index)),
                );
            }
        }
        rows.push(BranchRow::Section("git_tags"));
        if !ui.collapsed_sections.contains("git_tags") {
            rows.extend((0..snapshot.tags.len()).map(BranchRow::Tag));
        }
        let snapshot = snapshot.clone();
        panel.child(
            uniform_list(
                ("git-branch-rows", project),
                rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| this.git_branch_row(project, &snapshot, &rows[index], cx))
                        .collect()
                }),
            )
            .flex_1()
            .min_h_0()
            .track_scroll(&ui.branches_scroll),
        )
    }

    fn git_branch_row(
        &self,
        project: usize,
        snapshot: &Snapshot,
        row: &BranchRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = &self.git_states[project];
        let p = self.git_p();
        match row {
            BranchRow::Section(key) => {
                let key = *key;
                let folded = ui.collapsed_sections.contains(key);
                div()
                    .id(format!("git-branch-section-{key}"))
                    .h(px(34.0))
                    .w_full()
                    .bg(p.toolbar)
                    .hover(|style| style.bg(p.raised))
                    .child(
                        label_button(key, "", p)
                            .text()
                            .h_full()
                            .w_full()
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
                                    .gap(px(8.0))
                                    .text_size(px(11.0))
                                    .child(icon(
                                        if folded {
                                            IconName::ChevronRight
                                        } else {
                                            IconName::ChevronDown
                                        },
                                        p.muted,
                                    ))
                                    .child(self.t(key)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let folded = &mut this.git_states[project].collapsed_sections;
                                if !folded.remove(key) {
                                    folded.insert(key);
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            }
            BranchRow::Branch(index) => {
                #[cfg(all(test, feature = "ui-tests"))]
                ui.rendered_rows.set(ui.rendered_rows.get() + 1);
                let branch = &snapshot.branches[*index];
                let name = branch.name.clone();
                let active = navigation::branch_target(ui.branch.as_deref(), snapshot)
                    .is_some_and(|selected| selected.name == name);
                label_button(("git-branch-row", *index), "", p)
                    .w_full()
                    .h(px(34.0))
                    .rounded(px(0.0))
                    .bg(if active { p.selected } else { p.background })
                    .accessibility_label(name.clone())
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .child(icon(
                                IconName::GitBranch,
                                if branch.current { p.accent } else { p.muted },
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_color(p.text)
                                    .child(name.clone()),
                            )
                            .when(branch.current, |row| {
                                row.child(
                                    div().text_size(px(10.0)).text_color(p.accent).child("HEAD"),
                                )
                            })
                            .child(
                                div()
                                    .w(px(240.0))
                                    .text_ellipsis()
                                    .text_color(p.muted)
                                    .child(branch.upstream.clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.git_states[project].branch = Some(name.clone());
                        cx.notify();
                    }))
                    .into_any_element()
            }
            BranchRow::Tag(index) => {
                let tag = &snapshot.tags[*index];
                div()
                    .w_full()
                    .h(px(34.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .child(icon(IconName::Tag, p.muted))
                    .child(div().flex_1().text_size(px(12.0)).child(tag.clone()))
                    .child(
                        self.git_action_for(
                            project,
                            ("git-delete-tag", *index),
                            "git_delete",
                            Operation::DeleteTag(tag.clone()),
                            cx,
                        )
                        .text_color(p.muted),
                    )
                    .into_any_element()
            }
        }
    }
}
