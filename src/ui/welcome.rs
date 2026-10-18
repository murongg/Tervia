use super::{
    NewLocalTerminal, NewSession, OpenPreferences, OpenProject, Tervia, TogglePalette, ToggleTheme,
    controls::{icon, icon_button, label_button, titlebar_mouse_down, window_controls},
    theme,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::actions::Cancel;
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants},
    input::{Escape, Input},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use tervia::platform::Platform;

impl Tervia {
    fn welcome_action(&self, local: bool, cx: &mut Context<Self>) -> Button {
        let p = theme::Palette::new(self.appearance.light_theme);
        let foreground = if local { p.text } else { p.on_accent };
        let glyph = if local {
            IconName::SquareTerminal
        } else {
            IconName::FolderOpen
        };
        let label = self.t(if local {
            "welcome_local"
        } else {
            "welcome_open_project"
        });
        let hint = self.t(if local {
            "welcome_local_hint"
        } else {
            "welcome_project_hint"
        });
        let shortcut = if local {
            Platform::CURRENT.hint("⌘T")
        } else if Platform::CURRENT == Platform::Mac {
            "⌘O".into()
        } else {
            "Ctrl+Shift+O".into()
        };
        Button::new(if local {
            "welcome-local-terminal"
        } else {
            "welcome-open"
        })
        .ghost()
        .w_full()
        .h(px(76.0))
        .px(px(17.0))
        .rounded(px(9.0))
        .bg(if local {
            p.text.opacity(0.035)
        } else {
            p.accent
        })
        .text_color(foreground)
        .border_1()
        .border_color(if local { p.chrome_border() } else { p.accent })
        .accessibility_label(label)
        .debug_selector(move || {
            if local {
                "welcome-local-terminal"
            } else {
                "welcome-open"
            }
            .into()
        })
        .disabled(!local && self.git_mutating)
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(13.0))
                .child(
                    div()
                        .size(px(34.0))
                        .flex_shrink_0()
                        .rounded(px(7.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if local {
                            p.accent.opacity(0.08)
                        } else {
                            foreground.opacity(0.09)
                        })
                        .child(
                            icon(glyph, if local { p.accent } else { foreground }).size(px(18.0)),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_size(px(14.0))
                                .child(label),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(if local { p.secondary } else { foreground })
                                .child(hint),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .text_color(foreground)
                        .child(shortcut),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            if local {
                this.open_local_terminal(window, cx);
            } else {
                this.choose_repository(window, cx);
            }
        }))
    }

    pub(super) fn welcome(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = theme::Palette::workbench(self.appearance.light_theme);
        let update = self.workspace_update(cx);
        let recent = self
            .state
            .search_projects("", false)
            .into_iter()
            .filter(|found| !found.directory)
            .take(4)
            .collect::<Vec<_>>();
        let mut content = div().w(px(738.0)).flex().flex_col().gap(px(36.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(58.0))
                .child(
                    div()
                        .w(px(282.0))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .mb(px(28.0))
                                .child(
                                    img(Arc::new(Image::from_bytes(
                                        ImageFormat::Svg,
                                        include_bytes!("../../assets/tervia.svg").to_vec(),
                                    )))
                                    .size(px(48.0))
                                    .flex_shrink_0(),
                                )
                                .child(
                                    div()
                                        .text_size(px(18.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("Tervia"),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(28.0))
                                .font_weight(FontWeight::MEDIUM)
                                .mb(px(12.0))
                                .child(self.t("welcome_title")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .text_size(px(13.0))
                                .text_color(p.secondary)
                                .children(
                                    self.t("welcome_hint")
                                        .split('\n')
                                        .map(|line| div().child(line)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(11.0))
                        .child(self.welcome_action(false, cx))
                        .child(self.welcome_action(true, cx))
                        .child(
                            label_button("welcome-clone-open", self.t("git_clone_title"), p)
                                .icon(icon(IconName::Download, p.secondary).size(px(14.0)))
                                .text_size(px(12.0))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.welcome_cloning = true;
                                    this.welcome_url.focus_handle(cx).focus(window, cx);
                                    cx.notify();
                                })),
                        ),
                ),
        );
        if !recent.is_empty() {
            let mut rows = div().flex().flex_col().gap(px(4.0));
            for pair in recent.chunks(2) {
                let mut row = div().flex().gap(px(22.0));
                for found in pair {
                    let owner = found.owner;
                    let target = found.index;
                    let name = self.state.project_label(owner);
                    let path = self.state.projects[owner].path.clone();
                    row = row.child(
                        Button::new(("welcome-recent", owner))
                            .ghost()
                            .flex_1()
                            .min_w_0()
                            .h(px(56.0))
                            .px(px(8.0))
                            .accessibility_label(self.f("switch_project", &[("name", &name)]))
                            .tooltip(path.clone())
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(icon(IconName::Folder, p.secondary))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(div().text_ellipsis().child(name))
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(p.secondary)
                                                    .text_ellipsis()
                                                    .child(path),
                                            ),
                                    )
                                    .child(icon(IconName::ArrowUpRight, p.muted).size(px(13.0))),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.state.project_closed(owner) {
                                    this.reopen_project_target(owner, Some(target), window, cx);
                                } else {
                                    this.change_project(target, window, cx);
                                }
                            })),
                    );
                }
                if pair.len() == 1 {
                    row = row.child(div().flex_1());
                }
                rows = rows.child(row);
            }
            content = content.child(
                div()
                    .border_t_1()
                    .border_color(p.chrome_border())
                    .pt(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.secondary)
                                    .child(self.t("welcome_recent")),
                            )
                            .child(div().flex_1())
                            .when(!self.state.closed_project_rows().is_empty(), |row| {
                                row.child(
                                    label_button(
                                        "welcome-recent-projects",
                                        self.t("project_recent_closed"),
                                        p,
                                    )
                                    .debug_selector(|| "welcome-recent-projects".into())
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.show_project_switcher(true, window, cx)
                                        },
                                    )),
                                )
                            }),
                    )
                    .child(rows),
            );
        }
        div()
            .id("tervia-welcome")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context("Tervia")
            .bg(theme::workspace_background(self.appearance.light_theme))
            .text_color(p.text)
            .font_family(Platform::CURRENT.ui_font())
            .text_size(px(self.appearance.ui_size as f32))
            .on_action(cx.listener(|this, _: &OpenPreferences, _, cx| this.open_preferences(cx)))
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.choose_repository(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &NewSession, window, cx| {
                    this.open_local_terminal(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &NewLocalTerminal, window, cx| {
                this.new_local_terminal(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleTheme, window, cx| this.toggle_theme(window, cx)),
            )
            .on_action(cx.listener(|this, _: &TogglePalette, _, cx| {
                this.state.palette_open = !this.state.palette_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(
                |this, _: &super::projects::QuickSwitchProject, window, cx| {
                    this.show_project_switcher(false, window, cx)
                },
            ))
            .on_action(
                cx.listener(|this, _: &super::projects::ReopenProject, window, cx| {
                    this.show_project_switcher(true, window, cx)
                }),
            )
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px(px(16.0))
                    .when(Platform::CURRENT == Platform::Windows, |bar| bar.pr_0())
                    .text_size(px(12.0))
                    .child(div().w(px(72.0)))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("Tervia")
                            .on_mouse_down(MouseButton::Left, titlebar_mouse_down),
                    )
                    .when_some(update, |bar, update| bar.child(update))
                    .child(
                        icon_button(
                            "welcome-quick-actions",
                            IconName::Command,
                            self.t("quick_actions"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("quick_actions"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.palette_open = !this.state.palette_open;
                            cx.notify();
                        })),
                    )
                    .child(
                        icon_button(
                            "welcome-settings",
                            IconName::Settings,
                            self.t("settings"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("settings"))
                        .on_click(cx.listener(|this, _, _, cx| this.open_preferences(cx))),
                    )
                    .child(window_controls(window, p, self.appearance.language)),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(46.0))
                    .py(px(36.0))
                    .child(content),
            )
            .when(!self.state.notice.is_empty(), |welcome| {
                welcome.child(
                    div()
                        .px(px(28.0))
                        .pb(px(16.0))
                        .text_size(px(12.0))
                        .text_color(p.secondary)
                        .child(self.appearance.language.text(&self.state.notice).to_owned()),
                )
            })
            .when(self.navigation.switcher.open, |welcome| {
                welcome.child(self.project_switcher(cx))
            })
            .when(self.state.palette_open, |welcome| {
                welcome.child(self.palette(cx))
            })
            .when(self.welcome_cloning, |welcome| {
                welcome.child(self.welcome_clone(cx))
            })
    }

    fn welcome_clone(&self, cx: &mut Context<Self>) -> Div {
        let p = theme::Palette::new(self.appearance.light_theme);
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x10131b88))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.welcome_cloning = false;
                    this.focus.focus(window, cx);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(420.0))
                    .rounded(px(12.0))
                    .bg(p.sidebar)
                    .p(px(25.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(17.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.t("git_clone_title")),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.secondary)
                            .child(self.t("git_clone_url")),
                    )
                    .child(Input::new(&self.welcome_url).aria_label(self.t("git_clone_url")))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.secondary)
                            .child(self.t("git_clone_hint")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(7.0))
                            .mt(px(6.0))
                            .child(
                                label_button("welcome-clone-cancel", self.t("git_cancel"), p)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.welcome_cloning = false;
                                        this.focus.focus(window, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("welcome-clone")
                                    .primary()
                                    .bg(p.accent)
                                    .text_color(p.on_accent)
                                    .label(self.t("git_clone"))
                                    .disabled(
                                        self.git_mutating
                                            || self.welcome_url.read(cx).value().trim().is_empty(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let url = this.welcome_url.read(cx).value().to_string();
                                        this.welcome_cloning = false;
                                        this.clone_url(None, url, window, cx);
                                    })),
                            ),
                    ),
            )
    }
}
