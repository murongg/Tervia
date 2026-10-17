use tervia::preferences::Preferences;

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
    prefs.light_theme = true;
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
        light_theme: true,
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
