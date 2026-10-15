use crate::ui::scroll::ListScrollbars as _;
mod jobs;

use super::super::controls::{icon, icon_button};
use super::*;
use gpui_kit::assets::IconName;
use tervia::git::{Worktree, WorktreeCheckout, WorktreeOperation};

pub(super) fn prune_description(
    root: &std::path::Path,
    targets: &[Worktree],
    hint: &str,
) -> String {
    let paths = targets
        .iter()
        .map(|tree| format!("- {:?}", tree.path))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n\n{}\n\n{}", root.display(), paths, hint)
}

#[derive(Default)]
pub(super) struct Navigation(usize);
impl Navigation {
    pub fn begin(&mut self) -> usize {
        self.0 += 1;
        self.0
    }
    pub fn current(&self, token: usize) -> bool {
        self.0 == token
    }
    pub fn invalidate(&mut self) {
        self.0 += 1;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Checkout {
    New,
    Existing,
    Detached,
}

pub(super) struct WorktreesUi {
    checkout: Checkout,
    path: Entity<InputState>,
    branch: Entity<InputState>,
    revision: Entity<InputState>,
    reason: Entity<InputState>,
    selected: Option<PathBuf>,
    scroll: UniformListScrollHandle,
    pub(super) navigation: Navigation,
    pub(super) removing: Option<PathBuf>,
}

impl WorktreesUi {
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            checkout: Checkout::New,
            path: cx.new(|cx| InputState::new(window, cx)),
            branch: cx.new(|cx| InputState::new(window, cx)),
            revision: cx.new(|cx| InputState::new(window, cx).default_value("HEAD")),
            reason: cx.new(|cx| InputState::new(window, cx)),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            navigation: Navigation::default(),
            removing: None,
        }
    }
    pub fn subscribe(&self, subscriptions: &mut Vec<Subscription>, cx: &mut Context<Tervia>) {
        for input in [&self.path, &self.branch, &self.revision, &self.reason] {
            subscriptions.push(cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()));
        }
    }
}

impl GitUi {
    pub(in crate::ui) fn cancel_worktree_navigation(&mut self) {
        self.worktrees.navigation.invalidate();
    }
}

impl Tervia {
    pub(super) fn git_worktrees(&self, snapshot: &Arc<Snapshot>, cx: &mut Context<Self>) -> Div {
        let owner = self.state.active_project;
        let ui = &self.git_states[owner].worktrees;
        let p = self.git_p();
        let busy = self.worktree_busy(owner);
        let branch = ui.branch.read(cx).value();
        let revision = ui.revision.read(cx).value();
        let path = ui.path.read(cx).value();
        let selected = snapshot
            .worktrees
            .iter()
            .find(|tree| ui.selected.as_ref() == Some(&tree.path))
            .or_else(|| {
                snapshot
                    .worktrees
                    .iter()
                    .find(|tree| tree.path == self.state.projects[owner].root)
            });
        let targets: Vec<_> = snapshot
            .worktrees
            .iter()
            .filter(|tree| tree.prunable.is_some() && tree.locked.is_none() && !tree.main)
            .cloned()
            .collect();
        let mut modes = div().flex().gap(px(4.0));
        for (index, mode, key) in [
            (0usize, Checkout::New, "worktree_new_branch"),
            (1, Checkout::Existing, "worktree_existing_branch"),
            (2, Checkout::Detached, "worktree_detached"),
        ] {
            modes = modes.child(
                label_button(("worktree-checkout", index), self.t(key), p)
                    .bg(if ui.checkout == mode {
                        p.selected
                    } else {
                        p.background
                    })
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.git_states[owner].worktrees.checkout = mode;
                        cx.notify();
                    })),
            );
        }
        let mut form = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .p(px(12.0))
            .border_b_1()
            .border_color(p.border)
            .child(modes)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(82.0))
                            .text_color(p.secondary)
                            .child(self.t("worktree_path")),
                    )
                    .child(
                        Input::new(&ui.path)
                            .aria_label(self.t("worktree_path"))
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        icon_button(
                            "worktree-choose-path",
                            IconName::FolderOpen,
                            self.t("worktree_choose_path"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("worktree_choose_path"))
                        .disabled(busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.choose_worktree_path(owner, window, cx),
                        )),
                    ),
            );
        if ui.checkout != Checkout::Detached {
            form = form.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(82.0))
                            .text_color(p.secondary)
                            .child(self.t("worktree_branch")),
                    )
                    .child(
                        Input::new(&ui.branch)
                            .aria_label(self.t("worktree_branch"))
                            .flex_1()
                            .min_w_0(),
                    ),
            );
        }
        if ui.checkout != Checkout::Existing {
            form = form.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(82.0))
                            .text_color(p.secondary)
                            .child(self.t("worktree_start")),
                    )
                    .child(
                        Input::new(&ui.revision)
                            .aria_label(self.t("worktree_start"))
                            .flex_1()
                            .min_w_0(),
                    ),
            );
        }
        form = form.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.0))
                        .text_color(p.muted)
                        .child(self.t(if snapshot.status.unborn {
                            "worktree_commit_first"
                        } else {
                            "worktree_create_hint"
                        })),
                )
                .child(
                    label_button("worktree-create", self.t("worktree_create"), p)
                        .icon(icon(IconName::Plus, p.accent))
                        .disabled(
                            busy || snapshot.status.unborn
                                || path.is_empty()
                                || (ui.checkout != Checkout::Detached && branch.is_empty())
                                || (ui.checkout != Checkout::Existing && revision.is_empty()),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.create_worktree(owner, window, cx)
                        })),
                ),
        );
        let mut toolbar = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(7.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .flex_1()
                    .text_color(p.secondary)
                    .child(self.t("worktree_directories")),
            )
            .child(
                label_button("worktree-prune", self.t("worktree_prune"), p)
                    .disabled(busy || targets.is_empty())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.request_worktree_operation(
                            owner,
                            WorktreeOperation::Prune(targets.clone()),
                            window,
                            cx,
                        )
                    })),
            );
        if let Some(tree) = selected {
            let tree = tree.clone();
            let current = self.state.projects[owner].root == tree.path;
            toolbar = toolbar.child(
                label_button("worktree-open", self.t("worktree_open"), p)
                    .disabled(current || tree.bare || tree.missing || tree.prunable.is_some())
                    .on_click(cx.listener({
                        let tree = tree.clone();
                        move |this, _, window, cx| {
                            this.open_worktree(owner, tree.clone(), window, cx)
                        }
                    })),
            );
            if !tree.main {
                toolbar = toolbar
                    .child(
                        label_button(
                            "worktree-lock",
                            self.t(if tree.locked.is_some() {
                                "worktree_unlock"
                            } else {
                                "worktree_lock"
                            }),
                            p,
                        )
                        .disabled(busy)
                        .on_click(cx.listener({
                            let tree = tree.clone();
                            move |this, _, window, cx| {
                                let operation = if tree.locked.is_some() {
                                    WorktreeOperation::Unlock(tree.clone())
                                } else {
                                    WorktreeOperation::Lock {
                                        tree: tree.clone(),
                                        reason: this.git_states[owner]
                                            .worktrees
                                            .reason
                                            .read(cx)
                                            .value()
                                            .to_string(),
                                    }
                                };
                                this.request_worktree_operation(owner, operation, window, cx);
                            }
                        })),
                    )
                    .child(
                        label_button("worktree-remove", self.t("worktree_remove"), p)
                            .text_color(p.red)
                            .disabled(
                                busy || current
                                    || tree.locked.is_some()
                                    || tree.missing
                                    || tree.prunable.is_some(),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_worktree_operation(
                                    owner,
                                    WorktreeOperation::Remove(tree.clone()),
                                    window,
                                    cx,
                                )
                            })),
                    );
            }
        }
        let trees = snapshot.worktrees.clone();
        let lock_reason = selected
            .and_then(|tree| tree.locked.as_deref())
            .filter(|reason| !reason.is_empty());
        let reason_field = if let Some(reason) = lock_reason {
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_color(p.secondary)
                .child(reason.to_owned())
                .into_any_element()
        } else {
            Input::new(&ui.reason)
                .aria_label(self.t("worktree_lock_reason"))
                .flex_1()
                .min_w_0()
                .into_any_element()
        };
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(12.0))
            .child(self.git_panel_title("git_worktrees"))
            .child(form)
            .child(toolbar)
            .child(
                uniform_list(
                    ("worktree-list", owner),
                    trees.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.worktree_row(owner, index, &trees[index], cx))
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .scrollbar(&ui.scroll),
            )
            .when(selected.is_some_and(|tree| !tree.main), |panel| {
                panel.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .p(px(12.0))
                        .border_t_1()
                        .border_color(p.border)
                        .child(
                            div()
                                .text_color(p.secondary)
                                .child(self.t("worktree_lock_reason")),
                        )
                        .child(reason_field),
                )
            })
    }

    fn worktree_row(
        &self,
        owner: usize,
        index: usize,
        tree: &Worktree,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = &self.git_states[owner].worktrees;
        let p = self.git_p();
        let selected = ui
            .selected
            .as_ref()
            .map(|path| path == &tree.path)
            .unwrap_or(tree.path == self.state.projects[owner].root);
        let path = tree.path.clone();
        let caption = self.worktree_caption(tree);
        label_button(("worktree-row", index), "", p)
            .h(px(52.0))
            .w_full()
            .rounded(px(0.0))
            .bg(if selected { p.selected } else { p.background })
            .tooltip(format!(
                "{}\n{}",
                tree.path.display(),
                tree.locked
                    .as_deref()
                    .or(tree.prunable.as_deref())
                    .unwrap_or_default()
            ))
            .accessibility_label(format!("{} · {}", caption, tree.path.display()))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .px(px(4.0))
                    .child(icon(
                        IconName::FolderGit2,
                        if selected { p.accent } else { p.secondary },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().text_ellipsis().text_color(p.text).child(caption))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(p.muted)
                                    .text_ellipsis()
                                    .child(tree.path.to_string_lossy().into_owned()),
                            ),
                    )
                    .when(tree.locked.is_some(), |row| {
                        row.child(icon(IconName::Lock, p.muted))
                    })
                    .when(tree.missing || tree.prunable.is_some(), |row| {
                        row.child(icon(IconName::TriangleAlert, p.red))
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.git_states[owner].worktrees.navigation.invalidate();
                this.git_states[owner].worktrees.selected = Some(path.clone());
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn worktree_caption(&self, tree: &Worktree) -> String {
        let name = tree.branch.clone().unwrap_or_else(|| {
            format!(
                "{} · {}",
                tree.path.file_name().unwrap_or_default().to_string_lossy(),
                self.t(if tree.bare {
                    "worktree_bare"
                } else {
                    "worktree_detached"
                })
            )
        });
        let state = if tree.main {
            self.t("worktree_main")
        } else if tree.missing || tree.prunable.is_some() {
            self.t("worktree_missing")
        } else {
            ""
        };
        if state.is_empty() {
            name
        } else {
            format!("{name} · {state}")
        }
    }
}
