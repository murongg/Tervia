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
    controls::{badge, icon, icon_button, label_button, separator, titlebar_mouse_down},
};

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_test::TestAppContext;

    #[gpui_test::test]
    fn update_status_keeps_the_fixed_titlebar_actions_in_place(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut view = Tervia::new(window, cx);
                let project = view.state.open_directory("/synthetic/titlebar".into());
                view.init_project(project, window, cx);
                view.state.set_tab(Tab::Files);
                view
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        let target = tervia::updates::Target::current().unwrap();
        let release = self_update::Release::builder()
            .version("9.0.0")
            .asset(
                self_update::ReleaseAsset::new(
                    target.asset_name("9.0.0"),
                    "https://example.test/synthetic.zip",
                )
                .with_digest(format!("sha256:{}", "ab".repeat(32))),
            )
            .build()
            .unwrap();
        let candidate = tervia::updates::Candidate::from_release(release, "1.0.0", target)
            .unwrap()
            .unwrap();
        for light in [false, true] {
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.preferences.update(cx, |preferences, cx| {
                        preferences.light_theme = light;
                        cx.notify();
                    })
                })
            });
            for width in [880.0, 1220.0] {
                cx.simulate_resize(size(px(width), px(560.0)));
                view.update(cx, |view, cx| {
                    view.updater.update(cx, |updater, cx| {
                        updater.state = super::super::updates::State::Latest;
                        cx.notify();
                    })
                });
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let quick = cx.debug_bounds("titlebar-quick-actions").unwrap();
                let theme = cx.debug_bounds("titlebar-theme").unwrap();
                for state in [
                    super::super::updates::State::Available(candidate.clone()),
                    super::super::updates::State::Installing,
                    super::super::updates::State::Latest,
                ] {
                    view.update(cx, |view, cx| {
                        view.updater.update(cx, |updater, cx| {
                            updater.state = state;
                            cx.notify();
                        })
                    });
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    assert_eq!(cx.debug_bounds("titlebar-quick-actions").unwrap(), quick);
                    assert_eq!(cx.debug_bounds("titlebar-theme").unwrap(), theme);
                    if let Some(update) = cx.debug_bounds("workspace-update") {
                        assert!(update.right() + px(8.0) <= quick.left());
                    }
                }
            }
        }
    }
}

impl Tervia {
    pub(super) fn titlebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let update = self.workspace_update(cx);
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
                    .ml(px(8.0))
                    .min_w_0()
                    .child(if self.state.has_project() {
                        self.working_directory_switcher(cx).into_any_element()
                    } else {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(icon(IconName::SquareTerminal, p.secondary))
                            .child(self.t("local_terminal"))
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .on_mouse_down(MouseButton::Left, titlebar_mouse_down),
            )
            // Keep transient actions before the fixed controls so their click targets never move.
            .when(!self.state.has_project(), |bar| {
                bar.child(
                    label_button("local-open-project", self.t("welcome_open_project"), p)
                        .icon(icon(IconName::FolderOpen, p.secondary))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.choose_repository(window, cx)),
                        ),
                )
            })
            .when_some(update, |titlebar, update| {
                titlebar.child(div().mr(px(8.0)).child(update))
            })
            .child(
                label_button("open-palette", self.t("quick_actions"), p)
                    .debug_selector(|| "titlebar-quick-actions".into())
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
                    .debug_selector(|| "titlebar-theme".into())
                    .accessibility_label(self.t("toggle_theme"))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx))),
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
                    icon_button(
                        "toggle-terminal-dock",
                        IconName::SquareTerminal,
                        format!(
                            "{} · {}",
                            self.t("terminal_dock"),
                            if Platform::CURRENT == Platform::Mac {
                                "⌘J"
                            } else {
                                "Ctrl+Shift+J"
                            }
                        ),
                        p,
                        cx,
                    )
                    .debug_selector(|| "toggle-terminal-dock".into())
                    .accessibility_label(self.t("terminal_dock"))
                    .bg(if self.state.project().dock.visible {
                        p.selected
                    } else {
                        p.background.opacity(0.0)
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_terminal_dock(window, cx)),
                    ),
                )
            })
            .child(self.branch_switcher(super::git::SwitchAnchor::Navigation, cx))
    }

    pub(super) fn statusbar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        if !self.state.has_project() {
            let path = self
                .consoles()
                .get(self.state.command_session())
                .and_then(Option::as_ref)
                .map(|console| console.read(cx).cwd.clone());
            let path = path
                .filter(|path| dirs::home_dir().as_ref() != Some(path))
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "~".into());
            return div()
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
                .child(icon(IconName::SquareTerminal, p.secondary).size(px(12.0)))
                .child(self.t("local_terminal"))
                .child(path)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(p.accent)
                        .child(self.appearance.language.text(&self.state.notice).to_owned()),
                )
                .child(tervia::pty::default_shell().to_string_lossy().into_owned());
        }
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
            Tab::Files => project.current_file().map_or_else(String::new, |file| {
                if file.is_image() {
                    format!(
                        "{}  ·  {}",
                        file.disk_path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .unwrap_or("image")
                            .to_uppercase(),
                        self.t("preview")
                    )
                } else {
                    format!("{}  ·  UTF-8  ·  LF", file.language.to_uppercase())
                }
            }),
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
            if !self.state.has_project() && tab != Tab::Terminal {
                continue;
            }
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
                        if this.state.has_project() {
                            this.change_tab(tab, window, cx);
                        } else {
                            this.open_local_terminal(window, cx);
                        }
                    })),
            );
        }
        commands = commands
            .child(
                Button::new("palette-local-terminal")
                    .ghost()
                    .w_full()
                    .h(px(38.0))
                    .compact()
                    .label(self.t("new_local_terminal"))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.new_local_terminal(window, cx)),
                    ),
            )
            .child(
                Button::new("palette-welcome")
                    .ghost()
                    .w_full()
                    .h(px(38.0))
                    .compact()
                    .label(self.t("welcome_home"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_welcome(window, cx))),
            )
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
