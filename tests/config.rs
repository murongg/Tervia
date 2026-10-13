use std::fs;
use tempfile::TempDir;
use tervia::{config::Config, locale::Language, preferences::Preferences};

fn fixture(source: &str) -> (TempDir, Config) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(&path, source).unwrap();
    (root, Config::new(path))
}

#[test]
fn first_launch_creates_a_commented_config_and_round_trips_defaults() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tervia/config.toml");
    let mut config = Config::new(path.clone());
    assert_eq!(config.path(), path);
    assert_eq!(config.load().unwrap(), Preferences::default());
    let source = fs::read_to_string(&path).unwrap();
    assert!(source.contains('#'));
    assert!(source.contains("language = \"system\""));
    assert_eq!(Config::new(path).load().unwrap(), Preferences::default());
}

#[test]
fn partial_config_uses_defaults_and_loads_all_supported_preferences() {
    let (_root, mut config) = fixture("[ui]\ntheme = 'light'\nlanguage = 'zh-CN'\n");
    assert_eq!(
        config.load().unwrap(),
        Preferences {
            light_theme: true,
            language: Language::Chinese,
            ..Preferences::default()
        }
    );
    let (_root, mut config) =
        fixture("[ui.fonts]\ncode_family = 'Synthetic Mono'\ncode_size = 24\nui_size = 11\n");
    assert_eq!(
        config.load().unwrap(),
        Preferences {
            font_family: "Synthetic Mono".into(),
            code_size: 24,
            ui_size: 11,
            ..Preferences::default()
        }
    );
}

#[test]
fn saves_preserve_comments_unknown_settings_and_unchanged_formatting() {
    let (_root, mut config) = fixture(
        "# My synthetic setup\n[ui]\ntheme = 'dark' # keep this note\nlanguage = 'system'\n\n[future]\nvalue = 42 # another note\n",
    );
    let mut preferences = config.load().unwrap();
    preferences.light_theme = true;
    preferences.font_family = "Synthetic \\\" Mono".into();
    config.save(&preferences).unwrap();
    let source = fs::read_to_string(config.path()).unwrap();
    assert!(source.contains("# My synthetic setup"));
    assert!(source.contains("# keep this note"));
    assert!(source.contains("language = 'system'"));
    assert!(source.contains("[future]\nvalue = 42 # another note"));
    assert_eq!(
        Config::new(config.path().into()).load().unwrap(),
        preferences
    );
    preferences.reset();
    config.save(&preferences).unwrap();
    assert_eq!(
        Config::new(config.path().into()).load().unwrap(),
        Preferences::default()
    );
}

#[test]
fn all_languages_round_trip_without_resolving_system_language() {
    let (_root, mut config) = fixture("");
    let mut preferences = config.load().unwrap();
    for language in Language::ALL {
        preferences.language = language;
        config.save(&preferences).unwrap();
        assert_eq!(
            Config::new(config.path().into()).load().unwrap().language,
            language
        );
    }
}

#[test]
fn invalid_files_report_the_setting_and_are_never_overwritten() {
    for (source, key) in [
        ("[ui]\ntheme = 'sepia'", "theme"),
        ("[ui]\nlanguage = 'unknown'", "language"),
        ("[ui.fonts]\ncode_family = '  '", "code_family"),
        ("[ui.fonts]\ncode_size = 10", "code_size"),
        ("[ui.fonts]\ncode_size = 25", "code_size"),
        ("[ui.fonts]\ncode_size = -1", "code_size"),
        ("[ui.fonts]\nui_size = 17", "ui_size"),
        ("[ui.fonts]\nui_size = 10", "ui_size"),
        ("[ui.fonts]\ncode_size = 14.5", "code_size"),
        ("[ui.fonts]\nui_size = '13'", "ui_size"),
        ("[ui.theme]", "theme"),
        ("[ui]\ntheme = [", "TOML"),
        ("[ui]\ntheme = 'dark'\ntheme = 'light'", "TOML"),
        ("ui = 1", "ui"),
        ("[ui]\nfonts = false", "ui.fonts"),
    ] {
        let (_root, mut config) = fixture(source);
        let error = config.load().unwrap_err();
        assert!(error.contains(key), "{source}: {error}");
        assert!(error.contains("config.toml"));
        assert!(config.save(&Preferences::default()).is_err());
        assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
    }
}

#[test]
fn external_edits_and_deletions_block_saving_until_reloaded() {
    let (_root, mut config) = fixture("[ui]\ntheme = 'dark'\n");
    let mut preferences = config.load().unwrap();
    fs::write(config.path(), "[ui]\ntheme = 'light' # manual edit\n").unwrap();
    preferences.code_size = 20;
    assert!(config.save(&preferences).unwrap_err().contains("restart"));
    assert_eq!(
        fs::read_to_string(config.path()).unwrap(),
        "[ui]\ntheme = 'light' # manual edit\n"
    );
    assert!(config.load().unwrap().light_theme);
    config.save(&preferences).unwrap();
    fs::remove_file(config.path()).unwrap();
    assert!(config.save(&preferences).is_err());
    assert!(!config.path().exists());
}

#[test]
fn invalid_in_memory_values_and_unreadable_paths_cannot_destroy_a_file() {
    let (_root, mut config) = fixture("[ui.fonts]\ncode_size = 14\n");
    let mut preferences = config.load().unwrap();
    preferences.code_size = 255;
    assert!(config.save(&preferences).is_err());
    assert_eq!(
        fs::read_to_string(config.path()).unwrap(),
        "[ui.fonts]\ncode_size = 14\n"
    );
    let root = tempfile::tempdir().unwrap();
    assert!(Config::new(root.path().into()).load().is_err());
    let blocked = root.path().join("blocked");
    fs::write(&blocked, "synthetic file").unwrap();
    assert!(Config::new(blocked.join("config.toml")).load().is_err());
}

#[cfg(unix)]
#[test]
fn saving_a_symlink_preserves_the_link_and_target_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("dotfiles.toml");
    fs::write(&target, "# synthetic dotfile\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    let path = root.path().join("config.toml");
    symlink(&target, &path).unwrap();
    let mut config = Config::new(path.clone());
    let mut preferences = config.load().unwrap();
    preferences.code_size = 18;
    config.save(&preferences).unwrap();
    assert!(path.is_symlink());
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(Config::new(target).load().unwrap().code_size, 18);
}
