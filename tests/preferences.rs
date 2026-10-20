use tervia::preferences::Preferences;
use tervia::preferences::Theme;
use tervia::preferences::Themes;

#[test]
fn general_reset_preserves_appearance_and_workspace_choices() {
    let initial = Preferences {
        theme: Theme::Monokai.into(),
        language: tervia::locale::Language::English,
        font_family: "Synthetic Mono".into(),
        code_size: 19,
        ui_size: 15,
        file_nesting: true,
        command_suggestions: false,
        agent_notifications: true,
        ..Preferences::default()
    };
    let mut preferences = initial.clone();
    preferences.reset_general();
    assert_eq!(
        preferences,
        Preferences {
            language: Preferences::default().language,
            ..initial
        }
    );
}

#[test]
fn workspace_reset_preserves_typography_and_appearance() {
    let initial = Preferences {
        theme: Theme::Paper.into(),
        font_family: "Synthetic Mono".into(),
        code_size: 19,
        ui_size: 15,
        file_nesting: true,
        shell: tervia::shell::Profile::Cmd,
        ..Preferences::default()
    };
    let mut preferences = initial.clone();
    preferences.reset_workspace();
    assert_eq!(
        preferences,
        Preferences {
            file_nesting: false,
            shell: tervia::shell::Profile::Default,
            ..initial
        }
    );
}

#[test]
fn system_mode_tracks_os_changes_and_manual_modes_remain_fixed() {
    use tervia::preferences::Mode;
    let mut themes = Themes::default();
    themes.select(Theme::Monokai);
    themes.select(Theme::QuietLight);
    themes.set_mode(Mode::System);
    assert!(themes.sync_system(true));
    assert_eq!(themes.current(), Theme::QuietLight);
    assert!(!themes.sync_system(true));
    assert!(themes.sync_system(false));
    assert_eq!(themes.current(), Theme::Monokai);
    assert_eq!(themes.mode(), Mode::System);
    themes.set_light(true);
    assert_eq!(themes.mode(), Mode::Light);
    assert!(!themes.sync_system(false));
    assert_eq!(themes.current(), Theme::QuietLight);
    themes.toggle();
    assert_eq!(themes.mode(), Mode::Dark);
    assert_eq!(themes.current(), Theme::Monokai);
}

#[test]
fn theme_presets_have_stable_identifiers() {
    for theme in Theme::ALL {
        assert_eq!(Theme::from_id(theme.id()), Some(theme));
    }
    assert_eq!(Theme::from_id("mock-unknown"), None);
    assert_eq!(Theme::from_id(""), None);
}

#[test]
fn switching_modes_restores_the_independent_theme_choices() {
    let mut themes = Themes::default();
    themes.select(Theme::Midnight);
    themes.select(Theme::Paper);
    assert_eq!(themes.current(), Theme::Midnight);
    themes.toggle();
    assert_eq!(themes.current(), Theme::Paper);
    themes.select(Theme::Forest);
    assert_eq!(themes.current(), Theme::Paper);
    themes.toggle();
    assert_eq!(themes.current(), Theme::Forest);
    themes.set_light(true);
    assert_eq!(themes.current(), Theme::Paper);
    assert_eq!(themes.selected(false), Theme::Forest);
    assert_eq!(themes.selected(true), Theme::Paper);
}

#[test]
fn each_legacy_preset_initializes_only_its_own_mode() {
    for theme in Theme::ALL {
        let themes = Themes::from(theme);
        assert_eq!(themes.current(), theme);
        assert_eq!(themes.is_light(), theme.is_light());
        assert_eq!(
            themes.selected(!theme.is_light()),
            if theme.is_light() {
                Theme::Dark
            } else {
                Theme::Light
            }
        );
    }
}

#[test]
fn resetting_appearance_restores_the_default_theme_for_every_preset() {
    for theme in Theme::ALL {
        let mut preferences = Preferences {
            theme: theme.into(),
            ..Preferences::default()
        };
        preferences.reset_appearance();
        assert_eq!(preferences.theme, Themes::default());
    }
}

#[test]
fn theme_modes_keep_icons_and_syntax_highlighting_on_the_correct_base() {
    for theme in Theme::ALL {
        assert_eq!(
            theme.is_light(),
            matches!(
                theme,
                Theme::Light
                    | Theme::Paper
                    | Theme::Rose
                    | Theme::SolarizedLight
                    | Theme::QuietLight
            )
        );
        for language in tervia::locale::Language::ALL {
            assert_ne!(theme.label(language), theme.key());
        }
    }
}

#[test]
fn resetting_interface_preferences_does_not_resume_history_recording() {
    let mut preferences = Preferences {
        command_suggestions: false,
        ..Preferences::default()
    };
    preferences.reset();
    assert!(!preferences.command_suggestions);
}

#[test]
fn font_sizes_stay_within_readable_bounds() {
    let mut prefs = Preferences::default();
    prefs.set_code_size(4);
    assert_eq!(prefs.code_size, 11);
    prefs.set_code_size(50);
    assert_eq!(prefs.code_size, 24);
    prefs.set_ui_size(40);
    assert_eq!(prefs.ui_size, 16);
}

#[test]
fn reset_restores_all_appearance_preferences() {
    let mut prefs = Preferences::default();
    prefs.set_code_size(19);
    prefs.set_ui_size(15);
    prefs.font_family = "Monaco".into();
    prefs.theme = tervia::preferences::Theme::Light.into();
    prefs.language = tervia::locale::Language::English;
    prefs.reset();
    assert_eq!(prefs, Preferences::default());
}

#[test]
fn resetting_appearance_preserves_the_automatic_update_policy() {
    let updates = tervia::updates::Settings {
        auto_check: false,
        interval_minutes: 37,
    };
    let mut prefs = Preferences {
        updates,
        theme: tervia::preferences::Theme::Light.into(),
        ui_size: 16,
        ..Preferences::default()
    };
    prefs.reset();
    assert_eq!(
        prefs,
        Preferences {
            updates,
            ..Preferences::default()
        }
    );
}

#[test]
fn appearance_reset_preserves_language_and_workspace_behavior() {
    let initial = Preferences {
        theme: Theme::Monokai.into(),
        language: tervia::locale::Language::German,
        font_family: "Synthetic Mono".into(),
        code_size: 19,
        ui_size: 15,
        file_nesting: true,
        command_suggestions: false,
        agent_notifications: true,
        shell: tervia::shell::Profile::Cmd,
        ..Preferences::default()
    };
    let mut preferences = initial.clone();
    preferences.reset_appearance();
    let defaults = Preferences::default();
    assert_eq!(
        preferences,
        Preferences {
            theme: defaults.theme,
            font_family: defaults.font_family,
            code_size: defaults.code_size,
            ui_size: defaults.ui_size,
            ..initial
        }
    );
}
