use core::prelude::v1::test;
use gpui_kit::component::{ActiveTheme, ThemeMode};
use gpui_kit::*;
use gpui_test::TestAppContext;
use std::fs;
use tervia::{config::Config, locale::Language, preferences::Preferences};

use super::super::Tervia;

#[gpui_test::test]
fn autostart_reads_system_state_and_toggles_without_changing_interface_preferences(
    cx: &mut TestAppContext,
) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    cx.update(gpui_kit::init);
    let system = Arc::new(AtomicBool::new(true));
    let mut settings = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences::default());
        let error = cx.new(|_| None);
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let entity = cx.new(|cx| {
            let mut settings =
                super::PreferencesWindow::new(preferences, None, error, updater, window, cx);
            let system = system.clone();
            settings.startup_action = Arc::new(move |enabled| {
                if let Some(enabled) = enabled {
                    system.store(enabled, Ordering::SeqCst);
                }
                Ok(system.load(Ordering::SeqCst))
            });
            settings
        });
        settings = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let settings = settings.unwrap();
    cx.simulate_resize(size(px(800.0), px(900.0)));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).startup_enabled),
        Some(true)
    );
    for enabled in [false, true] {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let switch = cx
            .debug_bounds("autostart-switch")
            .expect("Autostart is discoverable in settings");
        cx.simulate_click(switch.center(), Modifiers::default());
        cx.run_until_parked();
        assert_eq!(system.load(Ordering::SeqCst), enabled);
        cx.update(|_, cx| {
            let settings = settings.read(cx);
            assert_eq!(settings.startup_enabled, Some(enabled));
            assert!(settings.startup_error.is_none());
            assert_eq!(settings.current, Preferences::default());
        });
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let reset = cx.debug_bounds("settings-reset-defaults").unwrap();
    cx.simulate_click(reset.center(), Modifiers::default());
    assert!(system.load(Ordering::SeqCst));
}

#[gpui_test::test]
fn autostart_failure_keeps_the_last_state_and_shows_a_retry(cx: &mut TestAppContext) {
    use std::sync::Arc;
    cx.update(gpui_kit::init);
    let mut settings = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences::default());
        let error = cx.new(|_| None);
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let entity = cx.new(|cx| {
            let mut settings =
                super::PreferencesWindow::new(preferences, None, error, updater, window, cx);
            settings.startup_action = Arc::new(|enabled| {
                if enabled.is_some() {
                    Err("Synthetic registration failure".into())
                } else {
                    Ok(false)
                }
            });
            settings
        });
        settings = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let settings = settings.unwrap();
    cx.simulate_resize(size(px(800.0), px(900.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let switch = cx.debug_bounds("autostart-switch").unwrap();
    cx.simulate_click(switch.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        let settings = settings.read(cx);
        assert_eq!(settings.startup_enabled, Some(false));
        assert_eq!(
            settings.startup_error.as_deref(),
            Some("Synthetic registration failure")
        );
        window.draw(cx).clear(cx);
    });
    let retry = cx
        .debug_bounds("autostart-retry")
        .expect("Registration failures offer a status refresh");
    cx.simulate_click(retry.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(settings.read(cx).startup_error.is_none()));
}

#[gpui_test::test]
fn autostart_read_failure_disables_the_switch_until_a_successful_retry(cx: &mut TestAppContext) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    cx.update(gpui_kit::init);
    let failing = Arc::new(AtomicBool::new(true));
    let reads = Arc::new(AtomicUsize::new(0));
    let mut settings = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences::default());
        let error = cx.new(|_| None);
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let entity = cx.new(|cx| {
            let mut settings =
                super::PreferencesWindow::new(preferences, None, error, updater, window, cx);
            let failing = failing.clone();
            let reads = reads.clone();
            settings.startup_action = Arc::new(move |enabled| {
                assert_eq!(
                    enabled, None,
                    "An unreadable status must never allow a write"
                );
                reads.fetch_add(1, Ordering::SeqCst);
                if failing.load(Ordering::SeqCst) {
                    Err("Synthetic status failure".into())
                } else {
                    Ok(true)
                }
            });
            settings
        });
        settings = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let settings = settings.unwrap();
    cx.simulate_resize(size(px(800.0), px(1000.0)));
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| settings.read(cx).startup_enabled), None);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let before = reads.load(Ordering::SeqCst);
    let switch = cx.debug_bounds("autostart-switch").unwrap();
    cx.simulate_click(switch.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(reads.load(Ordering::SeqCst), before);
    failing.store(false, Ordering::SeqCst);
    let retry = cx.debug_bounds("autostart-retry").unwrap();
    cx.simulate_click(retry.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert_eq!(settings.startup_enabled, Some(true));
        assert!(settings.startup_error.is_none());
    });
}

#[gpui_test::test]
fn window_resize_records_restored_state_before_an_immediate_quit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            Tervia::configured(Ok(Config::new(root.path().join("config.toml"))), window, cx)
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    view.update(cx, |this, cx| {
        this.state.window_maximized = true;
        this.persist_workspace(cx);
    });
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            assert!(!this.state.window_maximized);
            this.shutdown_terminals(cx);
        });
    });
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("workspace.json")).unwrap())
            .unwrap();
    assert_eq!(saved["window_maximized"], false);
}

#[gpui_test::test]
fn fullscreen_resize_preserves_the_previous_window_maximization(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(Tervia::new);
    view.update(cx, |this, _| this.state.window_maximized = true);
    cx.update(|window, _| window.toggle_fullscreen());
    cx.simulate_resize(size(px(1920.0), px(1080.0)));
    view.update(cx, |this, _| assert!(this.state.window_maximized));
    cx.update(|window, _| window.toggle_fullscreen());
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    view.update(cx, |this, _| assert!(!this.state.window_maximized));
}

#[gpui_test::test]
fn workspace_scrollbars_auto_hide_in_both_themes(cx: &mut TestAppContext) {
    use gpui_kit::base::{ScrollbarMode, Theme};
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        for light in [false, true] {
            super::apply(
                &Preferences {
                    light_theme: light,
                    ..Preferences::default()
                },
                cx,
            );
            assert_eq!(Theme::global(cx).scrollbar.mode(), ScrollbarMode::Scrolling);
        }
    });
}

#[gpui_test::test]
fn settings_scrollbar_drags_the_overflowing_form(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::apply(&Preferences::default(), cx);
    });
    let mut settings = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences {
            ui_size: 16,
            code_size: 24,
            ..Preferences::default()
        });
        let error = cx.new(|_| None);
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let entity = cx
            .new(|cx| super::PreferencesWindow::new(preferences, None, error, updater, window, cx));
        settings = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let settings = settings.unwrap();
    cx.simulate_resize(size(px(700.0), px(580.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bar = cx
        .debug_bounds("scrollbar-overlay")
        .expect("The settings form needs a scrollbar");
    cx.simulate_event(ScrollWheelEvent {
        position: bar.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-24.0))),
        ..Default::default()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let from = point(bar.right() - px(8.0), bar.top() + px(24.0));
    let to = point(from.x, bar.bottom() - px(24.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| assert!(settings.read(cx).scroll.offset().y < px(0.0)));
}

#[gpui_test::test]
fn file_nesting_switch_updates_the_shared_preference(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut shared = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences::default());
        shared = Some(preferences.clone());
        let error = cx.new(|_| None);
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let settings = cx
            .new(|cx| super::PreferencesWindow::new(preferences, None, error, updater, window, cx));
        gpui_kit::base::Root::new(settings, window, cx)
    });
    let shared = shared.unwrap();
    cx.simulate_resize(size(px(800.0), px(800.0)));
    for enabled in [true, false] {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let control = cx
            .debug_bounds("file-nesting-switch")
            .expect("File nesting is discoverable in Interface settings");
        cx.simulate_click(control.center(), Modifiers::default());
        cx.update(|_, cx| assert_eq!(shared.read(cx).file_nesting, enabled));
    }
}

#[gpui_test::test]
fn configured_update_policy_loads_into_the_scheduler_and_ui_edits_persist(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(
        &path,
        "[updates]\nauto_check = false\ninterval_minutes = 37\n",
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
            assert!(!this.preferences.read(cx).updates.auto_check);
            assert_eq!(this.preferences.read(cx).updates.interval_minutes, 37);
            this.preferences.update(cx, |preferences, cx| {
                preferences.updates.auto_check = true;
                preferences.updates.interval_minutes = 15;
                cx.notify();
            });
        })
    });
    assert_eq!(
        Config::new(path).load().unwrap().updates,
        tervia::updates::Settings {
            auto_check: true,
            interval_minutes: 15
        }
    );
}

#[gpui_test::test]
fn update_settings_show_save_errors_without_an_interface_reset_action(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences::default());
        let error = cx.new(|_| Some("Synthetic settings save failure".into()));
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let settings = cx
            .new(|cx| super::PreferencesWindow::new(preferences, None, error, updater, window, cx));
        gpui_kit::base::Root::new(settings, window, cx)
    });
    cx.simulate_resize(size(px(700.0), px(580.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let nav = cx.debug_bounds("settings-updates-nav").unwrap();
    cx.simulate_click(nav.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("settings-footer").is_some());
    assert!(cx.debug_bounds("settings-reset-defaults").is_none());
}

#[gpui_test::test]
fn escape_on_the_welcome_screen_cannot_interrupt_installation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.updater.update(cx, |updater, cx| {
                updater.state = super::super::updates::State::Installing;
                cx.notify();
            })
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.dispatch_action(gpui_kit::base::actions::Cancel);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).state.notice.clone()),
        Language::System.text("update_wait")
    );
}

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
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let entity = cx.new(|cx| {
            super::PreferencesWindow::new(
                preferences,
                Some("/synthetic/config.toml".into()),
                error,
                updater,
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
fn updates_navigation_exposes_check_status_and_respects_the_minimum_window(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut settings = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let preferences = cx.new(|_| Preferences {
            language: Language::Chinese,
            ..Preferences::default()
        });
        let error = cx.new(|_| None);
        let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
        let entity = cx
            .new(|cx| super::PreferencesWindow::new(preferences, None, error, updater, window, cx));
        settings = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(700.0), px(580.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let nav = cx
        .debug_bounds("settings-updates-nav")
        .expect("Updates navigation");
    cx.simulate_click(nav.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let check = cx
        .debug_bounds("updates-check")
        .expect("Check for updates button");
    assert!(check.right() <= px(676.0));
    assert!(check.bottom() <= px(580.0));
    assert!(cx.debug_bounds("updates-version").is_some());
    assert!(cx.debug_bounds("font-control").is_none());
    assert!(cx.debug_bounds("settings-footer").is_none());
    let version = cx.debug_bounds("updates-version").unwrap();
    assert!(check.left() >= version.right() + px(12.0));
    assert!(cx.update(|_, cx| settings.unwrap().read(cx).updater.read(cx).state.is_idle()));
}

#[gpui_test::test]
fn update_restart_blocks_dirty_files_in_inactive_projects_and_busy_writes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert!(this.update_restart_allowed());
            this.state.open_directory("/synthetic/project-a".into());
            this.state.open_directory("/synthetic/project-b".into());
            this.state.switch_project(0);
            this.state.projects[1].files.push(tervia::state::File {
                path: "draft.txt".into(),
                disk_path: "/synthetic/project-b/draft.txt".into(),
                language: "text".into(),
                saved: "Synthetic saved".into(),
                draft: "Synthetic draft".into(),
            });
            assert!(!this.update_restart_allowed());
            this.state.projects[1].files.clear();
            assert!(this.update_restart_allowed());
            this.git_mutating = true;
            assert!(!this.update_restart_allowed());
            this.git_mutating = false;
            this.saving_file = true;
            assert!(!this.update_restart_allowed());
        })
    });
}

#[gpui_test::test]
fn quitting_is_blocked_until_the_update_finishes_installing(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.updater.update(cx, |updater, _| {
                updater.state = super::super::updates::State::Installing
            });
            assert!(!this.try_quit(cx));
            assert!(!this.state.notice.is_empty());
            this.updater.update(cx, |updater, _| {
                updater.state = super::super::updates::State::Latest
            });
            assert!(this.try_quit(cx));
        })
    });
}

#[gpui_test::test]
fn background_update_notifications_preserve_an_existing_workspace_warning(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let target = tervia::updates::Target::current().unwrap();
    let release = self_update::Release::builder()
        .version("9.0.0")
        .asset(
            self_update::ReleaseAsset::new(
                target.asset_name("9.0.0"),
                "https://example.test/synthetic.zip",
            )
            .with_digest(format!("sha256:{}", "ab".repeat(32))),
        )
        .build()
        .unwrap();
    let candidate = tervia::updates::Candidate::from_release(release, "1.0.0", target)
        .unwrap()
        .unwrap();
    let view = view.unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.state.notice = "Synthetic existing warning".into();
            this.updater.update(cx, |updater, cx| {
                updater.state = super::super::updates::State::Available(candidate);
                cx.notify();
            });
        })
    });
    assert_eq!(
        cx.update(|_, cx| view.read(cx).state.notice.clone()),
        "Synthetic existing warning"
    );
}

#[gpui_test::test]
fn main_window_update_reports_install_errors_without_opening_settings(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("workspace-update").is_none());
    let target = tervia::updates::Target::current().unwrap();
    let release = self_update::Release::builder()
        .version("9.0.0")
        .asset(
            self_update::ReleaseAsset::new(
                target.asset_name("9.0.0"),
                "https://example.test/synthetic.zip",
            )
            .with_digest(format!("sha256:{}", "ab".repeat(32))),
        )
        .build()
        .unwrap();
    let candidate = tervia::updates::Candidate::from_release(release, "1.0.0", target)
        .unwrap()
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.preferences.update(cx, |preferences, cx| {
                preferences.language = Language::English;
                cx.notify();
            });
            this.updater.update(cx, |updater, cx| {
                updater.state = super::super::updates::State::Available(candidate);
                cx.notify();
            });
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let icon = cx
        .debug_bounds("workspace-update")
        .expect("An available update has a main-window icon");
    assert!(icon.right() <= cx.update(|window, _| window.viewport_size().width));
    cx.simulate_click(icon.center(), Modifiers::default());
    assert!(cx.has_pending_prompt());
    assert!(
        cx.pending_prompt()
            .unwrap()
            .1
            .contains(Language::English.text("update_confirm"))
    );
    assert!(cx.update(|_, cx| view.read(cx).preferences_window.is_none()));
    cx.simulate_prompt_answer(Language::English.text("git_cancel"));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.simulate_click(icon.center(), Modifiers::default());
    cx.simulate_prompt_answer(Language::English.text("update_install"));
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "Installation errors must be visible in the main window"
    );
    let (message, detail) = cx.pending_prompt().unwrap();
    assert_eq!(message, Language::English.text("update_error"));
    assert_eq!(
        detail,
        tervia::updates::Installation::discover().unwrap_err()
    );
    assert!(cx.update(|_, cx| view.read(cx).preferences_window.is_none()));
    assert!(cx.update(|_, cx| matches!(
        view.read(cx).updater.read(cx).state,
        super::super::updates::State::Failed {
            candidate: Some(_),
            ..
        }
    )));
    cx.simulate_prompt_answer(Language::English.text("close"));
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

#[gpui_test::test]
fn configured_startup_restores_open_projects_and_the_selected_directory(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let alpha = root.path().join("mock-alpha");
    let beta = root.path().join("mock-beta");
    fs::create_dir(&alpha).unwrap();
    fs::create_dir(&beta).unwrap();
    let alpha = alpha.canonicalize().unwrap();
    let beta = beta.canonicalize().unwrap();
    fs::write(
        root.path().join("workspace.json"),
        serde_json::json!({
            "version": 1,
            "projects": [
                { "root": alpha, "directory": alpha },
                { "root": beta, "directory": beta }
            ],
            "active": beta,
            "sidebar_expanded": true
        })
        .to_string(),
    )
    .unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            Tervia::configured(Ok(Config::new(root.path().join("config.toml"))), window, cx)
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| {
        let this = view.read(cx);
        assert_eq!(this.state.projects.len(), 2);
        assert_eq!(this.state.projects[0].root, alpha);
        assert_eq!(this.state.project().root, beta);
        assert!(this.state.sidebar_expanded);
        assert_eq!(this.git_states.len(), 2);
        assert_eq!(this.terminals.len(), 2);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).state.project().root, beta);
        assert!(view.read(cx).state.notice.is_empty());
    });
}
