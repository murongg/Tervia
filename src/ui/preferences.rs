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
use tervia::preferences::{Mode, Preferences, Theme as ColorTheme};
use tervia::shell::Profile;

use super::{
    controls::{
        has_window_controls, icon, icon_button, label_button, list_row, titlebar_drag_region,
        window_controls,
    },
    theme::Palette,
};

mod agents;
mod editor;
mod sections;
mod startup;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;
mod themes;

pub(super) fn allow_quit(cx: &mut App) -> bool {
    editor::guard_quit(cx)
}
pub(super) fn has_pending_theme_edits(cx: &App) -> bool {
    editor::has_pending_edits(cx)
}

const NAVIGATION_WIDTH: f32 = 176.0;

pub(super) fn system_is_light(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Light | WindowAppearance::VibrantLight
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    General,
    Appearance,
    Workspace,
    Updates,
}

impl Category {
    fn key(self) -> &'static str {
        match self {
            Self::General => "settings_general",
            Self::Appearance => "appearance",
            Self::Workspace => "settings_workspace",
            Self::Updates => "updates",
        }
    }
}

pub(super) fn apply(preferences: &Preferences, cx: &mut App) {
    gpui_kit::component::set_locale(preferences.language.code());
    cx.set_menus(super::menus(preferences.language));
    Theme::change(
        if preferences.theme.is_light() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    super::theme::apply_palette(&preferences.theme, cx);
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
    category: Category,
    font_select: Entity<SelectState<Vec<SharedString>>>,
    language_select: Entity<SelectState<Vec<LanguageChoice>>>,
    shell_select: Entity<SelectState<Vec<ShellChoice>>>,
    startup_action: startup::Action,
    startup_enabled: Option<bool>,
    startup_busy: bool,
    startup_error: Option<String>,
    agent_action: agents::Action,
    agent_busy: Option<tervia::agent::Agent>,
    agent_result: Option<Result<(tervia::agent::Agent, bool, PathBuf), String>>,
    custom_themes: Vec<tervia::themes::Document>,
    theme_loading: bool,
    theme_deleting: bool,
    theme_error: Option<String>,
    theme_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
struct LanguageChoice {
    language: Language,
    label: SharedString,
}

#[derive(Clone)]
struct ShellChoice {
    profile: Profile,
    label: SharedString,
}

impl SearchableListItem for ShellChoice {
    type Value = Profile;
    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Profile {
        &self.profile
    }
}

fn shell_choices(language: Language, current: Profile) -> Vec<ShellChoice> {
    let mut profiles = Profile::available();
    if !profiles.contains(&current) {
        profiles.push(current);
    }
    profiles
        .into_iter()
        .map(|profile| ShellChoice {
            label: profile.label(language).to_owned().into(),
            profile,
        })
        .collect()
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
        let shell_select = cx.new(|cx| {
            let choices = shell_choices(current.language, current.shell.clone());
            let selected = choices
                .iter()
                .position(|choice| choice.profile == current.shell)
                .unwrap_or(0);
            SelectState::new(choices, Some(IndexPath::new(selected)), window, cx)
        });
        let shell_subscription = cx.subscribe_in(&shell_select, window, {
            let preferences = preferences.clone();
            move |_, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(profile)) = event {
                    preferences.update(cx, |prefs, cx| {
                        if prefs.shell != *profile {
                            // Startup flags belong to one shell and can be invalid for the next one.
                            prefs.shell_args = None;
                            prefs.shell = profile.clone();
                        }
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
            this.shell_select.update(cx, |select, cx| {
                select.set_items(
                    shell_choices(language, this.current.shell.clone()),
                    window,
                    cx,
                );
                select.set_selected_value(&this.current.shell, window, cx);
            });
            cx.notify();
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let config_observer = cx.observe(&config_error, |_, _, cx| cx.notify());
        let entity = cx.entity();
        cx.defer(move |cx| {
            entity.update(cx, |this, cx| {
                this.configure_startup(None, cx);
                this.reload_themes(cx);
            })
        });
        Self {
            focus,
            scroll: ScrollHandle::new(),
            preferences,
            current,
            config_path,
            config_error,
            updater,
            category: Category::General,
            font_select,
            language_select,
            shell_select,
            startup_action: std::sync::Arc::new(tervia::autostart::configure),
            startup_enabled: None,
            startup_busy: false,
            startup_error: None,
            agent_action: std::sync::Arc::new(|agent, enabled| {
                let path = tervia::agent::hooks::path(agent).map_err(|error| error.to_string())?;
                let executable = std::env::current_exe().map_err(|error| error.to_string())?;
                tervia::agent::hooks::configure(&path, &executable, agent, enabled)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                Ok(path)
            }),
            agent_busy: None,
            agent_result: None,
            custom_themes: Vec::new(),
            theme_loading: false,
            theme_deleting: false,
            theme_error: None,
            theme_task: None,
            _subscriptions: vec![
                subscription,
                language_subscription,
                shell_subscription,
                observer,
                config_observer,
            ],
        }
    }

    fn size_control(&self, code: bool, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(&self.current.theme);
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
            .text_color(Palette::new(&self.current.theme).secondary)
            .child(self.t(key))
    }

    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(&self.current.theme);
        div()
            .debug_selector(|| "settings-navigation".to_owned())
            .w(px(NAVIGATION_WIDTH))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .px(px(12.0))
            .py(px(20.0))
            .children(
                [
                    (
                        Category::General,
                        "settings-general-nav",
                        IconName::Settings,
                    ),
                    (
                        Category::Appearance,
                        "settings-appearance-nav",
                        IconName::Palette,
                    ),
                    (
                        Category::Workspace,
                        "settings-workspace-nav",
                        IconName::PanelLeft,
                    ),
                    (
                        Category::Updates,
                        "settings-updates-nav",
                        IconName::Download,
                    ),
                ]
                .into_iter()
                .map(|(category, id, glyph)| {
                    let selected = self.category == category;
                    div().debug_selector(move || id.to_owned()).child(
                        list_row(id, selected, p, cx)
                            .h(px(36.0))
                            .accessibility_label(self.t(category.key()))
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(icon(glyph, p.text).flex_shrink_0())
                                    .child(
                                        div().min_w_0().truncate().child(self.t(category.key())),
                                    ),
                            )
                            .toggled(selected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.category = category;
                                this.scroll.set_offset(point(px(0.0), px(0.0)));
                                cx.notify();
                            })),
                    )
                }),
            )
            .child(div().flex_1())
            .when_some(self.config_path.clone(), |sidebar, path| {
                sidebar.child(
                    label_button("reveal-config", "config.toml", p, cx)
                        .w_full()
                        .justify_start()
                        .accessibility_label(self.t("show_config"))
                        .icon(icon(IconName::FolderOpen, p.secondary))
                        .tooltip(format!("{}\n{}", self.t("show_config"), path.display()))
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
            })
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(&self.current.theme);
        let error = self.config_error.read(cx).clone();
        let show_footer = self.category != Category::Updates || error.is_some();
        let content = match self.category {
            Category::General => self.general_settings(cx),
            Category::Appearance => self.appearance_settings(cx),
            Category::Workspace => self.workspace_settings(cx),
            Category::Updates => div().child(self.updater.clone()),
        };
        let body = div()
            .id("preferences-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(24.0))
            .py(px(20.0))
            .child(content);
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
                            label_button("config-error", self.t("config_details"), p, cx)
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
            .when(self.category != Category::Updates, |footer| {
                footer.child(
                    label_button("reset-preferences", self.t("reset_defaults"), p, cx)
                        .debug_selector(|| "settings-reset-defaults".into())
                        .flex_shrink_0()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.preferences.update(cx, |prefs, cx| {
                                match this.category {
                                    Category::General => prefs.reset_general(),
                                    Category::Appearance => prefs.reset_appearance(),
                                    Category::Workspace => prefs.reset_workspace(),
                                    Category::Updates => {}
                                }
                                cx.notify();
                            });
                            if this.category == Category::Appearance {
                                this.font_select.update(cx, |select, cx| {
                                    select.set_selected_value(
                                        &SharedString::from(Preferences::default().font_family),
                                        window,
                                        cx,
                                    )
                                });
                            }
                        })),
                )
            });
        div()
            .relative()
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
            // Carry the sidebar behind the traffic lights without a seam below the titlebar.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(NAVIGATION_WIDTH))
                    .bg(p.sidebar)
                    .border_r_1()
                    .border_color(p.border),
            )
            .child(
                div()
                    .h(px(48.0))
                    .pl(px(NAVIGATION_WIDTH + 24.0))
                    .pr(px(24.0))
                    .when(has_window_controls(window), |bar| bar.pr_0())
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(!has_window_controls(window), |bar| {
                        titlebar_drag_region(bar)
                    })
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .when(has_window_controls(window), |title| {
                                titlebar_drag_region(title)
                            })
                            .child(self.t(self.category.key())),
                    )
                    .child(window_controls(
                        window,
                        p,
                        self.current.language,
                        |_, window, _| window.remove_window(),
                    )),
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
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .child(body)
                                    .child(super::scroll::overlay(
                                        &self.scroll,
                                        gpui_kit::component::scroll::ScrollbarAxis::Vertical,
                                    )),
                            )
                            .when(show_footer, |pane| pane.child(footer)),
                    ),
            )
    }
}
