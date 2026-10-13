use tervia::preferences::Preferences;

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
