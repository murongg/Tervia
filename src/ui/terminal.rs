use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::{
    Disableable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::Input,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::agent::Agent;

use super::{
    Tervia,
    controls::{icon, icon_button, separator},
};

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

impl Tervia {
    pub(super) fn terminal_shell(&self, cx: &App) -> String {
        let shell = self
            .consoles()
            .get(self.state.command_session())
            .and_then(Option::as_ref)
            .map(|console| console.read(cx).shell())
            .unwrap_or_else(|| {
                self.appearance
                    .shell
                    .resolve()
                    .to_string_lossy()
                    .into_owned()
            });
        shell
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&shell)
            .to_owned()
    }

    fn shell_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let profiles = self.shell_profiles.clone();
        let language = self.appearance.language;
        icon_button(
            "new-session-shell",
            IconName::ChevronDown,
            self.t("select_shell"),
            self.p(),
            cx,
        )
        .debug_selector(|| "new-session-shell".into())
        .accessibility_label(self.t("select_shell"))
        .dropdown_menu(move |mut menu, _, _| {
            for &profile in &profiles {
                let weak = weak.clone();
                menu = menu.item(PopupMenuItem::new(profile.label(language)).on_click(
                    move |_, window, cx| {
                        let _ = weak.update(cx, |_, cx| {
                            // Restore menu focus before focusing the newly created terminal.
                            cx.defer_in(window, move |this, window, cx| {
                                this.new_session_with_shell(profile, window, cx)
                            });
                        });
                    },
                ));
            }
            menu
        })
    }

    fn terminal_session_name(&self, index: usize, cx: &App) -> String {
        let session = &self.state.terminals().sessions[index];
        if let Some(name) = &session.custom_name {
            return name.clone();
        }
        if let Some(task) = &session.task {
            return task.label.clone();
        }
        let agent = self.consoles()[index]
            .as_ref()
            .and_then(|console| console.read(cx).agent);
        match agent {
            Some(agent) if index == 0 => agent.name().to_owned(),
            Some(agent) => format!("{} {}", agent.name(), index + 1),
            None => session.name.clone(),
        }
    }

    pub(super) fn rename_terminal_session(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.state.terminals().sessions.len() {
            return;
        }
        self.terminal_rename = None;
        let name = self.terminal_session_name(index, cx);
        self.state.select_session(index);
        self.ensure_console(index, window, cx);
        self.terminal_rename = Some((self.console_owner(), index));
        self.terminal_name.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.set_placeholder(self.t("terminal_name_placeholder"), window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    fn finish_terminal_rename(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((owner, index)) = self.terminal_rename.take() else {
            return;
        };
        if owner != self.console_owner() {
            return;
        }
        if save {
            let name = self.terminal_name.read(cx).value().to_string();
            if self.state.terminals_mut().rename(index, &name) {
                self.persist_workspace(cx);
            }
        }
        self.ensure_console(self.state.terminals().active_session, window, cx)
            .read(cx)
            .focus
            .clone()
            .focus(window, cx);
        cx.notify();
    }

    fn terminal_name_input(&self, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "terminal-rename-input".into())
            .capture_action(cx.listener(
                |this, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    this.finish_terminal_rename(true, window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Enter, window, cx| {
                    this.finish_terminal_rename(true, window, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                    this.finish_terminal_rename(false, window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    this.finish_terminal_rename(false, window, cx);
                    cx.stop_propagation();
                },
            ))
            .child(
                Input::new(&self.terminal_name)
                    .small()
                    .w(px(180.0))
                    .h(px(29.0)),
            )
    }

    pub(super) fn open_terminal_link(
        &mut self,
        project: usize,
        link: tervia::terminal::FileLink,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.has_project() || self.state.active_project != project {
            return;
        }
        let root = self.state.projects[project].root.clone();
        let selection = self.file_reads.select(project);
        let line = link.line.min(u32::MAX as usize);
        let column = link.column.saturating_sub(1).min(u32::MAX as usize);
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { link.resolve(&root) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // Later directory/module navigation wins while the filesystem resolves this link.
                if !this.state.has_project()
                    || this.state.active_project != project
                    || !this.file_reads.selected(project, selection)
                {
                    return;
                }
                match result {
                    Ok(path) => this.open_file_match(project, path, line, column, window, cx),
                    Err(error) => {
                        this.state.notice = error;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.has_project() && !self.state.project().dock.visible {
            self.change_tab(tervia::state::Tab::Terminal, window, cx);
        }
        if self.state.terminals().sessions.is_empty() {
            self.new_session(window, cx);
        }
        if self.state.terminals().sessions.len() == 1 {
            self.new_session(window, cx);
        }
        self.state.toggle_split();
        cx.notify();
    }

    pub(super) fn clear_terminal(&mut self, cx: &mut Context<Self>) {
        // The focused split pane can differ from the selected session tab.
        let session = self.state.command_session();
        if let Some(console) = self.consoles().get(session).and_then(Option::as_ref) {
            console.update(cx, |console, cx| console.clear(cx));
        }
        cx.notify();
    }

    pub(super) fn copy_terminal_output(&mut self, cx: &mut Context<Self>) {
        let session = self.state.command_session();
        let output = self
            .consoles()
            .get(session)
            .and_then(Option::as_ref)
            .map(|console| console.read(cx).output())
            .unwrap_or_default();
        cx.write_to_clipboard(ClipboardItem::new_string(output));
        self.state.notice = "copied_output".into();
        cx.notify();
    }

    pub(super) fn terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let terminal = self.state.terminals().clone();
        let active = terminal.active_session;
        let mut tabs = div()
            .h(px(42.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(16.0))
            .gap(px(4.0))
            .border_b_1()
            .border_color(p.chrome_border());
        for index in 0..terminal.sessions.len() {
            let selected = active == index;
            let agent = self.consoles()[index]
                .as_ref()
                .and_then(|console| console.read(cx).agent);
            let session_name = self.terminal_session_name(index, cx);
            let owner = self.console_owner();
            let weak = cx.weak_entity();
            let rename = self.t("rename_terminal");
            let title = if self.terminal_rename == Some((owner, index)) {
                self.terminal_name_input(cx).into_any_element()
            } else {
                Button::new(("session", index))
                    .debug_selector(move || format!("session-title-{index}"))
                    .custom(ButtonCustomVariant::new(cx).foreground(p.text))
                    .small()
                    .h(px(29.0))
                    .rounded(px(5.0))
                    .icon(agent_icon(agent, selected, p, self.appearance.light_theme))
                    .label(session_name.clone())
                    .max_w(px(220.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip(format!(
                        "{}\n{}",
                        session_name,
                        self.t("terminal_rename_hint")
                    ))
                    .when(
                        self.consoles()[index]
                            .as_ref()
                            .is_some_and(|console| console.read(cx).attention),
                        |button| {
                            button
                                .child(icon(IconName::Bell, p.accent).size(px(11.0)))
                                .tooltip(self.t("terminal_attention"))
                        },
                    )
                    .text_color(if selected { p.text } else { p.secondary })
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.click_count() >= 2 {
                            this.rename_terminal_session(index, window, cx);
                            return;
                        }
                        this.state.select_session(index);
                        this.ensure_console(index, window, cx)
                            .read(cx)
                            .focus
                            .clone()
                            .focus(window, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            };
            tabs = tabs.child(
                div()
                    .id(("session-tab", index))
                    .debug_selector(move || format!("session-tab-{index}"))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .hover(|style| style.bg(p.text.opacity(0.12)))
                    .bg(if selected {
                        p.selected
                    } else {
                        p.toolbar.opacity(0.0)
                    })
                    .child(title)
                    .child(
                        icon_button(
                            ("close-session", index),
                            IconName::X,
                            self.t("close_session"),
                            p,
                            cx,
                        )
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .foreground(p.text)
                                .hover(p.text.opacity(0.16))
                                .active(p.text.opacity(0.24)),
                        )
                        .accessibility_label(self.f("close_named", &[("name", &session_name)]))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.close_terminal_session(index, window, cx)
                            },
                        )),
                    )
                    .context_menu(move |menu, _, _| {
                        let weak = weak.clone();
                        menu.item(PopupMenuItem::new(rename).on_click(move |_, window, cx| {
                            let _ = weak.update(cx, |_, cx| {
                                // Let the menu restore its focus before focusing the inline editor.
                                cx.defer_in(window, move |this, window, cx| {
                                    if this.console_owner() == owner {
                                        this.rename_terminal_session(index, window, cx);
                                    }
                                });
                            });
                        }))
                    }),
            );
        }
        tabs = tabs
            .when(
                self.state.active_tab() != tervia::state::Tab::Terminal,
                |tabs| {
                    tabs.child(
                        icon_button("close-terminal-dock", IconName::X, self.t("close"), p, cx)
                            .accessibility_label(format!(
                                "{} {}",
                                self.t("close"),
                                self.t("terminal_dock")
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_terminal_dock(window, cx)
                            })),
                    )
                },
            )
            .child(
                icon_button(
                    "new-session",
                    IconName::Plus,
                    self.t("new_terminal_shortcut"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("new_terminal"))
                .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx))),
            )
            .when(cfg!(windows), |tabs| tabs.child(self.shell_menu(cx)))
            .child(div().flex_1())
            .child(
                icon_button(
                    "split-terminal",
                    IconName::Columns2,
                    self.t("toggle_split"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("toggle_split"))
                .on_click(cx.listener(|this, _, window, cx| this.split_terminal(window, cx))),
            )
            .child(separator(p))
            .child(
                icon_button(
                    "search-terminal",
                    IconName::Search,
                    self.t("terminal_search"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("terminal_search"))
                .debug_selector(|| "terminal-search".into())
                .disabled(terminal.sessions.is_empty())
                .on_click(cx.listener(|this, _, window, cx| {
                    let console = this.ensure_console(this.state.command_session(), window, cx);
                    console.update(cx, |console, cx| console.open_search(window, cx));
                })),
            )
            .child(
                icon_button(
                    "clear-terminal",
                    IconName::Eraser,
                    self.t("clear_terminal_shortcut"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("clear_terminal"))
                .disabled(terminal.sessions.is_empty())
                .on_click(cx.listener(|this, _, _, cx| this.clear_terminal(cx))),
            )
            .child(
                icon_button(
                    "copy-terminal",
                    IconName::Copy,
                    self.t("copy_output"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("copy_terminal"))
                .disabled(terminal.sessions.is_empty())
                .on_click(cx.listener(|this, _, _, cx| this.copy_terminal_output(cx))),
            );
        let mut panes = div().flex().flex_1().min_h_0();
        if terminal.sessions.is_empty() {
            panes = panes.child(
                div()
                    .debug_selector(|| "local-terminal-empty".into())
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .text_color(p.secondary)
                            .child(self.t("local_terminal_empty")),
                    )
                    .child(
                        Button::new("empty-new-terminal")
                            .ghost()
                            .icon(icon(IconName::Plus, p.secondary))
                            .label(self.t("new_terminal"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.new_session(window, cx)),
                            ),
                    ),
            );
        } else {
            panes = panes.child(self.terminal_pane(active, window, cx));
        }
        if terminal.split_terminal && terminal.sessions.len() > 1 {
            let second = if active == 0 { 1 } else { 0 };
            panes = panes
                .child(div().w(px(1.0)).bg(p.chrome_border()))
                .child(self.terminal_pane(second, window, cx));
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(tabs)
            .child(panes)
    }

    fn terminal_pane(
        &mut self,
        session: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let console = self.ensure_console(session, window, cx);
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .when_some(self.task_run_summary(session, cx), |pane, summary| {
                pane.child(summary)
            })
            .child(console)
    }
}

fn agent_icon(agent: Option<Agent>, selected: bool, p: super::theme::Palette, light: bool) -> Icon {
    let Some(agent) = agent else {
        return icon(
            IconName::SquareTerminal,
            if selected { p.accent } else { p.muted },
        );
    };
    let (bytes, color): (&[u8], Hsla) = match agent {
        Agent::Claude => (
            include_bytes!("../../assets/claude.svg"),
            rgb(if light { 0x9f4a2e } else { 0xd97757 }).into(),
        ),
        Agent::Codex => (include_bytes!("../../assets/codex.svg"), p.text),
        Agent::Gemini => (
            include_bytes!("../../assets/gemini.svg"),
            rgb(if light { 0x31578d } else { 0x8ab4f8 }).into(),
        ),
        Agent::Grok => (include_bytes!("../../assets/grok.svg"), p.text),
        Agent::Hermes => (include_bytes!("../../assets/hermes.svg"), p.text),
        Agent::OpenCode => (include_bytes!("../../assets/opencode.svg"), p.text),
        Agent::OpenClaw => (
            include_bytes!("../../assets/openclaw.svg"),
            rgb(if light { 0x92343e } else { 0xe88480 }).into(),
        ),
    };
    Icon::default().data(bytes).size(px(15.0)).text_color(color)
}
