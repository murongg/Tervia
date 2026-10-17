use super::super::controls::{icon, icon_button};
use super::history::Reference;
use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::ContextMenuExt as _;

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

pub(super) const SECTIONS: [&str; 4] = [
    "git_worktrees",
    "git_local_branches",
    "git_remote_branches",
    "git_tags",
];

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
        let navigation = [
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
        ];
        let mut panels = Vec::new();
        for (index, key) in SECTIONS.into_iter().enumerate() {
            let mut rows: Vec<_> = match key {
                "git_worktrees" => (0..snapshot.worktrees.len())
                    .map(SidebarRow::Worktree)
                    .collect(),
                "git_tags" => (0..snapshot.tags.len()).map(SidebarRow::Tag).collect(),
                _ => snapshot
                    .branches
                    .iter()
                    .enumerate()
                    .filter(|(_, branch)| branch.remote == (key == "git_remote_branches"))
                    .map(|(index, _)| SidebarRow::Branch(index))
                    .collect(),
            };
            let count = rows.len();
            if key == "git_remote_branches" && count == 0 {
                continue;
            }
            if key == "git_tags" && count == 0 {
                rows.push(SidebarRow::Hint("git_no_tags"));
            }
            let folded = ui.collapsed_sections.contains(key);
            let body = if folded {
                None
            } else if rows.is_empty() {
                Some(div().flex_1().min_h_0())
            } else {
                let snapshot = snapshot.clone();
                Some(
                    uniform_list(
                        format!("git-sidebar-rows-{project}-{index}"),
                        rows.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| {
                                    this.git_sidebar_row(project, &snapshot, &rows[index], cx)
                                })
                                .collect()
                        }),
                    )
                    .flex_1()
                    .min_h_0()
                    .scrollbar(&ui.sidebar_scroll[index]),
                )
            };
            panels.push((
                index,
                folded,
                sections::panel(
                    self.git_sidebar_row(project, snapshot, &SidebarRow::Section(key, count), cx),
                    body,
                ),
            ));
        }
        div()
            .debug_selector(|| "git-sidebar-pane".into())
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .child(
                div().flex_shrink_0().flex().flex_col().children(
                    navigation
                        .iter()
                        .map(|row| self.git_sidebar_row(project, snapshot, row, cx)),
                ),
            )
            .child(sections::stack(
                format!("git-sidebar-sections-{project}"),
                &ui.sidebar_layout,
                32.0,
                sections::HeaderBorder::Top,
                panels,
            ))
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
                let active =
                    ui.mode == mode && (mode != Mode::History || ui.history.target.is_none());
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
                    .debug_selector(move || format!("git-sidebar-section-{key}"))
                    .w_full()
                    .h(px(32.0))
                    .flex_shrink_0()
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
                    .when(key == "git_local_branches" || key == "git_tags", |row| {
                        row.child(self.reference_form_trigger(project, key == "git_tags", cx))
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
                let reference = Reference::Branch {
                    name: name.clone(),
                    remote: branch.remote,
                };
                let selected =
                    ui.mode == Mode::History && ui.history.target.as_ref() == Some(&reference);
                let menu_reference = reference.clone();
                let owner = cx.weak_entity();
                label_button(("git-sidebar-branch", *index), "", p)
                    .debug_selector({
                        let index = *index;
                        move || format!("git-sidebar-branch-{index}")
                    })
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
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.click_count() >= 2 {
                            this.switch_branch(project, reference.clone(), window, cx);
                        } else {
                            this.select_git_reference(project, reference.clone(), window, cx);
                        }
                    }))
                    .context_menu(move |menu, _, cx| {
                        if let Some(view) = owner.upgrade() {
                            view.read(cx).reference_menu(
                                project,
                                &menu_reference,
                                menu,
                                owner.clone(),
                            )
                        } else {
                            menu
                        }
                    })
                    .into_any_element()
            }
            SidebarRow::Worktree(index) => {
                let tree = snapshot.worktrees[*index].clone();
                let current = tree.path == self.state.projects[project].root;
                let caption = self.worktree_caption(&tree);
                let name = self.worktree_name(&tree);
                let menu_tree = tree.clone();
                let owner = cx.weak_entity();
                let index = *index;
                label_button(("git-sidebar-worktree", index), "", p)
                    .debug_selector(move || format!("git-sidebar-worktree-{index}"))
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
                            .child(
                                icon(
                                    IconName::FolderGit2,
                                    if current { p.accent } else { p.muted },
                                )
                                .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .debug_selector(move || format!("git-worktree-name-{index}"))
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(name),
                            )
                            .when(tree.main, |row| {
                                // Keep directory identity outside the branch's ellipsis, even in a narrow sidebar.
                                row.child(
                                    super::super::controls::badge(
                                        self.t("worktree_main_badge"),
                                        p.secondary,
                                        p,
                                    )
                                    .debug_selector(move || format!("git-worktree-main-{index}"))
                                    .flex_shrink_0()
                                    .whitespace_nowrap()
                                    .px(px(4.0)),
                                )
                            })
                            .when(tree.locked.is_some(), |row| {
                                row.child(icon(IconName::Lock, p.muted))
                            })
                            .when(current, |row| {
                                row.child(
                                    div()
                                        .debug_selector(move || {
                                            format!("git-worktree-current-{index}")
                                        })
                                        .flex_shrink_0()
                                        .child(icon(IconName::Check, p.accent)),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if current {
                            this.select_git_mode(project, Mode::Worktrees, window, cx);
                        } else {
                            this.open_worktree(project, tree.clone(), window, cx);
                        }
                    }))
                    .context_menu(move |menu, _, cx| {
                        let Some(view) = owner.upgrade() else {
                            return menu;
                        };
                        view.read(cx)
                            .worktree_menu_items(project, &menu_tree, owner.clone())
                            .into_iter()
                            .fold(menu, |menu, item| menu.item(item))
                    })
                    .into_any_element()
            }
            SidebarRow::Tag(index) => {
                let tag = snapshot.tags[*index].clone();
                let reference = Reference::Tag(tag.clone());
                let menu_reference = reference.clone();
                let owner = cx.weak_entity();
                label_button(("git-sidebar-tag", *index), "", p)
                    .h(px(32.0))
                    .w_full()
                    .rounded(px(0.0))
                    .bg(
                        if ui.mode == Mode::History
                            && ui.history.target.as_ref() == Some(&reference)
                        {
                            p.selected
                        } else {
                            p.sidebar
                        },
                    )
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
                        this.select_git_reference(project, reference.clone(), window, cx)
                    }))
                    .context_menu(move |menu, _, cx| {
                        if let Some(view) = owner.upgrade() {
                            view.read(cx).reference_menu(
                                project,
                                &menu_reference,
                                menu,
                                owner.clone(),
                            )
                        } else {
                            menu
                        }
                    })
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
