use std::collections::BTreeMap;
use tervia::themes::{COLOR_KEYS, Color, Definition, Draft, Store};

fn synthetic() -> Definition {
    Definition::new(
        "user-synthetic",
        "Synthetic night",
        false,
        COLOR_KEYS
            .iter()
            .map(|key| ((*key).into(), Color::parse("#234567").unwrap()))
            .collect(),
        serde_json::json!({
            "keyword": {"color": "#aabbcc", "font_style": "italic", "font_weight": 700},
            "string": {"color": "#bbccdd"},
            "number": {"color": "#ccddee"},
            "comment": {"color": "#8899aa"},
            "function": {"color": "#ddeeff"},
            "type": {"color": "#abcdef"}
        }),
    )
    .unwrap()
}

#[test]
fn snapshots_round_trip_all_colors_and_syntax_styles() {
    let data = synthetic();
    let source = data.to_toml().unwrap();
    assert_eq!(Definition::parse(&source).unwrap(), data);
    assert_eq!(data.color("syntax.keyword").unwrap().hex(), "#aabbcc");
    assert_eq!(data.syntax()["keyword"]["font_style"], "italic");
    assert!(Definition::parse(&source.replace("#234567", "not-a-color")).is_err());
}

#[test]
fn draft_retains_last_valid_preview_and_blocks_invalid_saves() {
    let mut draft = Draft::new(synthetic());
    let previous = draft.data().color("ui.accent").unwrap();
    assert!(draft.set_color("ui.accent", "#oops").is_err());
    assert_eq!(draft.data().color("ui.accent").unwrap(), previous);
    assert!(!draft.can_save());
    draft.set_color("ui.accent", "#12ab34").unwrap();
    assert!(draft.can_save());
    assert!(draft.is_dirty());
    draft.reset_color("ui.accent").unwrap();
    assert_eq!(draft.data().color("ui.accent").unwrap(), previous);
    assert!(!draft.is_dirty());
    draft.set_name("  ");
    assert!(!draft.can_save());
}

#[test]
fn colors_validate_rgb_alpha_and_reject_non_hex_content() {
    assert_eq!(Color::parse("#aAbBcC").unwrap().hex(), "#aabbcc");
    assert_eq!(Color::parse("#aabbcc80").unwrap().hex(), "#aabbcc80");
    for invalid in ["", "#fff", "#1122334455", "#gg1122", "red", "#11223é"] {
        assert!(Color::parse(invalid).is_err(), "{invalid}");
    }
    assert!(Definition::parse("").is_err());
}

#[test]
fn contrast_accounts_for_transparent_foreground_and_composited_backgrounds() {
    let black = Color::parse("#000000").unwrap();
    let white = Color::parse("#ffffff").unwrap();
    let transparent = Color::parse("#ffffff00").unwrap();
    assert!((white.contrast(black) - 21.0).abs() < 0.01);
    assert!((transparent.contrast(black) - 1.0).abs() < 0.01);
    assert_eq!(transparent.over(black), black);
}

#[test]
fn removing_a_custom_theme_only_resets_its_own_appearance() {
    use tervia::preferences::{Mode, Theme, Themes};
    let data = std::sync::Arc::new(synthetic());
    let mut themes = Themes::default();
    themes.select(Theme::Custom(data.clone()));
    themes.select(Theme::Paper);
    themes.set_mode(Mode::System);
    themes.sync_system(true);
    assert!(themes.remove_custom(data.id()));
    assert_eq!(themes.selected(false), Theme::Dark);
    assert_eq!(themes.current(), Theme::Paper);
    assert_eq!(themes.mode(), Mode::System);
    assert!(!themes.remove_custom(data.id()));
}

#[test]
fn store_creates_unique_files_and_rename_keeps_identity() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path().join("config.toml"));
    let first = store.create(&synthetic()).unwrap();
    let second = store.create(&synthetic()).unwrap();
    assert_ne!(first.data.id(), second.data.id());
    let mut renamed = first.data.as_ref().clone();
    renamed.set_name("Synthetic renamed").unwrap();
    let saved = store.save(&first, &renamed).unwrap();
    assert_eq!(saved.data.id(), first.data.id());
    assert_eq!(
        store.load(first.data.id()).unwrap().data.name(),
        "Synthetic renamed"
    );
    assert_eq!(store.list().unwrap().themes.len(), 2);
    store.delete(&saved).unwrap();
    assert_eq!(store.list().unwrap().themes.len(), 1);
}

#[test]
fn store_preserves_comments_and_refuses_to_overwrite_external_edits() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path().join("config.toml"));
    let original = store.create(&synthetic()).unwrap();
    let path = root
        .path()
        .join("themes")
        .join(format!("{}.toml", original.data.id()));
    let commented = format!(
        "# synthetic comment\n{}\nfuture = 'synthetic'\n",
        original.source.replace(
            "name = \"Synthetic night\"",
            "name = \"Synthetic night\" # synthetic name comment"
        )
    );
    std::fs::write(&path, &commented).unwrap();
    let document = store.load(original.data.id()).unwrap();
    let mut renamed = document.data.as_ref().clone();
    renamed.set_name("Synthetic rename").unwrap();
    let saved = store.save(&document, &renamed).unwrap();
    assert!(saved.source.contains("# synthetic comment"));
    assert!(saved.source.contains("# synthetic name comment"));
    assert!(saved.source.contains("future = 'synthetic'"));
    std::fs::write(&path, "# external synthetic edit").unwrap();
    assert!(store.save(&saved, &renamed).is_err());
    assert!(store.delete(&saved).is_err());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# external synthetic edit"
    );
}

#[test]
fn malformed_files_are_isolated_and_identifiers_cannot_escape_the_store() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path().join("config.toml"));
    store.create(&synthetic()).unwrap();
    std::fs::write(root.path().join("themes/user-broken.toml"), "invalid [[").unwrap();
    let catalog = store.list().unwrap();
    assert_eq!(catalog.themes.len(), 1);
    assert_eq!(catalog.errors.len(), 1);
    for invalid in [
        "../config",
        "user-../../outside",
        "/tmp/mock",
        "dark",
        "user-",
    ] {
        assert!(store.load(invalid).is_err(), "{invalid}");
    }
    assert!(
        Definition::new(
            "dark",
            "Synthetic",
            false,
            BTreeMap::new(),
            serde_json::json!({})
        )
        .is_err()
    );
}

#[test]
fn custom_selections_persist_without_changing_system_mode_or_the_other_choice() {
    use tervia::preferences::{Mode, Theme};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    let store = Store::new(path.clone());
    let document = store.create(&synthetic()).unwrap();
    std::fs::write(
        &path,
        format!(
            "[ui]\ntheme = 'system'\ndark_theme = '{}'\nlight_theme = 'paper'\n",
            document.data.id()
        ),
    )
    .unwrap();
    let mut config = tervia::config::Config::new(path.clone());
    let mut preferences = config.load().unwrap();
    assert_eq!(preferences.theme.selected(false).id(), document.data.id());
    assert_eq!(preferences.theme.selected(true), Theme::Paper);
    assert_eq!(preferences.theme.mode(), Mode::System);
    preferences.set_code_size(17);
    config.save(&preferences).unwrap();
    assert_eq!(
        tervia::config::Config::new(path.clone()).load().unwrap(),
        preferences
    );
    store.delete(&document).unwrap();
    let recovered = tervia::config::Config::new(path).load().unwrap();
    assert_eq!(recovered.theme.selected(false), Theme::Dark);
    assert_eq!(recovered.theme.selected(true), Theme::Paper);
    assert_eq!(recovered.theme.mode(), Mode::System);
}
