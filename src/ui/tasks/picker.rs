use super::*;

impl Tervia {
    pub(in crate::ui) fn project_tasks(&self, cx: &mut Context<Self>) -> Div {
        use gpui_kit::assets::IconName;
        let p = self.p();
        let rows = self.task_rows(cx);
        let count = rows.len();
        let selected = self.tasks.selected;
        let root = self
            .tasks
            .owner
            .map(|owner| {
                self.state.projects[owner]
                    .root
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();
        let content = div()
            .id("project-tasks")
            .debug_selector(|| "project-tasks".into())
            .w(px(600.0))
            .max_w_full()
            .max_h_full()
            .min_h_0()
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.sidebar)
            .overflow_hidden()
            .flex()
            .flex_col()
            .p(px(10.0))
            .gap(px(8.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    this.close_project_tasks(window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    this.close_project_tasks(window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    this.confirm_project_task(window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers != Modifiers::default() {
                    return;
                }
                if matches!(event.keystroke.key.as_str(), "up" | "down") {
                    this.tasks.selected = if event.keystroke.key == "up" {
                        this.tasks.selected.saturating_sub(1)
                    } else {
                        (this.tasks.selected + 1).min(count.saturating_sub(1))
                    };
                    this.tasks
                        .scroll
                        .scroll_to_item(this.tasks.selected, ScrollStrategy::Center);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.t("tasks_title")),
                    )
                    .child(
                        icon_button(
                            "task-refresh",
                            IconName::RefreshCw,
                            self.t("tasks_refresh"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("tasks_refresh"))
                        .disabled(self.tasks.loading)
                        .on_click(cx.listener(|this, _, _, cx| this.discover_tasks(cx))),
                    )
                    .child(
                        icon_button("task-close", IconName::X, self.t("close"), p, cx)
                            .accessibility_label(self.t("close"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_project_tasks(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .min_w_0()
                    .max_w_full()
                    .overflow_hidden()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(root),
            )
            .child(
                Input::new(&self.tasks.input)
                    .w_full()
                    .flex_shrink_0()
                    .small()
                    .prefix(icon(IconName::Search, p.secondary)),
            )
            .when(!self.tasks.catalog.warnings.is_empty(), |content| {
                content.child(
                    div()
                        .id("task-warnings")
                        .flex_shrink_0()
                        .max_h(px(80.0))
                        .overflow_y_scroll()
                        .text_size(px(12.0))
                        .text_color(p.red)
                        .child(format!(
                            "{}\n{}",
                            self.t("tasks_discovery_warning"),
                            self.tasks.catalog.warnings.join("\n")
                        )),
                )
            })
            .child(if self.tasks.loading || count == 0 {
                div()
                    .py(px(22.0))
                    .px(px(8.0))
                    .text_color(p.secondary)
                    .child(self.t(if self.tasks.loading {
                        "tasks_loading"
                    } else if self.tasks.catalog.tasks.is_empty() {
                        "tasks_empty"
                    } else {
                        "tasks_no_match"
                    }))
                    .into_any_element()
            } else {
                uniform_list(
                    "task-list",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let index = rows[row];
                                let task = &this.tasks.catalog.tasks[index];
                                let command = task.display();
                                Button::new(("project-task", row))
                                    .ghost()
                                    .compact()
                                    .w_full()
                                    .h(px(52.0))
                                    .justify_start()
                                    .debug_selector(move || format!("project-task-{row}"))
                                    .bg(if selected == row {
                                        p.selected
                                    } else {
                                        p.sidebar.opacity(0.0)
                                    })
                                    .accessibility_label(format!(
                                        "{} · {} · {command}",
                                        task.label, task.source
                                    ))
                                    .tooltip(command.clone())
                                    .child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(9.0))
                                            .child(icon(IconName::Play, p.accent))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(
                                                        div()
                                                            .text_ellipsis()
                                                            .child(task.label.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .text_color(p.secondary)
                                                            .text_ellipsis()
                                                            .child(command),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(p.secondary)
                                                    .child(task.source.clone()),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.tasks.selected = row;
                                        this.confirm_project_task(window, cx);
                                    }))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .h(px((count as f32 * 52.0).min(260.0)))
                .min_h_0()
                .track_scroll(&self.tasks.scroll)
                .into_any_element()
            })
            .child(
                div()
                    .text_size(px(11.0))
                    .flex_shrink_0()
                    .text_color(p.secondary)
                    .debug_selector(|| "task-picker-hint".into())
                    .child(self.t("tasks_hint")),
            );
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(80.0))
            .pb(px(16.0))
            .px(px(16.0))
            .bg(rgba(0x00000066))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_project_tasks(window, cx)),
            )
            .child(content)
    }
}
