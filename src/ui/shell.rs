use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::platform::Platform;
use tervia::state::Tab;

use super::{
    Tervia,
    controls::{badge, icon, icon_button, label_button, separator},
};

impl Tervia {
    pub(super) fn titlebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        div()
            .h(px(48.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(16.0))
            .pl(px(if Platform::CURRENT == Platform::Mac {
                88.0
            } else {
                16.0
            }))
            .child(
                icon_button(
                    "toggle-sidebar",
                    IconName::PanelLeft,
                    self.t("toggle_projects_shortcut"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("toggle_projects_label"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.toggle_sidebar();
                    cx.notify();
                })),
            )
            .child(separator(p))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .ml(px(8.0))
                    .child(icon(IconName::Folder, p.secondary))
                    .child(
                        div()
                            .text_color(p.text)
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.state.project_label(self.state.active_project)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
            )
            .child(
                label_button("open-palette", self.t("quick_actions"), p)
                    .icon(icon(IconName::Command, p.secondary))
                    .tooltip(Platform::CURRENT.hint(self.t("quick_actions_shortcut")))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.state.palette_open = !this.state.palette_open;
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            )
            .child(
                div().ml(px(12.0)).child(
                    icon_button(
                        "theme",
                        if self.appearance.light_theme {
                            IconName::Moon
                        } else {
                            IconName::Sun
                        },
                        self.t("toggle_theme"),
                        p,
                        cx,
                    )
                    .accessibility_label(self.t("toggle_theme"))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx))),
                ),
            )
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let expanded = self.state.sidebar_expanded;
        let mut rows = div()
            .id("project-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(8.0))
            .py(px(12.0));
        if expanded {
            rows = rows.child(
                div()
                    .px(px(8.0))
                    .mb(px(8.0))
                    .text_size(px(11.0))
                    .text_color(p.muted)
                    .child(self.t("projects")),
            );
        }
        for index in self.state.project_rows() {
            let target = self.state.project_target(index);
            let project = &self.state.projects[target];
            let name = self.state.project_label(index);
            let active = self.state.same_project(index, self.state.active_project);
            let avatar = div()
                .size(px(28.0))
                .flex_shrink_0()
                .rounded(px(7.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(if active { p.accent } else { p.secondary })
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(13.0))
                .child(
                    name.chars()
                        .next()
                        .unwrap_or('T')
                        .to_uppercase()
                        .to_string(),
                );
            let mut content = div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .w_full()
                .child(avatar);
            if expanded {
                content = content.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(if active { p.text } else { p.secondary })
                                .child(name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(p.muted)
                                .text_ellipsis()
                                .child(project.branch.clone()),
                        ),
                );
            }
            rows = rows.child(
                Button::new(("project", index))
                    .ghost()
                    .compact()
                    .w_full()
                    .h(px(44.0))
                    .rounded(px(8.0))
                    .bg(if active {
                        p.selected
                    } else {
                        p.sidebar.opacity(0.0)
                    })
                    .accessibility_label(self.f("switch_project", &[("name", &name)]))
                    .tooltip(format!("{}\n{}", name, project.path))
                    .child(content)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change_project(this.state.project_target(index), window, cx)
                    })),
            );
        }
        div()
            .w(px(if expanded { 196.0 } else { 56.0 }))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.chrome_border())
            .child(rows)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(8.0))
                    .pb(px(12.0))
                    .child(
                        Button::new("add-project")
                            .ghost()
                            .compact()
                            .h(px(36.0))
                            .w_full()
                            .icon(icon(IconName::Plus, p.secondary))
                            .when(expanded, |button| button.label(self.t("add_project")))
                            .accessibility_label(self.t("git_open_repository"))
                            .tooltip(self.t("git_open_repository"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_repository(window, cx);
                            })),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .compact()
                            .h(px(36.0))
                            .w_full()
                            .icon(icon(IconName::Settings, p.secondary))
                            .when(expanded, |button| button.label(self.t("settings")))
                            .accessibility_label(self.t("settings"))
                            .tooltip(self.t("appearance_shortcuts"))
                            .on_click(cx.listener(|this, _, _, cx| this.open_preferences(cx))),
                    ),
            )
    }

    pub(super) fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let active = self.state.project().tab;
        let count = self.git_states[self.state.active_project]
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.files.len())
            .unwrap_or(0);
        let mut nav = div()
            .h(px(49.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(18.0));
        for (index, (tab, label, glyph)) in [
            (Tab::Terminal, self.t("terminal"), IconName::SquareTerminal),
            (Tab::Git, "Git", IconName::GitBranch),
            (Tab::Files, self.t("files"), IconName::Files),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = active == tab;
            nav = nav.child(
                Button::new(("module", index))
                    .ghost()
                    .small()
                    .h(px(32.0))
                    .px(px(12.0))
                    .rounded(px(6.0))
                    .bg(if selected {
                        p.selected
                    } else {
                        p.background.opacity(0.0)
                    })
                    .text_color(if selected { p.text } else { p.secondary })
                    .icon(icon(glyph, if selected { p.accent } else { p.secondary }))
                    .label(label)
                    .tooltip(Platform::CURRENT.hint(&format!("{label} · ⌘{}", index + 1)))
                    .when(tab == Tab::Git && count > 0, |button| {
                        button.child(badge(count.to_string(), p.secondary, p))
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.change_tab(tab, window, cx)),
                    ),
            );
        }
        nav.child(div().flex_1())
            .when(active != Tab::Terminal, |nav| {
                nav.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_color(p.secondary)
                        .text_size(px(11.0))
                        .child(icon(IconName::GitBranch, p.muted))
                        .child(self.state.project().branch.clone()),
                )
            })
    }

    pub(super) fn statusbar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let project = self.state.project();
        let context = match project.tab {
            Tab::Terminal => {
                let shell = tervia::pty::default_shell();
                format!(
                    "UTF-8  ·  {}",
                    std::path::Path::new(&shell)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            }
            Tab::Git => self.f(
                "changed_files",
                &[(
                    "count",
                    &self.git_states[self.state.active_project]
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.status.files.len())
                        .unwrap_or(0)
                        .to_string(),
                )],
            ),
            Tab::Files => format!(
                "{}  ·  UTF-8  ·  LF",
                project
                    .files
                    .get(project.active_file)
                    .map(|file| file.language.to_uppercase())
                    .unwrap_or_default()
            ),
        };
        let path = if project.tab == Tab::Terminal {
            self.terminals[self.state.active_project][self.state.command_session()]
                .as_ref()
                .map(|console| {
                    let cwd = &console.read(cx).cwd;
                    if std::env::var_os("HOME")
                        .is_some_and(|home| cwd == std::path::Path::new(&home))
                    {
                        "~".to_owned()
                    } else {
                        cwd.to_string_lossy().into_owned()
                    }
                })
                .unwrap_or_else(|| project.path.clone())
        } else if project.tab == Tab::Git {
            self.git_states[self.state.active_project]
                .repository
                .as_ref()
                .map(|repository| repository.root.to_string_lossy().into_owned())
                .unwrap_or_else(|| project.path.clone())
        } else {
            project.path.clone()
        };
        div()
            .h(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(16.0))
            .border_t_1()
            .border_color(p.chrome_border())
            .text_size(px(11.0))
            .text_color(p.secondary)
            .child(icon(IconName::Folder, p.muted))
            .child(path)
            .child(
                div()
                    .ml(px(10.0))
                    .flex_1()
                    .text_ellipsis()
                    .text_color(p.accent)
                    .child(self.appearance.language.text(&self.state.notice).to_owned()),
            )
            .child(div().text_color(p.muted).child(self.t("git_live")))
            .child(separator(p))
            .child(context)
            .child(
                icon_button(
                    "help",
                    IconName::CircleQuestionMark,
                    self.t("shortcuts_info"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("settings"))
                .on_click(cx.listener(|this, _, _, cx| this.open_preferences(cx))),
            )
    }

    pub(super) fn palette(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let mut commands = div()
            .w(px(460.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(p.border)
            .bg(p.sidebar)
            .flex()
            .flex_col()
            .p(px(12.0))
            .gap(px(5.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .py(px(8.0))
                    .child(icon(IconName::Command, p.accent))
                    .child(div().ml(px(10.0)).flex_1().child(self.t("quick_actions")))
                    .child(
                        icon_button("close-palette", IconName::X, self.t("close"), p, cx)
                            .accessibility_label(self.t("close_quick_actions"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.palette_open = false;
                                cx.notify();
                            })),
                    ),
            );
        for (index, (tab, label, key)) in [
            (Tab::Terminal, self.t("open_terminal"), "⌘1"),
            (Tab::Git, self.t("review_git"), "⌘2"),
            (Tab::Files, self.t("browse_files"), "⌘3"),
        ]
        .into_iter()
        .enumerate()
        {
            commands = commands.child(
                Button::new(("palette-tab", index))
                    .ghost()
                    .w_full()
                    .h(px(38.0))
                    .compact()
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .child(div().flex_1().child(label))
                            .child(div().text_color(p.muted).child(key)),
                    )
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.state.palette_open = false;
                        this.change_tab(tab, window, cx);
                    })),
            );
        }
        commands = commands
            .child(
                Button::new("palette-sidebar")
                    .ghost()
                    .w_full()
                    .h(px(38.0))
                    .compact()
                    .label(self.t("toggle_projects"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.toggle_sidebar();
                        this.state.palette_open = false;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("palette-terminal")
                    .ghost()
                    .w_full()
                    .h(px(38.0))
                    .compact()
                    .label(self.t("new_session"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.state.palette_open = false;
                        this.state.set_tab(Tab::Terminal);
                        this.new_session(window, cx);
                    })),
            );
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(125.0))
            .bg(rgba(0x00000066))
            .child(commands)
    }
}
