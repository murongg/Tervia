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
fn light_and_dark_themes_are_configured_independently() {
    let (_root, mut config) = fixture(
        "[ui]\ntheme = 'dark'\ndark_theme = 'midnight' # mock night choice\nlight_theme = 'paper' # mock day choice\n",
    );
    let mut preferences = config.load().unwrap();
    assert_eq!(
        preferences.theme.current(),
        tervia::preferences::Theme::Midnight
    );
    assert_eq!(
        preferences.theme.selected(true),
        tervia::preferences::Theme::Paper
    );
    preferences.theme.toggle();
    config.save(&preferences).unwrap();
    assert_eq!(
        Config::new(config.path().into()).load().unwrap(),
        preferences
    );
    let saved = fs::read_to_string(config.path()).unwrap();
    assert!(saved.contains("# mock night choice"));
    assert!(saved.contains("# mock day choice"));
    let document = saved.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(document["ui"]["theme"].as_str(), Some("light"));
    assert_eq!(document["ui"]["dark_theme"].as_str(), Some("midnight"));
    assert_eq!(document["ui"]["light_theme"].as_str(), Some("paper"));
}

#[test]
fn theme_choices_reject_the_wrong_mode_and_preserve_the_document() {
    for (key, value) in [
        ("dark_theme", "paper"),
        ("light_theme", "forest"),
        ("dark_theme", "mock-unknown"),
        ("light_theme", "mock-unknown"),
    ] {
        let source = format!("[ui]\n{key} = '{value}'\n");
        let (_root, mut config) = fixture(&source);
        assert!(config.load().is_err(), "{key} = {value}");
        assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
    }
}

#[test]
fn theme_presets_survive_saving_other_settings_and_restarting() {
    for id in [
        "dark",
        "light",
        "midnight",
        "forest",
        "paper",
        "rose",
        "darcula",
        "monokai",
        "solarized-dark",
        "solarized-light",
        "quiet-light",
    ] {
        let (_root, mut config) = fixture(&format!(
            "[ui]\ntheme = '{id}' # synthetic choice\nfuture = 'mock'\n"
        ));
        let mut preferences = config
            .load()
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        preferences.set_ui_size(15);
        config.save(&preferences).unwrap();
        let saved = fs::read_to_string(config.path()).unwrap();
        let document = saved.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(preferences.theme.current().id(), id);
        let key = if preferences.theme.is_light() {
            "light_theme"
        } else {
            "dark_theme"
        };
        assert_eq!(document["ui"][key].as_str(), Some(id));
        assert!(saved.contains("# synthetic choice"));
        assert!(saved.contains("future = 'mock'"));
        assert_eq!(
            Config::new(config.path().into()).load().unwrap(),
            preferences
        );
    }
}

#[test]
fn classic_presets_remember_each_dark_and_light_pair() {
    for dark in ["darcula", "monokai", "solarized-dark"] {
        for light in ["solarized-light", "quiet-light"] {
            let (_root, mut config) = fixture(&format!(
                "[ui]\ntheme = 'dark'\ndark_theme = '{dark}'\nlight_theme = '{light}'\n"
            ));
            let mut preferences = config.load().unwrap();
            assert_eq!(preferences.theme.current().id(), dark);
            preferences.theme.toggle();
            assert_eq!(preferences.theme.current().id(), light);
            config.save(&preferences).unwrap();
            let mut restarted = Config::new(config.path().into()).load().unwrap();
            assert_eq!(restarted, preferences);
            restarted.theme.toggle();
            assert_eq!(restarted.theme.current().id(), dark);
        }
    }
}

#[test]
fn system_appearance_persists_without_replacing_the_two_theme_choices() {
    let (_root, mut config) = fixture(
        "[ui]\ntheme = 'system' # mock appearance\ndark_theme = 'monokai'\nlight_theme = 'quiet-light'\n",
    );
    let preferences = config.load().unwrap();
    config.save(&preferences).unwrap();
    let source = fs::read_to_string(config.path()).unwrap();
    let document = source.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(document["ui"]["theme"].as_str(), Some("system"));
    assert_eq!(document["ui"]["dark_theme"].as_str(), Some("monokai"));
    assert_eq!(document["ui"]["light_theme"].as_str(), Some("quiet-light"));
    assert!(source.contains("# mock appearance"));
    assert_eq!(
        Config::new(config.path().into()).load().unwrap(),
        preferences
    );
}

#[test]
fn agent_notifications_default_off_and_explicit_opt_in_survives_reset() {
    assert!(!Preferences::default().agent_notifications);
    let (_root, mut config) = fixture("");
    assert!(!config.load().unwrap().agent_notifications);
    let root = tempfile::tempdir().unwrap();
    let mut first_launch = Config::new(root.path().join("mock-config.toml"));
    assert!(!first_launch.load().unwrap().agent_notifications);
    let (_root, mut enabled) = fixture("[agents]\nnotifications = true\n");
    let mut preferences = enabled.load().unwrap();
    preferences.reset();
    assert!(preferences.agent_notifications);
    enabled.save(&preferences).unwrap();
    assert!(
        Config::new(enabled.path().into())
            .load()
            .unwrap()
            .agent_notifications
    );
}

#[test]
fn agent_notifications_are_persisted_and_reset_does_not_unmute_them() {
    let (_root, mut config) =
        fixture("[agents]\nnotifications = false # mock choice\nfuture = 'mock'\n");
    let mut preferences = config.load().unwrap();
    assert!(!preferences.agent_notifications);
    preferences.reset();
    assert!(!preferences.agent_notifications);
    config.save(&preferences).unwrap();
    assert!(
        !Config::new(config.path().into())
            .load()
            .unwrap()
            .agent_notifications
    );
    assert!(
        fs::read_to_string(config.path())
            .unwrap()
            .contains("# mock choice")
    );
    let (_root, mut config) = fixture("[agents]\nnotifications = 'invalid'\n");
    assert!(config.load().is_err());
}

#[test]
fn terminal_shell_rejects_unknown_profiles_and_non_string_values() {
    for value in [
        "'mock-shell'",
        "'./mock-shell'",
        "false",
        "7",
        "[]",
        "\"/synthetic/mock\\u0000shell\"",
    ] {
        let (_root, mut config) = fixture(&format!("[terminal]\nshell = {value}\n"));
        assert!(config.load().is_err(), "Invalid terminal.shell: {value}");
    }
}

#[test]
fn terminal_shell_defaults_and_explicit_profiles_round_trip_with_comments() {
    use tervia::shell::Profile;
    let (_root, mut config) = fixture("");
    assert_eq!(config.load().unwrap().shell, Profile::Default);
    for (id, profile) in [
        ("default", Profile::Default),
        ("pwsh", Profile::PowerShell),
        ("powershell", Profile::WindowsPowerShell),
        ("cmd", Profile::Cmd),
    ] {
        let (_root, mut config) = fixture(&format!(
            "[terminal]\nshell = '{id}' # synthetic choice\nfuture = 'mock'\n"
        ));
        let mut preferences = config.load().unwrap();
        assert_eq!(preferences.shell, profile);
        preferences.shell = Profile::Cmd;
        config.save(&preferences).unwrap();
        assert_eq!(
            Config::new(config.path().into()).load().unwrap().shell,
            Profile::Cmd
        );
        let source = fs::read_to_string(config.path()).unwrap();
        assert!(source.contains("# synthetic choice"));
        assert!(source.contains("future = 'mock'"));
    }
}

#[test]
fn terminal_shell_accepts_absolute_paths_without_losing_comments() {
    let executable = if cfg!(windows) {
        r"C:\synthetic tools\项目\mock-shell.exe"
    } else {
        "/synthetic tools/项目/mock-shell"
    };
    let (_root, mut config) = fixture(&format!(
        "[terminal]\nshell = '{executable}' # synthetic custom shell\n"
    ));
    let mut preferences = config
        .load()
        .expect("absolute custom shell paths are valid");
    assert_eq!(preferences.shell.id(), executable);
    assert_eq!(
        preferences.shell.resolve(),
        std::ffi::OsStr::new(executable)
    );
    preferences.ui_size = 14;
    config.save(&preferences).unwrap();
    assert_eq!(config.load().unwrap().shell.id(), executable);
    assert!(
        fs::read_to_string(config.path())
            .unwrap()
            .contains("# synthetic custom shell")
    );
}

#[test]
fn terminal_shell_arguments_reject_invalid_types_and_nuls() {
    for value in [
        "true",
        "7",
        "'-i'",
        "{ mock = '-i' }",
        "[7]",
        "['-i', false]",
        "[\"\\u0000\"]",
    ] {
        let source = format!("[terminal]\nshell_args = {value}\n");
        let (_root, mut config) = fixture(&source);
        let error = config
            .load()
            .expect_err("invalid shell arguments must be rejected");
        assert!(error.contains("terminal.shell_args"), "{error}");
        assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
    }
}

#[test]
fn terminal_shell_arguments_distinguish_defaults_empty_and_literal_values() {
    let (_root, mut config) = fixture("[terminal]\nfuture = 'mock'\n");
    let mut preferences = config.load().unwrap();
    assert_eq!(preferences.shell_args, None);
    for arguments in [
        Vec::<String>::new(),
        [
            "",
            "--mock",
            "space value",
            "项目",
            "quote\"value",
            r"back\slash",
            "$HOME; $(mock) | & *",
        ]
        .map(str::to_owned)
        .to_vec(),
    ] {
        preferences.shell_args = Some(arguments.clone());
        config.save(&preferences).unwrap();
        assert_eq!(config.load().unwrap().shell_args, Some(arguments));
    }
    preferences.shell_args = None;
    config.save(&preferences).unwrap();
    assert_eq!(config.load().unwrap().shell_args, None);
    let saved = fs::read_to_string(config.path()).unwrap();
    assert!(
        !saved.parse::<toml_edit::DocumentMut>().unwrap()["terminal"]
            .as_table_like()
            .unwrap()
            .contains_key("shell_args")
    );
    assert!(saved.contains("future = 'mock'"));
}

#[test]
fn unchanged_shell_arguments_keep_array_formatting_and_comments() {
    let arguments = r#"shell_args = [
    '--mock', # synthetic argument
    'space value',
    '项目',
    'quote"value',
    'back\slash',
    '$HOME; $(mock) | & *',
    '',
] # synthetic array
"#;
    let (_root, mut config) = fixture(&format!("[terminal]\n{arguments}future = 'mock'\n"));
    let mut preferences = config.load().unwrap();
    preferences.ui_size = 14;
    config.save(&preferences).unwrap();
    assert!(
        fs::read_to_string(config.path())
            .unwrap()
            .contains(arguments)
    );
    preferences.shell_args = Some(vec!["synthetic replacement".into()]);
    config.save(&preferences).unwrap();
    let saved = fs::read_to_string(config.path()).unwrap();
    assert!(saved.contains("# synthetic array"));
    assert!(saved.contains("future = 'mock'"));
    assert_eq!(config.load().unwrap().shell_args, preferences.shell_args);
}

#[test]
fn workspace_reset_removes_explicit_shell_arguments_from_config() {
    let (_root, mut config) = fixture(
        "[terminal]\nshell = 'cmd'\nshell_args = []\nfuture = 'mock' # synthetic future option\n",
    );
    let mut preferences = config.load().unwrap();
    preferences.reset_workspace();
    config.save(&preferences).unwrap();
    let reloaded = config.load().unwrap();
    assert_eq!(reloaded.shell_args, None);
    assert_eq!(reloaded.shell, tervia::shell::Profile::Default);
    assert!(
        fs::read_to_string(config.path())
            .unwrap()
            .contains("future = 'mock' # synthetic future option")
    );
}

#[test]
fn invalid_in_memory_shell_arguments_cannot_replace_the_config() {
    let source = "[terminal]\nshell_args = ['--mock'] # synthetic arguments\n";
    let (_root, mut config) = fixture(source);
    let mut preferences = config.load().unwrap();
    preferences.shell_args = Some(vec!["synthetic\0argument".into()]);
    let error = config.save(&preferences).unwrap_err();
    assert!(error.contains("terminal.shell_args"), "{error}");
    assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
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
            theme: tervia::preferences::Theme::Light.into(),
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
fn file_nesting_defaults_off_and_round_trips_without_losing_comments() {
    let (_root, mut config) =
        fixture("[files]\nnesting = true # synthetic preference\nfuture_option = 'synthetic'\n");
    let mut preferences = config.load().unwrap();
    assert!(preferences.file_nesting);
    preferences.file_nesting = false;
    config.save(&preferences).unwrap();
    assert!(
        !Config::new(config.path().into())
            .load()
            .unwrap()
            .file_nesting
    );
    let source = fs::read_to_string(config.path()).unwrap();
    assert!(source.contains("# synthetic preference"));
    assert!(source.contains("future_option = 'synthetic'"));
    let (_root, mut config) = fixture("");
    assert!(!config.load().unwrap().file_nesting);
}

#[test]
fn terminal_suggestions_default_on_and_round_trip_with_comments() {
    let (_root, mut config) =
        fixture("[terminal]\nsuggestions = false # synthetic preference\nfuture = 'mock'\n");
    let mut preferences = config.load().unwrap();
    assert!(!preferences.command_suggestions);
    preferences.command_suggestions = true;
    config.save(&preferences).unwrap();
    assert!(
        Config::new(config.path().into())
            .load()
            .unwrap()
            .command_suggestions
    );
    let source = fs::read_to_string(config.path()).unwrap();
    assert!(source.contains("# synthetic preference"));
    assert!(source.contains("future = 'mock'"));
    let (_root, mut config) = fixture("");
    assert!(config.load().unwrap().command_suggestions);
    for source in ["terminal = false", "[terminal]\nsuggestions = 'yes'"] {
        let (_root, mut config) = fixture(source);
        assert!(config.load().unwrap_err().contains("terminal"));
        assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
    }
}

#[test]
fn invalid_file_nesting_settings_leave_the_config_untouched() {
    for source in [
        "files = false",
        "[files]\nnesting = 'yes'",
        "[files]\nnesting = 1",
    ] {
        let (_root, mut config) = fixture(source);
        assert!(config.load().unwrap_err().contains("files"));
        assert!(config.save(&Preferences::default()).is_err());
        assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
    }
}

#[test]
fn update_settings_round_trip_with_custom_intervals_and_preserve_comments() {
    let (_root, mut config) = fixture(
        "[updates]\nauto_check = false # synthetic switch\ninterval_minutes = 37 # synthetic interval\nfuture_option = 'synthetic'\n",
    );
    let mut preferences = config.load().unwrap();
    assert!(!preferences.updates.auto_check);
    assert_eq!(preferences.updates.interval_minutes, 37);
    preferences.updates.auto_check = true;
    preferences.updates.interval_minutes = 120;
    config.save(&preferences).unwrap();
    assert_eq!(
        Config::new(config.path().into()).load().unwrap(),
        preferences
    );
    let source = fs::read_to_string(config.path()).unwrap();
    assert!(source.contains("# synthetic switch"));
    assert!(source.contains("# synthetic interval"));
    assert!(source.contains("future_option = 'synthetic'"));
    preferences.updates.interval_minutes = 0;
    assert!(config.save(&preferences).is_err());
    assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
}

#[test]
fn invalid_update_configuration_is_rejected_without_overwriting_it() {
    for (source, key) in [
        ("updates = false", "updates"),
        ("[updates]\nauto_check = 'yes'", "updates.auto_check"),
        (
            "[updates]\ninterval_minutes = 0",
            "updates.interval_minutes",
        ),
        (
            "[updates]\ninterval_minutes = 4",
            "updates.interval_minutes",
        ),
        (
            "[updates]\ninterval_minutes = 10081",
            "updates.interval_minutes",
        ),
        (
            "[updates]\ninterval_minutes = -1",
            "updates.interval_minutes",
        ),
        (
            "[updates]\ninterval_minutes = '60'",
            "updates.interval_minutes",
        ),
        (
            "[updates]\ninterval_minutes = 60.5",
            "updates.interval_minutes",
        ),
    ] {
        let (_root, mut config) = fixture(source);
        assert!(config.load().unwrap_err().contains(key));
        assert!(config.save(&Preferences::default()).is_err());
        assert_eq!(fs::read_to_string(config.path()).unwrap(), source);
    }
}

#[test]
fn saves_preserve_comments_unknown_settings_and_unchanged_formatting() {
    let (_root, mut config) = fixture(
        "# My synthetic setup\n[ui]\ntheme = 'dark' # keep this note\nlanguage = 'system'\n\n[future]\nvalue = 42 # another note\n",
    );
    let mut preferences = config.load().unwrap();
    preferences.theme = tervia::preferences::Theme::Light.into();
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
    assert!(config.load().unwrap().theme.is_light());
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
