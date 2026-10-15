use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::agent::Agent;

use super::{
    Tervia,
    controls::{icon, icon_button, separator},
};

impl Tervia {
    pub(super) fn open_terminal_link(
        &mut self,
        project: usize,
        link: tervia::terminal::FileLink,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.active_project != project {
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
                if this.state.active_project != project
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
        if !self.state.project().dock.visible {
            self.change_tab(tervia::state::Tab::Terminal, window, cx);
        }
        if self.state.project().sessions.len() == 1 {
            self.new_session(window, cx);
        }
        self.state.toggle_split();
        cx.notify();
    }

    pub(super) fn clear_terminal(&mut self, cx: &mut Context<Self>) {
        let project = self.state.active_project;
        // The focused split pane can differ from the selected session tab.
        let session = self.state.command_session();
        if let Some(console) = &self.terminals[project][session] {
            console.update(cx, |console, cx| console.clear(cx));
        }
        cx.notify();
    }

    pub(super) fn copy_terminal_output(&mut self, cx: &mut Context<Self>) {
        let project = self.state.active_project;
        let session = self.state.command_session();
        let output = self.terminals[project][session]
            .as_ref()
            .map(|console| console.read(cx).output())
            .unwrap_or_default();
        cx.write_to_clipboard(ClipboardItem::new_string(output));
        self.state.notice = "copied_output".into();
        cx.notify();
    }

    pub(super) fn terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let project = self.state.project().clone();
        let active = project.active_session;
        let mut tabs = div()
            .h(px(42.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(16.0))
            .gap(px(4.0))
            .border_b_1()
            .border_color(p.chrome_border());
        for (index, _session) in project.sessions.iter().enumerate() {
            let selected = active == index;
            let agent = self.terminals[self.state.active_project][index]
                .as_ref()
                .and_then(|console| console.read(cx).agent);
            let session_name = match agent {
                Some(agent) if index == 0 => agent.name().to_owned(),
                Some(agent) => format!("{} {}", agent.name(), index + 1),
                None => _session.name.clone(),
            };
            tabs = tabs.child(
                div()
                    .id(("session-tab", index))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .hover(|style| style.bg(p.text.opacity(0.12)))
                    .bg(if selected {
                        p.selected
                    } else {
                        p.toolbar.opacity(0.0)
                    })
                    .child(
                        Button::new(("session", index))
                            .custom(ButtonCustomVariant::new(cx).foreground(p.text))
                            .small()
                            .h(px(29.0))
                            .rounded(px(5.0))
                            .icon(agent_icon(agent, selected, p, self.appearance.light_theme))
                            .label(session_name.clone())
                            .when(
                                self.terminals[self.state.active_project][index]
                                    .as_ref()
                                    .is_some_and(|console| console.read(cx).attention),
                                |button| {
                                    button
                                        .child(icon(IconName::Bell, p.accent).size(px(11.0)))
                                        .tooltip(self.t("terminal_attention"))
                                },
                            )
                            .text_color(if selected { p.text } else { p.secondary })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.state.select_session(index);
                                this.ensure_console(index, window, cx)
                                    .read(cx)
                                    .focus
                                    .clone()
                                    .focus(window, cx);
                                cx.notify();
                            })),
                    )
                    .when(project.sessions.len() > 1, |tab| {
                        tab.child(
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
                                move |this, _, _, cx| {
                                    this.state.close_session(index);
                                    if let Some(console) =
                                        this.terminals[this.state.active_project].remove(index)
                                    {
                                        console.update(cx, |console, _| console.shutdown());
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                    }),
            );
        }
        tabs = tabs
            .when(project.tab != tervia::state::Tab::Terminal, |tabs| {
                tabs.child(
                    icon_button("close-terminal-dock", IconName::X, self.t("close"), p, cx)
                        .accessibility_label(format!(
                            "{} {}",
                            self.t("close"),
                            self.t("terminal_dock")
                        ))
                        .on_click(
                            cx.listener(|this, _, window, cx| {
                                this.toggle_terminal_dock(window, cx)
                            }),
                        ),
                )
            })
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
                .on_click(cx.listener(|this, _, _, cx| this.copy_terminal_output(cx))),
            );
        let mut panes = div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(self.terminal_pane(active, window, cx));
        if project.split_terminal {
            let second = if active == 0 && project.sessions.len() > 1 {
                1
            } else {
                0
            };
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
        div().flex_1().min_w_0().min_h_0().child(console)
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
