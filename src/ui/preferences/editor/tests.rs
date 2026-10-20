use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

#[test]
fn copying_a_maximum_length_unicode_name_leaves_room_for_the_copy_suffix() {
    for language in [Language::English, Language::Chinese] {
        let name = copy_name(&"界".repeat(80), language);
        assert!(name.chars().count() <= 80);
        assert!(name.ends_with(if language == Language::Chinese {
            "副本"
        } else {
            "copy"
        }));
    }
}

#[gpui_test::test]
fn saving_a_copy_preserves_system_mode_and_applies_only_its_own_choice(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let store = tervia::themes::Store::new(root.path().join("config.toml"));
    let mut editor = None;
    let mut preferences = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let prefs = cx.new(|_| {
            let mut prefs = Preferences::default();
            prefs.theme.select(ColorTheme::Paper);
            prefs.theme.set_mode(Mode::System);
            prefs.theme.sync_system(true);
            prefs
        });
        preferences = Some(prefs.clone());
        let data = super::super::super::theme::snapshot(&ColorTheme::Dark);
        let entity = cx.new(|cx| Editor::new(data, None, store.clone(), prefs, None, window, cx));
        editor = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let editor = editor.unwrap();
    let preferences = preferences.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |this, cx| {
            this.draft.set_name("Synthetic saved night");
            this.save(window, cx);
        })
    });
    cx.run_until_parked();
    let catalog = store.list().unwrap();
    assert_eq!(catalog.themes.len(), 1);
    cx.cx.update(|cx| {
        let prefs = preferences.read(cx);
        assert_eq!(
            prefs.theme.selected(false).id(),
            catalog.themes[0].data.id()
        );
        assert_eq!(prefs.theme.current(), ColorTheme::Paper);
        assert_eq!(prefs.theme.mode(), Mode::System);
    });
}

#[gpui_test::test]
fn color_inputs_preview_locally_validate_and_reset_without_changing_preferences(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let mut editor = None;
    let mut preferences = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let prefs = cx.new(|_| Preferences::default());
        preferences = Some(prefs.clone());
        let data = super::super::super::theme::snapshot(&ColorTheme::Dark);
        let entity = cx.new(|cx| {
            Editor::new(
                data,
                None,
                tervia::themes::Store::new(root.path().join("config.toml")),
                prefs,
                None,
                window,
                cx,
            )
        });
        editor = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let editor = editor.unwrap();
    let preferences = preferences.unwrap();
    cx.simulate_resize(size(px(880.0), px(600.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("theme-editor-preview").is_some());
    assert!(cx.debug_bounds("theme-editor-save").is_some());
    let input = cx.update(|_, cx| editor.read(cx).controls["ui.text"].input.clone());
    cx.update(|window, cx| input.update(cx, |input, cx| input.replace_all("#abcdef", window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).draft.data().color("ui.text").unwrap().hex(),
            "#abcdef"
        );
        assert_eq!(preferences.read(cx).theme.current(), ColorTheme::Dark);
    });
    cx.update(|window, cx| input.update(cx, |input, cx| input.replace_all("#invalid", window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!editor.read(cx).draft.can_save());
        assert_eq!(
            editor.read(cx).draft.data().color("ui.text").unwrap().hex(),
            "#abcdef"
        );
    });
    cx.update(|window, cx| editor.update(cx, |this, cx| this.reset_color("ui.text", window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(editor.read(cx).draft.can_save()));
    cx.update(|window, cx| input.update(cx, |input, cx| input.replace_all("#123abc", window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).draft.data().color("ui.text").unwrap().hex(),
            "#123abc"
        )
    });
    assert!(
        tervia::themes::Store::new(root.path().join("config.toml"))
            .list()
            .unwrap()
            .themes
            .is_empty()
    );
}

#[gpui_test::test]
fn editing_and_renaming_an_existing_theme_keeps_its_file_identity(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let store = tervia::themes::Store::new(root.path().join("config.toml"));
    let document = store
        .create(&super::super::super::theme::snapshot(&ColorTheme::Dark))
        .unwrap();
    let id = document.data.id().to_owned();
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let prefs = cx.new(|_| Preferences::default());
        let data = document.data.as_ref().clone();
        let entity =
            cx.new(|cx| Editor::new(data, Some(document), store.clone(), prefs, None, window, cx));
        editor = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let editor = editor.unwrap();
    let name = cx.update(|_, cx| editor.read(cx).name.clone());
    cx.update(|window, cx| {
        name.update(cx, |name, cx| {
            name.replace_all("Synthetic renamed night", window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = cx.debug_bounds("theme-editor-save").unwrap();
    cx.simulate_click(save.center(), Modifiers::default());
    cx.run_until_parked();
    let catalog = store.list().unwrap();
    assert_eq!(catalog.themes.len(), 1);
    assert_eq!(catalog.themes[0].data.id(), id);
    assert_eq!(catalog.themes[0].data.name(), "Synthetic renamed night");
}

#[gpui_test::test]
fn closing_a_dirty_editor_requires_discard_confirmation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let prefs = cx.new(|_| Preferences::default());
        let data = super::super::super::theme::snapshot(&ColorTheme::Dark);
        let entity = cx.new(|cx| {
            Editor::new(
                data,
                None,
                tervia::themes::Store::new(root.path().join("config.toml")),
                prefs,
                None,
                window,
                cx,
            )
        });
        editor = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |this, cx| {
            this.draft.set_name("Synthetic edited name");
            this.request_close(window, cx);
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_cancel"));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(editor.read(cx).draft.is_dirty()));
    assert!(!cx.cx.update(guard_quit));
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "Application quit must also protect unsaved theme drafts"
    );
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_cancel"));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(editor.read(cx).draft.is_dirty()));
}
