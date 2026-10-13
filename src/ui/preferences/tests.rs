use core::prelude::v1::test;
use gpui_kit::component::{ActiveTheme, ThemeMode};
use gpui_kit::*;
use gpui_test::TestAppContext;
use std::fs;
use tervia::{config::Config, locale::Language, preferences::Preferences};

use super::super::Tervia;

#[gpui_test::test]
fn settings_controls_align_without_overlapping_long_labels_at_minimum_size(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut settings = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = Preferences {
            language: Language::German,
            code_size: 24,
            ui_size: 16,
            ..Preferences::default()
        };
        super::apply(&preferences, cx);
        let preferences = cx.new(|_| preferences);
        let error = cx.new(|_| Some("Synthetic configuration error".into()));
        let entity = cx.new(|cx| {
            super::PreferencesWindow::new(
                preferences,
                Some("/synthetic/config.toml".into()),
                error,
                window,
                cx,
            )
        });
        settings = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let settings = settings.unwrap();
    cx.simulate_resize(size(px(700.0), px(580.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let language = cx
        .debug_bounds("language-control")
        .expect("Language control bounds");
    let font = cx
        .debug_bounds("font-control")
        .expect("Font control bounds");
    let navigation = cx
        .debug_bounds("settings-navigation")
        .expect("Category navigation");
    assert_eq!(navigation.size.width, px(176.0));
    assert_eq!(language.left(), font.left());
    assert_eq!(language.right(), font.right());
    for (label, control) in [("language", language), ("code_font", font)] {
        let label = cx.debug_bounds(label).unwrap();
        assert!(label.left() >= navigation.right() + px(24.0));
        assert!(label.right() + px(16.0) <= control.left());
        assert!(control.right() <= px(676.0));
    }
    let footer = cx
        .debug_bounds("settings-footer")
        .expect("Fixed settings footer");
    assert!(footer.bottom() <= px(580.0));
    assert!(footer.top() > font.bottom());
    let before = cx.update(|_, cx| settings.read(cx).current.clone());
    cx.update(|_, cx| {
        settings.update(cx, |this, cx| {
            this.scroll.set_offset(point(px(0.0), px(-1000.0)));
            cx.notify();
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.update(|_, cx| settings.read(cx).scroll.offset().y) < px(0.0));
    let category = cx.debug_bounds("settings-interface-nav").unwrap();
    cx.simulate_click(category.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).scroll.offset().y),
        px(0.0)
    );
    assert_eq!(cx.update(|_, cx| settings.read(cx).current.clone()), before);
}

#[gpui_test::test]
fn configured_preferences_apply_at_startup_and_ui_edits_persist(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(
        &path,
        "[ui]\ntheme = 'light'\nlanguage = 'ja'\n[ui.fonts]\ncode_size = 18\nui_size = 15\n",
    )
    .unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::configured(Ok(Config::new(path.clone())), window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert!(this.appearance.light_theme);
            assert_eq!(this.appearance.language, Language::Japanese);
            assert_eq!(cx.theme().mode, ThemeMode::Light);
            assert_eq!(cx.theme().font_size, px(15.0));
            assert_eq!(cx.theme().mono_font_size, px(18.0));
            this.preferences.update(cx, |preferences, cx| {
                preferences.code_size = 20;
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    assert_eq!(Config::new(path.clone()).load().unwrap().code_size, 20);
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.preferences.update(cx, |preferences, cx| {
                preferences.reset();
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    assert_eq!(Config::new(path).load().unwrap(), Preferences::default());
}

#[gpui_test::test]
fn invalid_config_uses_defaults_and_save_errors_remain_visible(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(&path, "[ui.fonts]\ncode_size = 999").unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::configured(Ok(Config::new(path.clone())), window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.appearance, Preferences::default());
            assert!(
                this.config_error
                    .read(cx)
                    .as_ref()
                    .unwrap()
                    .contains("code_size")
            );
            assert!(this.state.notice.contains("config.toml"));
            this.preferences.update(cx, |preferences, cx| {
                preferences.code_size = 20;
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.appearance.code_size, 20);
            assert!(this.config_error.read(cx).is_some());
        })
    });
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "[ui.fonts]\ncode_size = 999"
    );
}
