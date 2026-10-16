use super::super::controls::icon;
use super::history::Reference;
use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::popover::{Popover, PopoverState};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum SwitchAnchor {
    Navigation,
    Toolbar,
}

pub(super) struct Switcher {
    pub input: Entity<InputState>,
    pub open: Option<SwitchAnchor>,
    // Keep the ref identity across background refreshes and recent-branch reordering.
    pub selected: Option<Reference>,
    pub pending: Option<Reference>,
    pub recent: Vec<Reference>,
    scroll: UniformListScrollHandle,
}
impl Switcher {
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            input: cx.new(|cx| InputState::new(window, cx)),
            open: None,
            selected: None,
            pending: None,
            recent: Vec::new(),
            scroll: UniformListScrollHandle::new(),
        }
    }
    pub fn observe(&mut self, snapshot: &Snapshot) {
        self.recent.retain(|reference| reference.exists(snapshot));
        if let Some(branch) = snapshot.branches.iter().find(|branch| branch.current) {
            let reference = branch_reference(branch);
            self.recent.retain(|recent| *recent != reference);
            self.recent.insert(0, reference);
            self.recent.truncate(8);
        }
    }
}
fn branch_reference(branch: &tervia::git::Branch) -> Reference {
    Reference::Branch {
        name: branch.name.clone(),
        remote: branch.remote,
    }
}

impl Tervia {
    pub(super) fn branch_picker_rows(&self, project: usize, cx: &App) -> Vec<tervia::git::Branch> {
        let ui = &self.git_states[project];
        let query = ui.switcher.input.read(cx).value().to_lowercase();
        let mut rows: Vec<_> = ui
            .snapshot
            .as_ref()
            .into_iter()
            .flat_map(|snapshot| snapshot.branches.iter())
            .filter(|branch| {
                query
                    .split_whitespace()
                    .all(|word| branch.name.to_lowercase().contains(word))
            })
            .cloned()
            .collect();
        rows.sort_by_key(|branch| {
            (
                !branch.current,
                ui.switcher
                    .recent
                    .iter()
                    .position(|recent| *recent == branch_reference(branch))
                    .unwrap_or(usize::MAX),
                branch.remote,
                branch.name.clone(),
            )
        });
        rows
    }

    pub(super) fn reset_branch_picker_selection(&mut self, project: usize, cx: &mut Context<Self>) {
        let rows = self.branch_picker_rows(project, cx);
        let selected = rows
            .iter()
            .find(|branch| !branch.current)
            .or_else(|| rows.first())
            .map(branch_reference);
        let index = rows
            .iter()
            .position(|branch| Some(branch_reference(branch)) == selected)
            .unwrap_or(0);
        let ui = &mut self.git_states[project].switcher;
        ui.selected = selected;
        ui.scroll.scroll_to_item(index, ScrollStrategy::Center);
        cx.notify();
    }

    pub(in crate::ui) fn toggle_branch_picker(
        &mut self,
        anchor: SwitchAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.state.active_project;
        if self.git_states[project].switcher.open == Some(anchor) {
            self.git_states[project].switcher.open = None;
            cx.notify();
        } else {
            self.show_branch_picker(project, anchor, window, cx);
        }
    }

    fn show_branch_picker(
        &mut self,
        project: usize,
        anchor: SwitchAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_mutating || !self.state.project_visible(project) {
            return;
        }
        if self.git_states[project].snapshot.is_none() {
            self.refresh_git(project, window, cx);
            return;
        }
        self.navigation.cancel();
        let placeholder = self.t("git_search_branches");
        let ui = &mut self.git_states[project];
        ui.cancel_worktree_navigation();
        ui.reference_form.action = None;
        ui.switcher.open = Some(anchor);
        ui.switcher.pending = None;
        ui.switcher.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(placeholder, window, cx);
        });
        self.reset_branch_picker_selection(project, cx);
        cx.notify();
    }

    pub(super) fn switch_branch(
        &mut self,
        project: usize,
        reference: Reference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_busy(project) {
            return;
        }
        let Reference::Branch { name, remote } = &reference else {
            return;
        };
        let ui = &self.git_states[project];
        let Some(snapshot) = &ui.snapshot else {
            return;
        };
        let branch = snapshot
            .branches
            .iter()
            .find(|branch| branch.name == *name && branch.remote == *remote);
        let Some(branch) = branch else {
            self.git_states[project].switcher.pending = Some(reference);
            let message = self.t("git_branch_missing");
            self.git_states[project].error.action(message);
            cx.notify();
            return;
        };
        // A remote ref may already have a local tracking branch; reuse that checkout target.
        let target = if branch.remote {
            snapshot
                .branches
                .iter()
                .find(|local| !local.remote && local.upstream == branch.name)
                .unwrap_or(branch)
        } else {
            branch
        };
        if target.current {
            self.git_states[project].switcher.open = None;
            cx.notify();
            return;
        }
        let operation = if target.remote {
            Operation::Track(target.name.clone())
        } else {
            Operation::Switch(target.name.clone())
        };
        let key = if target.remote {
            "git_track"
        } else {
            "git_switch_to_branch"
        };
        self.git_states[project].switcher.pending = Some(reference);
        self.run_git_action(project, key, operation, window, cx);
    }

    pub(in crate::ui) fn branch_switcher(
        &self,
        anchor: SwitchAnchor,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        if ui.snapshot.is_none() && ui.switcher.open.is_none() {
            return div().into_any_element();
        }
        let p = self.p();
        let open = ui.switcher.open == Some(anchor);
        let opening = cx.weak_entity();
        let owner = opening.clone();
        let branch = ui
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.branch.clone())
            .unwrap_or_else(|| self.state.projects[project].branch.clone());
        let shortcut = if tervia::platform::Platform::CURRENT == tervia::platform::Platform::Mac {
            "⌘⌥B"
        } else {
            "Ctrl+Alt+B"
        };
        Popover::new(("git-quick-switch", anchor as usize))
            .open(open)
            .anchor(if anchor == SwitchAnchor::Navigation {
                Anchor::TopRight
            } else {
                Anchor::TopLeft
            })
            .track_focus(&ui.switcher.input.focus_handle(cx))
            .trigger(
                label_button(("git-branch-switcher", anchor as usize), "", p)
                    .h(px(30.0))
                    .max_w(px(250.0))
                    .min_w_0()
                    .debug_selector(move || {
                        if anchor == SwitchAnchor::Navigation {
                            "git-branch-switcher-navigation".into()
                        } else {
                            "git-branch-switcher-toolbar".into()
                        }
                    })
                    .accessibility_label(self.t("git_quick_switch"))
                    .tooltip(format!(
                        "{} · {shortcut}\n{branch}",
                        self.t("git_quick_switch")
                    ))
                    .disabled(self.git_mutating)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .min_w_0()
                            .gap(px(6.0))
                            .child(icon(IconName::GitBranch, p.secondary))
                            .child(div().min_w_0().text_ellipsis().child(branch))
                            .child(icon(IconName::ChevronDown, p.secondary).size(px(12.0))),
                    ),
            )
            .on_open_change(move |open, window, cx| {
                let open = *open;
                let owner = opening.clone();
                // Row actions can dismiss this popover while the workspace is already updating.
                window.defer(cx, move |window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if open {
                            this.show_branch_picker(project, anchor, window, cx);
                        } else if this.git_states[project].switcher.open == Some(anchor) {
                            this.git_states[project].switcher.open = None;
                            cx.notify();
                        }
                    });
                });
            })
            .content(move |_, _, cx| {
                let close = cx.weak_entity();
                owner
                    .update(cx, |this, cx| this.branch_picker(project, close, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .into_any_element()
    }

    fn branch_picker(
        &self,
        project: usize,
        close: WeakEntity<PopoverState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = &self.git_states[project];
        let p = self.p();
        let rows = self.branch_picker_rows(project, cx);
        let count = rows.len();
        let busy = self.git_action_busy(project);
        let cancel = close.clone();
        let escape = close.clone();
        div()
            .id("git-branch-picker")
            .debug_selector(|| "git-branch-picker".into())
            .w(px(360.0))
            .max_h(px(430.0))
            .min_h_0()
            // Full-height scroll wrappers collapse during the popover's content measurement.
            .overflow_y_scroll()
            // The search focus ring extends outside its border into this clipped scroll area.
            .px(px(4.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(p.text)
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    if let Some(reference) = this.git_states[project].switcher.selected.clone() {
                        this.switch_branch(project, reference, window, cx);
                    }
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::component::input::Enter, window, cx| {
                    if let Some(reference) = this.git_states[project].switcher.selected.clone() {
                        this.switch_branch(project, reference, window, cx);
                    }
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    let _ = cancel.update(cx, |state, cx| state.dismiss(window, cx));
                    this.git_states[project].switcher.open = None;
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    let _ = escape.update(cx, |state, cx| state.dismiss(window, cx));
                    this.git_states[project].switcher.open = None;
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers != Modifiers::default()
                    || !matches!(event.keystroke.key.as_str(), "up" | "down")
                {
                    return;
                }
                let rows = this.branch_picker_rows(project, cx);
                let selected = &this.git_states[project].switcher.selected;
                let current = rows
                    .iter()
                    .position(|branch| Some(branch_reference(branch)) == *selected);
                let index = match (event.keystroke.key.as_str(), current) {
                    ("up", Some(index)) => index.saturating_sub(1),
                    ("up", None) => rows.len().saturating_sub(1),
                    (_, Some(index)) => (index + 1).min(rows.len().saturating_sub(1)),
                    _ => 0,
                };
                let ui = &mut this.git_states[project].switcher;
                ui.selected = rows.get(index).map(branch_reference);
                ui.scroll.scroll_to_item(index, ScrollStrategy::Center);
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(12.0))
                    .child(self.t("git_quick_switch")),
            )
            .child(
                Input::new(&ui.switcher.input)
                    .aria_label(self.t("git_search_branches"))
                    .w_full(),
            )
            .when(
                ui.switcher.pending.is_some() && !ui.error.is_empty(),
                |panel| {
                    panel.child(
                        div()
                            .debug_selector(|| "git-branch-picker-error".into())
                            .child(self.git_error(project, cx)),
                    )
                },
            )
            .child(if rows.is_empty() {
                div()
                    .h(px(44.0))
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .text_color(p.secondary)
                    .text_size(px(12.0))
                    .child(self.t("git_no_matching_branch"))
                    .into_any_element()
            } else {
                uniform_list(
                    "git-switch-rows",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let branch = &rows[row];
                                let reference = branch_reference(branch);
                                let selected = this.git_states[project].switcher.selected.as_ref()
                                    == Some(&reference);
                                let recent = this.git_states[project]
                                    .switcher
                                    .recent
                                    .contains(&reference);
                                label_button(
                                    SharedString::from(format!(
                                        "git-quick-branch-{}-{}",
                                        branch.remote, branch.name
                                    )),
                                    "",
                                    p,
                                )
                                .debug_selector(move || format!("git-quick-branch-row-{row}"))
                                .h(px(36.0))
                                .w_full()
                                .justify_start()
                                .disabled(busy)
                                .bg(if selected {
                                    p.raised
                                } else {
                                    p.sidebar.opacity(0.0)
                                })
                                .accessibility_label(branch.name.clone())
                                .tooltip(branch.name.clone())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .min_w_0()
                                        .w_full()
                                        .child(icon(
                                            if branch.current {
                                                IconName::Check
                                            } else {
                                                IconName::GitBranch
                                            },
                                            if branch.current {
                                                p.accent
                                            } else {
                                                p.secondary
                                            },
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_ellipsis()
                                                .child(branch.name.clone()),
                                        )
                                        .when(
                                            branch.remote || (!branch.current && recent),
                                            |row| {
                                                row.child(
                                                    div()
                                                        .text_size(px(10.0))
                                                        .text_color(p.secondary)
                                                        .child(this.t(if branch.remote {
                                                            "git_remote_branches"
                                                        } else {
                                                            "git_recent_branch"
                                                        })),
                                                )
                                            },
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.switch_branch(project, reference.clone(), window, cx)
                                }))
                                .into_any_element()
                            })
                            .collect()
                    }),
                )
                .h(px((count as f32 * 36.0).min(216.0)))
                .flex_shrink_0()
                .scrollbar(&ui.switcher.scroll)
                .into_any_element()
            })
            // Keep polling and checkout progress from shifting the selectable rows.
            .child(
                div()
                    .h(px(24.0))
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(p.chrome_border())
                    .pt(px(8.0))
                    .text_size(px(10.0))
                    .text_color(p.secondary)
                    .child(
                        self.t(if self.git_mutating && ui.switcher.pending.is_some() {
                            "git_running"
                        } else {
                            "git_branch_picker_hint"
                        }),
                    ),
            )
            .into_any_element()
    }
}

impl GitUi {
    pub(in crate::ui) fn close_branch_picker(&mut self) -> bool {
        self.switcher.open.take().is_some()
    }
}
