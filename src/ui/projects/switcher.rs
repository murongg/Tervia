use super::*;
use gpui_kit::component::input::{Input, InputState};
use tervia::state::ProjectMatch;

pub(in crate::ui) struct Switcher {
    pub input: Entity<InputState>,
    pub open: bool,
    pub closed_only: bool,
    pub selected: usize,
    pub scroll: UniformListScrollHandle,
    pub previous_focus: Option<FocusHandle>,
}

impl Switcher {
    pub fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            input: cx.new(|cx| InputState::new(window, cx)),
            open: false,
            closed_only: false,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            previous_focus: None,
        }
    }
}

impl Tervia {
    pub(in crate::ui) fn show_project_switcher(
        &mut self,
        closed_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.cancel();
        for ui in &mut self.git_states {
            ui.close_branch_picker();
        }
        self.state.palette_open = false;
        let placeholder = self.t("project_search");
        let switcher = &mut self.navigation.switcher;
        if !switcher.open {
            switcher.previous_focus = window.focused(cx);
        }
        switcher.open = true;
        switcher.closed_only = closed_only;
        switcher.selected = 0;
        switcher.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(placeholder, window, cx);
        });
        switcher.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn close_project_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.switcher.open = false;
        self.navigation
            .switcher
            .previous_focus
            .take()
            .unwrap_or_else(|| self.focus.clone())
            .focus(window, cx);
        cx.notify();
    }

    fn project_switcher_rows(&self, cx: &App) -> Vec<ProjectMatch> {
        self.state.search_projects(
            &self.navigation.switcher.input.read(cx).value(),
            self.navigation.switcher.closed_only,
        )
    }

    pub(in crate::ui) fn confirm_project_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.switcher.open
            && let Some(found) = self
                .project_switcher_rows(cx)
                .get(self.navigation.switcher.selected)
                .copied()
        {
            self.select_project_match(found, window, cx);
        }
    }

    fn select_project_match(
        &mut self,
        found: ProjectMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_project_switcher(window, cx);
        if self.state.project_closed(found.owner) {
            if found.directory {
                self.reopen_project_target(found.owner, Some(found.index), window, cx);
            } else {
                self.reopen_closed_project(found.owner, window, cx);
            }
        } else {
            self.change_project(found.index, window, cx);
        }
    }

    pub(in crate::ui) fn project_switcher(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let rows = self.project_switcher_rows(cx);
        let count = rows.len();
        let selected = self.navigation.switcher.selected;
        let content = div()
            .id("project-switcher")
            .debug_selector(|| "project-switcher".into())
            .w(px(520.0))
            .max_w_full()
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.sidebar)
            .flex()
            .flex_col()
            .p(px(10.0))
            .gap(px(8.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    this.close_project_switcher(window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    this.close_project_switcher(window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    this.confirm_project_switcher(window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers != Modifiers::default() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        let switcher = &mut this.navigation.switcher;
                        switcher.selected = if event.keystroke.key == "up" {
                            switcher.selected.saturating_sub(1)
                        } else {
                            (switcher.selected + 1).min(count.saturating_sub(1))
                        };
                        switcher
                            .scroll
                            .scroll_to_item(switcher.selected, ScrollStrategy::Center);
                        cx.stop_propagation();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .px(px(4.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.t(if self.navigation.switcher.closed_only {
                        "project_recent_closed"
                    } else {
                        "project_quick_switch"
                    })),
            )
            .child(
                Input::new(&self.navigation.switcher.input)
                    .aria_label(self.t("project_search"))
                    .w_full(),
            )
            .child(if rows.is_empty() {
                div()
                    .h(px(64.0))
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .text_color(p.secondary)
                    .child(self.t("project_search_empty"))
                    .into_any_element()
            } else {
                uniform_list(
                    "project-switcher-list",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let found = rows[row];
                                let name = if found.directory {
                                    this.state.projects[found.index].name.clone()
                                } else {
                                    this.state.project_label(found.owner)
                                };
                                let path = if found.directory {
                                    &this.state.projects[found.index].root
                                } else {
                                    this.state.project_root(found.owner)
                                };
                                let path = path.to_string_lossy().into_owned();
                                list_row(("project-match", row), selected == row, p, cx)
                                    .h(px(52.0))
                                    .debug_selector(move || format!("project-match-{row}"))
                                    .accessibility_label(format!("{name} · {path}"))
                                    .tooltip(path.clone())
                                    .child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(9.0))
                                            .child(icon(
                                                if this.state.project_favorite(found.owner) {
                                                    IconName::Star
                                                } else {
                                                    IconName::Folder
                                                },
                                                p.secondary,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(div().text_ellipsis().child(name))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .text_color(p.secondary)
                                                            .text_ellipsis()
                                                            .child(path),
                                                    ),
                                            )
                                            .when(this.state.project_closed(found.owner), |row| {
                                                row.child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(p.secondary)
                                                        .child(this.t("project_closed")),
                                                )
                                            }),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_project_match(found, window, cx)
                                    }))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .h(px((count as f32 * 52.0).min(312.0)))
                .track_scroll(&self.navigation.switcher.scroll)
                .into_any_element()
            })
            .when_some(self.state.previous_project(), |content, previous| {
                content.child(
                    list_row("previous-project", false, p, cx)
                        .label(self.f(
                            "project_previous_named",
                            &[(
                                "name",
                                &self.state.project_label(
                                    self.state.project_owner(previous).unwrap_or(previous),
                                ),
                            )],
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.close_project_switcher(window, cx);
                            this.change_project(previous, window, cx);
                        })),
                )
            });
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(80.0))
            .px(px(16.0))
            .bg(rgba(0x00000066))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_project_switcher(window, cx)),
            )
            .child(content)
    }
}
