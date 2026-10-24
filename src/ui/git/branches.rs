use super::super::controls::icon;
use super::history::Reference;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    popover::Popover,
};

#[derive(Clone)]
pub(super) enum ReferenceEdit {
    Branch,
    Tag,
    Rename(String),
}
impl ReferenceEdit {
    fn tag(&self) -> bool {
        matches!(self, Self::Tag)
    }
    fn key(&self) -> &'static str {
        match self {
            Self::Branch => "git_create_branch",
            Self::Tag => "git_create_tag",
            Self::Rename(_) => "git_rename",
        }
    }
    fn operation(&self, name: String) -> Operation {
        match self {
            Self::Branch => Operation::CreateBranch(name),
            Self::Tag => Operation::CreateTag(name),
            Self::Rename(old) => Operation::RenameBranch {
                old: old.clone(),
                new: name,
            },
        }
    }
}
pub(super) struct ReferenceForm {
    pub input: Entity<InputState>,
    pub action: Option<ReferenceEdit>,
}
impl ReferenceForm {
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            input: cx.new(|cx| InputState::new(window, cx)),
            action: None,
        }
    }
}

impl Tervia {
    fn reference_menu_item(
        &self,
        project: usize,
        key: &'static str,
        operation: Operation,
        view: WeakEntity<Self>,
    ) -> PopupMenuItem {
        PopupMenuItem::new(self.t(key))
            .disabled(
                self.git_states[project].busy
                    || self.git_mutating
                    || self.saving_file
                    || self.file_reads.has_reloads(),
            )
            .on_click(move |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.run_git_action(project, key, operation.clone(), window, cx)
                });
            })
    }

    pub(super) fn reference_menu(
        &self,
        project: usize,
        reference: &Reference,
        mut menu: PopupMenu,
        view: WeakEntity<Self>,
    ) -> PopupMenu {
        menu = menu.label(reference.name().to_owned());
        match reference {
            Reference::Branch { name, remote } => {
                let current = self.git_states[project]
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| {
                        snapshot.branches.iter().any(|branch| {
                            branch.name == *name && branch.remote == *remote && branch.current
                        })
                    });
                if !current {
                    menu = menu
                        .item(self.reference_menu_item(
                            project,
                            if *remote {
                                "git_track"
                            } else {
                                "git_switch_to_branch"
                            },
                            if *remote {
                                Operation::Track(name.clone())
                            } else {
                                Operation::Switch(name.clone())
                            },
                            view.clone(),
                        ))
                        .separator()
                        .item(self.reference_menu_item(
                            project,
                            "git_merge",
                            Operation::Merge(name.clone()),
                            view.clone(),
                        ))
                        .item(self.reference_menu_item(
                            project,
                            "git_rebase",
                            Operation::Rebase(name.clone()),
                            view.clone(),
                        ));
                }
                if !remote {
                    let name = name.clone();
                    let owner = view.clone();
                    menu = menu.separator().item(
                        PopupMenuItem::new(self.t("git_rename"))
                            .disabled(self.git_states[project].busy || self.git_mutating)
                            .on_click(move |_, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.show_reference_form(
                                        project,
                                        ReferenceEdit::Rename(name.clone()),
                                        window,
                                        cx,
                                    )
                                });
                            }),
                    );
                    if !current {
                        menu = menu.item(self.reference_menu_item(
                            project,
                            "git_delete",
                            Operation::DeleteBranch(reference.name().into()),
                            view,
                        ));
                    }
                }
            }
            Reference::Tag(name) => {
                menu = menu.item(self.reference_menu_item(
                    project,
                    "git_delete",
                    Operation::DeleteTag(name.clone()),
                    view,
                ));
            }
        }
        menu
    }

    pub(super) fn history_reference_actions(
        &self,
        project: usize,
        snapshot: &Snapshot,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.git_p();
        let mut actions = div().flex().items_center().gap(px(4.0));
        let Some(reference) = self.git_states[project].history.target.clone() else {
            return actions;
        };
        if let Reference::Branch { name, remote } = &reference
            && let Some(branch) = snapshot
                .branches
                .iter()
                .find(|branch| branch.name == *name && branch.remote == *remote)
        {
            if !branch.current {
                actions = actions.child(
                    self.git_action_for(
                        project,
                        "git-history-switch",
                        if *remote {
                            "git_track"
                        } else {
                            "git_switch_to_branch"
                        },
                        if *remote {
                            Operation::Track(name.clone())
                        } else {
                            Operation::Switch(name.clone())
                        },
                        cx,
                    )
                    .debug_selector(|| "git-history-switch".into()),
                );
            } else {
                actions = actions.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.secondary)
                        .child(self.t("git_current_branch")),
                );
            }
        }
        let owner = cx.weak_entity();
        actions.child(
            label_button("git-history-more", "", p, cx)
                .icon(icon(IconName::Ellipsis, p.secondary))
                .accessibility_label(self.t("git_reference_actions"))
                .tooltip(self.t("git_reference_actions"))
                .dropdown_menu(move |menu, _, cx| {
                    if let Some(view) = owner.upgrade() {
                        view.read(cx)
                            .reference_menu(project, &reference, menu, owner.clone())
                    } else {
                        menu
                    }
                }),
        )
    }

    pub(super) fn show_reference_form(
        &mut self,
        project: usize,
        action: ReferenceEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_states[project].busy || self.git_mutating {
            return;
        }
        let placeholder = self.t(if matches!(action, ReferenceEdit::Rename(_)) {
            "git_new_name"
        } else {
            "git_name"
        });
        let value = if let ReferenceEdit::Rename(name) = &action {
            name.clone()
        } else {
            String::new()
        };
        let ui = &mut self.git_states[project];
        ui.reference_form.action = Some(action);
        ui.reference_form.input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.set_placeholder(placeholder, window, cx);
        });
        cx.notify();
    }

    fn submit_reference_form(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &self.git_states[project];
        let Some(action) = &ui.reference_form.action else {
            return;
        };
        let name = ui.reference_form.input.read(cx).value().to_string();
        if name.trim().is_empty() {
            return;
        }
        self.run_git_action(project, action.key(), action.operation(name), window, cx);
    }

    pub(super) fn reference_form_trigger(
        &self,
        project: usize,
        tag: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = &self.git_states[project];
        let p = self.git_p();
        let open = ui
            .reference_form
            .action
            .as_ref()
            .is_some_and(|action| action.tag() == tag);
        let key = if tag {
            "git_create_tag"
        } else {
            "git_create_branch"
        };
        let opening = cx.weak_entity();
        let owner = opening.clone();
        Popover::new(("git-reference-popover", usize::from(tag)))
            .open(open)
            .track_focus(&ui.reference_form.input.focus_handle(cx))
            .trigger(
                label_button(
                    if tag {
                        "git-create-tag-trigger"
                    } else {
                        "git-create-branch-trigger"
                    },
                    "",
                    p,
                    cx,
                )
                .debug_selector(move || {
                    if tag {
                        "git-create-tag-trigger".into()
                    } else {
                        "git-create-branch-trigger".into()
                    }
                })
                .icon(icon(IconName::Plus, p.secondary))
                .tooltip(self.t(key))
                .accessibility_label(self.t(key))
                .disabled(ui.busy || self.git_mutating),
            )
            .on_open_change(move |open, window, cx| {
                let open = *open;
                let owner = opening.clone();
                // Dismissal may already hold the workspace update lease.
                window.defer(cx, move |window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if open {
                            if this.git_states[project].reference_form.action.is_none() {
                                this.show_reference_form(
                                    project,
                                    if tag {
                                        ReferenceEdit::Tag
                                    } else {
                                        ReferenceEdit::Branch
                                    },
                                    window,
                                    cx,
                                );
                            }
                        } else if this.git_states[project]
                            .reference_form
                            .action
                            .as_ref()
                            .is_some_and(|action| action.tag() == tag)
                        {
                            this.git_states[project].reference_form.action = None;
                            cx.notify();
                        }
                    });
                });
            })
            .content(move |_, _, cx| {
                let close = cx.weak_entity();
                owner
                    .update(cx, |this, cx| {
                        let ui = &this.git_states[project];
                        let Some(action) = &ui.reference_form.action else {
                            return div().into_any_element();
                        };
                        let title = this.t(action.key());
                        let name = ui.reference_form.input.read(cx).value().to_string();
                        let operation = action.operation(name.clone());
                        let key = action.key();
                        let cancel = close.clone();
                        let escape = close.clone();
                        div()
                            .id("git-reference-form")
                            .debug_selector(|| "git-reference-form".into())
                            .w(px(280.0))
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .text_color(p.text)
                            .capture_action(cx.listener(
                                move |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                                    this.submit_reference_form(project, window, cx);
                                    cx.stop_propagation();
                                },
                            ))
                            .capture_action(cx.listener(
                                move |this, _: &gpui_kit::component::input::Enter, window, cx| {
                                    this.submit_reference_form(project, window, cx);
                                    cx.stop_propagation();
                                },
                            ))
                            .capture_action(cx.listener(
                                move |this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                                    let _ =
                                        cancel.update(cx, |state, cx| state.dismiss(window, cx));
                                    this.git_states[project].reference_form.action = None;
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            ))
                            .capture_action(cx.listener(
                                move |this, _: &gpui_kit::component::input::Escape, window, cx| {
                                    let _ =
                                        escape.update(cx, |state, cx| state.dismiss(window, cx));
                                    this.git_states[project].reference_form.action = None;
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            ))
                            .child(div().font_weight(FontWeight::MEDIUM).child(title))
                            .when(!matches!(action, ReferenceEdit::Rename(_)), |form| {
                                form.child(
                                    div().text_size(px(11.0)).text_color(p.secondary).child(
                                        this.f(
                                            "git_reference_base",
                                            &[(
                                                "branch",
                                                ui.snapshot
                                                    .as_ref()
                                                    .map(|snapshot| snapshot.status.branch.as_str())
                                                    .unwrap_or("HEAD"),
                                            )],
                                        ),
                                    ),
                                )
                            })
                            .child(
                                Input::new(&ui.reference_form.input)
                                    .aria_label(this.t("git_name"))
                                    .w_full(),
                            )
                            .child(
                                this.git_action_for(
                                    project,
                                    "git-reference-submit",
                                    key,
                                    operation,
                                    cx,
                                )
                                .disabled(ui.busy || this.git_mutating || name.trim().is_empty())
                                .w_full(),
                            )
                            .into_any_element()
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .into_any_element()
    }
}
