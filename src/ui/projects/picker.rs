use super::*;
use gpui_kit::component::{
    input::Input,
    popover::{Popover, PopoverState},
};

impl Tervia {
    fn working_directory_name(&self, parent: usize, index: usize) -> String {
        if index == parent {
            self.t("working_directory_root").to_owned()
        } else {
            self.state.projects[index]
                .root
                .strip_prefix(&self.state.projects[parent].root)
                .unwrap_or(&self.state.projects[index].root)
                .to_string_lossy()
                .into_owned()
        }
    }

    pub(in crate::ui) fn working_directory_rows(&self, parent: usize, cx: &App) -> Vec<usize> {
        let query = self.navigation.input.read(cx).value().to_lowercase();
        self.state
            .working_directories(parent)
            .into_iter()
            .filter(|index| {
                self.working_directory_name(parent, *index)
                    .to_lowercase()
                    .contains(query.trim())
            })
            .collect()
    }

    pub(in crate::ui) fn toggle_directory_picker(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.open = open;
        if open {
            let placeholder = self.t("working_directory_search");
            self.navigation.input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.set_placeholder(placeholder, window, cx);
            });
            let rows = self.working_directory_rows(self.working_directory_owner(), cx);
            self.navigation.selected = rows
                .iter()
                .position(|index| *index == self.state.active_project)
                .unwrap_or(0);
            self.navigation
                .scroll
                .scroll_to_item(self.navigation.selected, ScrollStrategy::Center);
        }
        cx.notify();
    }

    pub(in crate::ui) fn working_directory_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p();
        let parent = self.working_directory_owner();
        let name = self.state.project_label(parent);
        let current = self.working_directory_name(parent, self.state.active_project);
        let owner = cx.weak_entity();
        let opening = owner.clone();
        Popover::new("working-directory-popover")
            .open(self.navigation.open)
            .track_focus(&self.navigation.input.focus_handle(cx))
            .anchor(Anchor::TopLeft)
            .trigger(
                Button::new("working-directory-trigger")
                    .custom(icon_style(p, cx))
                    .compact()
                    .h(px(30.0))
                    .min_w_0()
                    .max_w(px(330.0))
                    .debug_selector(|| "working-directory-switcher".into())
                    .accessibility_label(self.t("working_directory_switch"))
                    .tooltip(format!(
                        "{}\n{}",
                        tervia::platform::Platform::CURRENT
                            .hint(self.t("working_directory_shortcut"))
                            .replace(
                                '⌥',
                                if tervia::platform::Platform::CURRENT
                                    == tervia::platform::Platform::Mac
                                {
                                    "⌥"
                                } else {
                                    "Alt+"
                                }
                            ),
                        self.state.project().path
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .child(icon(IconName::Folder, p.secondary))
                            .child(
                                div()
                                    .max_w(px(120.0))
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_color(p.secondary)
                                    .child(name),
                            )
                            .child(icon(IconName::ChevronRight, p.muted).size(px(12.0)))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_ellipsis()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(current),
                            )
                            .child(icon(IconName::ChevronDown, p.secondary).size(px(12.0))),
                    ),
            )
            .on_open_change(move |open, window, cx| {
                let owner = opening.clone();
                let open = *open;
                // Dismissing from a row already holds the workspace entity's update lease.
                window.defer(cx, move |window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.toggle_directory_picker(open, window, cx)
                    });
                });
            })
            .content(move |_, _, cx| {
                let close = cx.weak_entity();
                owner
                    .update(cx, |this, cx| this.directory_picker(parent, close, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .into_any_element()
    }

    pub(in crate::ui) fn select_working_directory(
        &mut self,
        parent: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.open = false;
        if let Ok(path) = self.state.projects[index]
            .root
            .strip_prefix(&self.state.projects[parent].root)
        {
            self.activate_directory(parent, path.to_path_buf(), window, cx);
        } else {
            self.change_project(index, window, cx);
        }
    }

    fn directory_picker(
        &self,
        parent: usize,
        close: WeakEntity<PopoverState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.p();
        let rows = self.working_directory_rows(parent, cx);
        let count = rows.len();
        let cancel_close = close.clone();
        let escape_close = close.clone();
        let add_close = close.clone();
        div()
            .id("working-directory-picker")
            .debug_selector(|| "working-directory-picker".into())
            .w(px(320.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(p.text)
            // The popover also binds Enter to toggle itself; empty searches must stay open.
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    let rows = this.working_directory_rows(parent, cx);
                    if let Some(index) = rows.get(this.navigation.selected).copied() {
                        this.select_working_directory(parent, index, window, cx);
                    }
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    let _ = cancel_close.update(cx, |state, cx| state.dismiss(window, cx));
                    this.navigation.open = false;
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
            .capture_action(cx.listener(
                move |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    let _ = escape_close.update(cx, |state, cx| state.dismiss(window, cx));
                    this.navigation.open = false;
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.modifiers != Modifiers::default() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        this.navigation.selected = if event.keystroke.key == "up" {
                            this.navigation.selected.saturating_sub(1)
                        } else {
                            (this.navigation.selected + 1).min(count.saturating_sub(1))
                        };
                        this.navigation
                            .scroll
                            .scroll_to_item(this.navigation.selected, ScrollStrategy::Center);
                        cx.stop_propagation();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .px(px(2.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.t("working_directory_switch")),
            )
            .child(
                Input::new(&self.navigation.input)
                    .aria_label(self.t("working_directory_search"))
                    .w_full(),
            )
            .child(if rows.is_empty() {
                div()
                    .h(px(48.0))
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .text_size(px(12.0))
                    .text_color(p.secondary)
                    .child(self.t("working_directory_empty"))
                    .into_any_element()
            } else {
                uniform_list(
                    "working-directory-list",
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let index = rows[row];
                                let active = this.state.active_project == index;
                                let close = close.clone();
                                Button::new(("working-directory", index))
                                    .custom(icon_style(p, cx))
                                    .compact()
                                    .h(px(36.0))
                                    .w_full()
                                    .justify_start()
                                    .bg(if this.navigation.selected == row {
                                        p.raised
                                    } else {
                                        p.sidebar.opacity(0.0)
                                    })
                                    .debug_selector(move || format!("working-directory-{index}"))
                                    .tooltip(this.state.projects[index].path.clone())
                                    .accessibility_label(this.working_directory_name(parent, index))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .min_w_0()
                                            .w_full()
                                            .child(icon(
                                                if active {
                                                    IconName::Check
                                                } else {
                                                    IconName::Folder
                                                },
                                                if active { p.accent } else { p.secondary },
                                            ))
                                            .child(
                                                div().flex_1().min_w_0().text_ellipsis().child(
                                                    this.working_directory_name(parent, index),
                                                ),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let _ =
                                            close.update(cx, |state, cx| state.dismiss(window, cx));
                                        this.select_working_directory(parent, index, window, cx);
                                    }))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .h(px((count as f32 * 36.0).min(252.0)))
                .scrollbar(&self.navigation.scroll)
                .into_any_element()
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(p.chrome_border())
                    .pt(px(6.0))
                    .child(
                        Button::new("working-directory-add")
                            .ghost()
                            .compact()
                            .h(px(32.0))
                            .w_full()
                            .justify_start()
                            .debug_selector(|| "working-directory-add".into())
                            .icon(icon(IconName::Plus, p.secondary))
                            .label(self.t("working_directory_add"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let _ = add_close.update(cx, |state, cx| state.dismiss(window, cx));
                                this.choose_working_directory(parent, window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
