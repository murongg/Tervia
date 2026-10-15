use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::base::actions::Cancel;
use gpui_kit::component::input::Escape;
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::{
    IndexPath, Sizable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tervia::locale::Language;
use tervia::platform::Platform;
use tervia::preferences::Preferences;

use super::{
    controls::{icon, icon_button, label_button},
    theme::Palette,
};

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

pub(super) fn apply(preferences: &Preferences, cx: &mut App) {
    gpui_kit::component::set_locale(preferences.language.code());
    cx.set_menus(super::menus(preferences.language));
    Theme::change(
        if preferences.light_theme {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    super::theme::apply_palette(preferences.light_theme, cx);
    Theme::update(cx, |theme| {
        theme.font_size = px(preferences.ui_size as f32);
        theme.mono_font_size = px(preferences.code_size as f32);
        theme.mono_font_family = preferences.font_family.clone().into();
        theme.radius = px(6.0);
    });
}

pub struct PreferencesWindow {
    focus: FocusHandle,
    scroll: ScrollHandle,
    preferences: Entity<Preferences>,
    current: Preferences,
    config_path: Option<PathBuf>,
    config_error: Entity<Option<String>>,
    updater: Entity<super::updates::Updater>,
    updates_selected: bool,
    font_select: Entity<SelectState<Vec<SharedString>>>,
    language_select: Entity<SelectState<Vec<LanguageChoice>>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
struct LanguageChoice {
    language: Language,
    label: SharedString,
}

impl SearchableListItem for LanguageChoice {
    type Value = Language;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Language {
        &self.language
    }
}

fn language_choices(display: Language) -> Vec<LanguageChoice> {
    Language::ALL
        .into_iter()
        .map(|language| LanguageChoice {
            language,
            label: language.label(display).into(),
        })
        .collect()
}

impl PreferencesWindow {
    fn t(&self, key: &'static str) -> &'static str {
        self.current.language.text(key)
    }
    pub fn new(
        preferences: Entity<Preferences>,
        config_path: Option<PathBuf>,
        config_error: Entity<Option<String>>,
        updater: Entity<super::updates::Updater>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let current = preferences.read(cx).clone();
        let installed = cx.text_system().all_font_names();
        let fonts: Vec<SharedString> = [
            "Menlo",
            "Monaco",
            "SF Mono",
            "JetBrains Mono",
            "Courier New",
            "Courier",
            "Consolas",
            "Cascadia Mono",
            "Cascadia Code",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Noto Sans Mono",
        ]
        .into_iter()
        .filter(|font| installed.iter().any(|name| name == font))
        .map(Into::into)
        .collect();
        let mut fonts = fonts;
        if !fonts
            .iter()
            .any(|font| font.as_ref() == current.font_family)
        {
            fonts.insert(0, current.font_family.clone().into());
        }
        let selected = fonts
            .iter()
            .position(|font| font.as_ref() == current.font_family)
            .unwrap_or(0);
        let font_select =
            cx.new(|cx| SelectState::new(fonts, Some(IndexPath::new(selected)), window, cx));
        let subscription = cx.subscribe_in(&font_select, window, {
            let preferences = preferences.clone();
            move |_, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(font)) = event {
                    preferences.update(cx, |prefs, cx| {
                        prefs.font_family = font.to_string();
                        cx.notify();
                    });
                }
            }
        });
        let selected_language = Language::ALL
            .iter()
            .position(|language| *language == current.language)
            .unwrap_or(0);
        let language_select = cx.new(|cx| {
            SelectState::new(
                language_choices(current.language),
                Some(IndexPath::new(selected_language)),
                window,
                cx,
            )
        });
        let language_subscription = cx.subscribe_in(&language_select, window, {
            let preferences = preferences.clone();
            move |_, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(language)) = event {
                    preferences.update(cx, |prefs, cx| {
                        prefs.language = *language;
                        cx.notify();
                    });
                }
            }
        });
        let observer = cx.observe_in(&preferences, window, |this, preferences, window, cx| {
            this.current = preferences.read(cx).clone();
            window.set_window_title(&format!("Tervia {}", this.t("settings")));
            let language = this.current.language;
            this.language_select.update(cx, |select, cx| {
                select.set_items(language_choices(language), window, cx);
                select.set_selected_value(&language, window, cx);
            });
            cx.notify();
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let config_observer = cx.observe(&config_error, |_, _, cx| cx.notify());
        Self {
            focus,
            scroll: ScrollHandle::new(),
            preferences,
            current,
            config_path,
            config_error,
            updater,
            updates_selected: false,
            font_select,
            language_select,
            _subscriptions: vec![
                subscription,
                language_subscription,
                observer,
                config_observer,
            ],
        }
    }

    fn size_control(&self, code: bool, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        let value = if code {
            self.current.code_size
        } else {
            self.current.ui_size
        };
        let key = if code { "code" } else { "ui" };
        div()
            .flex()
            .items_center()
            .h(px(32.0))
            .px(px(2.0))
            .border_1()
            .border_color(p.border)
            .rounded(px(6.0))
            .child(
                icon_button(
                    format!("{key}-minus"),
                    IconName::Minus,
                    self.t("decrease_size"),
                    p,
                    cx,
                )
                .accessibility_label(format!(
                    "{} · {}",
                    if code {
                        self.t("decrease_code_size")
                    } else {
                        self.t("decrease_ui_size")
                    },
                    self.current
                        .language
                        .format("current_size", &[("value", &value.to_string())])
                ))
                .on_click(cx.listener(move |this, _, _, cx| this.change_size(code, -1, cx))),
            )
            .child(
                div()
                    .w(px(64.0))
                    .text_center()
                    .text_size(px(13.0))
                    .child(format!("{value} pt")),
            )
            .child(
                icon_button(
                    format!("{key}-plus"),
                    IconName::Plus,
                    self.t("increase_size"),
                    p,
                    cx,
                )
                .accessibility_label(format!(
                    "{} · {}",
                    if code {
                        self.t("increase_code_size")
                    } else {
                        self.t("increase_ui_size")
                    },
                    self.current
                        .language
                        .format("current_size", &[("value", &value.to_string())])
                ))
                .on_click(cx.listener(move |this, _, _, cx| this.change_size(code, 1, cx))),
            )
    }

    fn row(&self, label: &'static str, control_id: &'static str, control: impl IntoElement) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .min_h(px(44.0))
            .gap(px(20.0))
            .child(
                div()
                    .debug_selector(|| label.to_owned())
                    .flex_1()
                    .min_w_0()
                    .child(self.t(label)),
            )
            // Select styles its inner input while its root fills its parent. The parent owns the column width.
            .child(
                div()
                    .debug_selector(|| control_id.to_owned())
                    .w(px(224.0))
                    .h(px(32.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(control),
            )
    }

    fn heading(&self, key: &'static str) -> Div {
        div()
            .mb(px(8.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(Palette::new(self.current.light_theme).secondary)
            .child(self.t(key))
    }

    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .debug_selector(|| "settings-navigation".to_owned())
            .w(px(176.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .px(px(12.0))
            .py(px(20.0))
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .debug_selector(|| "settings-interface-nav".to_owned())
                    .child(
                        Button::new("preferences-interface")
                            .ghost()
                            .w_full()
                            .h(px(36.0))
                            .accessibility_label(self.t("settings_interface"))
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(icon(IconName::Settings, p.text))
                                    .child(self.t("settings_interface")),
                            )
                            .toggled(!self.updates_selected)
                            .when(!self.updates_selected, |button| button.bg(p.selected))
                            .text_color(p.text)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.updates_selected = false;
                                this.scroll.set_offset(point(px(0.0), px(0.0)));
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "settings-updates-nav".to_owned())
                    .child(
                        Button::new("preferences-updates")
                            .ghost()
                            .w_full()
                            .h(px(36.0))
                            .accessibility_label(self.t("updates"))
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(icon(IconName::Download, p.text))
                                    .child(self.t("updates")),
                            )
                            .toggled(self.updates_selected)
                            .when(self.updates_selected, |button| button.bg(p.selected))
                            .text_color(p.text)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.updates_selected = true;
                                this.scroll.set_offset(point(px(0.0), px(0.0)));
                                cx.notify();
                            })),
                    ),
            )
            .child(div().flex_1())
            .when_some(self.config_path.clone(), |sidebar, path| {
                sidebar.child(
                    Button::new("reveal-config")
                        .ghost()
                        .small()
                        .h(px(28.0))
                        .text_color(p.secondary)
                        .w_full()
                        .accessibility_label(self.t("show_config"))
                        .child(
                            div()
                                .flex()
                                .w_full()
                                .items_center()
                                .gap(px(8.0))
                                .child(icon(IconName::FolderOpen, p.secondary))
                                .child("config.toml"),
                        )
                        .tooltip(format!("{}\n{}", self.t("show_config"), path.display()))
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
            })
    }

    fn theme_control(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .flex()
            .w_full()
            .h(px(32.0))
            .p(px(2.0))
            .gap(px(2.0))
            .bg(p.raised)
            .rounded(px(7.0))
            .children([false, true].into_iter().map(|light| {
                let selected = light == self.current.light_theme;
                Button::new(if light {
                    "preferences-light"
                } else {
                    "preferences-dark"
                })
                .ghost()
                .small()
                .flex_1()
                .h(px(28.0))
                .label(self.t(if light { "light" } else { "dark" }))
                .icon(icon(
                    if light { IconName::Sun } else { IconName::Moon },
                    if selected { p.text } else { p.secondary },
                ))
                .toggled(selected)
                .bg(if selected { p.background } else { p.raised })
                .border_1()
                .border_color(if selected { p.border } else { p.raised })
                .rounded(px(5.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.preferences.update(cx, |prefs, cx| {
                        prefs.light_theme = light;
                        cx.notify();
                    });
                }))
            }))
    }

    fn change_size(&self, code: bool, delta: i8, cx: &mut Context<Self>) {
        self.preferences.update(cx, |prefs, cx| {
            if code {
                prefs.set_code_size(prefs.code_size.saturating_add_signed(delta));
            } else {
                prefs.set_ui_size(prefs.ui_size.saturating_add_signed(delta));
            }
            cx.notify();
        });
    }
}

impl Render for PreferencesWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(self.current.light_theme);
        let error = self.config_error.read(cx).clone();
        let show_footer = !self.updates_selected || error.is_some();
        let body = div()
            .id("preferences-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(24.0))
            .py(px(20.0))
            .when(self.updates_selected, |body| {
                body.child(self.updater.clone())
            })
            .when(!self.updates_selected, |body| {
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(24.0))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .mb(px(20.0))
                                        .text_size(px(20.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(self.t("settings_interface")),
                                )
                                .child(self.row("theme", "theme-control", self.theme_control(cx)))
                                .child(
                                    self.row(
                                        "language",
                                        "language-control",
                                        Select::new(&self.language_select)
                                            .w_full()
                                            .accessibility_label(self.t("language")),
                                    ),
                                ),
                        )
                        .child(div().h(px(1.0)).bg(p.border))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(self.heading("settings_fonts"))
                                .child(
                                    self.row(
                                        "code_font",
                                        "font-control",
                                        Select::new(&self.font_select)
                                            .w_full()
                                            .accessibility_label(self.t("code_font")),
                                    ),
                                )
                                .child(self.row(
                                    "code_size",
                                    "code-size-control",
                                    self.size_control(true, cx),
                                ))
                                .child(self.row(
                                    "ui_size",
                                    "ui-size-control",
                                    self.size_control(false, cx),
                                ))
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .px(px(12.0))
                                        .py(px(10.0))
                                        .rounded(px(6.0))
                                        .bg(p.raised.opacity(0.5))
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(p.secondary)
                                                .child(self.t("preview")),
                                        )
                                        .child(
                                            div()
                                                .overflow_x_hidden()
                                                .font_family(self.current.font_family.clone())
                                                .text_size(px(self.current.code_size as f32))
                                                .line_height(relative(1.4))
                                                .child("Aa Bb 012345  {}"),
                                        ),
                                ),
                        )
                        .child(div().h(px(1.0)).bg(p.border))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(self.heading("settings_files"))
                                .child(
                                    self.row(
                                        "file_nesting",
                                        "file-nesting-control",
                                        div()
                                            .debug_selector(|| "file-nesting-switch".into())
                                            .flex()
                                            .items_center()
                                            .h_full()
                                            .child(
                                                Switch::new("file-nesting")
                                                    .checked(self.current.file_nesting)
                                                    .accessibility_label(self.t("file_nesting"))
                                                    .on_change(cx.listener(
                                                        |this, enabled, _, cx| {
                                                            this.preferences.update(
                                                                cx,
                                                                |prefs, cx| {
                                                                    prefs.file_nesting = *enabled;
                                                                    cx.notify();
                                                                },
                                                            );
                                                        },
                                                    )),
                                            ),
                                    ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(p.secondary)
                                        .child(self.t("file_nesting_hint")),
                                ),
                        ),
                )
            });
        let footer = div()
            .debug_selector(|| "settings-footer".to_owned())
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(16.0))
            .px(px(24.0))
            .py(px(16.0))
            .border_t_1()
            .border_color(p.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(if error.is_some() { p.red } else { p.secondary })
                            .child(self.t(if error.is_some() {
                                "config_error"
                            } else {
                                "preferences_hint"
                            })),
                    )
                    .when_some(error, |status, error| {
                        status.child(
                            label_button("config-error", self.t("config_details"), p)
                                .text_size(px(11.0))
                                .text_color(p.red)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    drop(window.prompt(
                                        PromptLevel::Warning,
                                        this.t("config_error"),
                                        Some(&error),
                                        &[this.t("close")],
                                        cx,
                                    ));
                                })),
                        )
                    }),
            )
            .when(!self.updates_selected, |footer| {
                footer.child(
                    label_button("reset-preferences", self.t("reset_defaults"), p)
                        .debug_selector(|| "settings-reset-defaults".into())
                        .flex_shrink_0()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.preferences.update(cx, |prefs, cx| {
                                prefs.reset();
                                cx.notify();
                            });
                            this.font_select.update(cx, |select, cx| {
                                select.set_selected_value(
                                    &SharedString::from(Preferences::default().font_family),
                                    window,
                                    cx,
                                )
                            });
                        })),
                )
            });
        div()
            .size_full()
            .key_context("TerviaSettings")
            .track_focus(&self.focus)
            .on_action(|_: &Cancel, window, _| window.remove_window())
            .on_action(|_: &Escape, window, _| window.remove_window())
            .flex()
            .flex_col()
            .bg(p.background)
            .text_color(p.text)
            .font_family(Platform::CURRENT.ui_font())
            .text_size(px(13.0))
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.secondary)
                    .font_weight(FontWeight::MEDIUM)
                    .on_mouse_down(MouseButton::Left, super::controls::titlebar_mouse_down)
                    .child(self.t("settings")),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.navigation(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(body)
                            .when(show_footer, |pane| pane.child(footer)),
                    ),
            )
    }
}
