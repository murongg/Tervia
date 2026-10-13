use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use std::sync::atomic::AtomicUsize;
use tervia::updates::Settings;

#[gpui_test::test]
fn automatic_checks_repeat_after_failures_and_apply_live_settings(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let preferences = cx.new(|_| Preferences {
        updates: Settings {
            auto_check: true,
            interval_minutes: 5,
        },
        ..Preferences::default()
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let updater = cx.new(|cx| {
        Updater::with_checker(
            preferences.clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Err("Synthetic offline response".into())
            }),
            cx,
        )
    });
    updater.update(cx, |updater, cx| updater.start_automatic(cx));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.executor().advance_clock(Duration::from_secs(299));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    preferences.update(cx, |preferences, cx| {
        preferences.updates.auto_check = false;
        cx.notify();
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(600));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    preferences.update(cx, |preferences, cx| {
        preferences.updates.auto_check = true;
        preferences.updates.interval_minutes = 10;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    cx.executor().advance_clock(Duration::from_secs(599));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
}

#[gpui_test::test]
fn manual_checks_reset_the_timer_and_disabling_auto_does_not_block_manual_checks(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let preferences = cx.new(|_| Preferences {
        updates: Settings {
            auto_check: true,
            interval_minutes: 5,
        },
        ..Preferences::default()
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let updater = cx.new(|cx| {
        Updater::with_checker(
            preferences.clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }),
            cx,
        )
    });
    updater.update(cx, |updater, cx| updater.start_automatic(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(120));
    updater.update(cx, |updater, cx| updater.check(cx));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    cx.executor().advance_clock(Duration::from_secs(299));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    preferences.update(cx, |preferences, cx| {
        preferences.updates.auto_check = false;
        cx.notify();
    });
    updater.update(cx, |updater, cx| updater.check(cx));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    cx.executor().advance_clock(Duration::from_secs(3600));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
}

#[gpui_test::test]
fn automatic_checks_can_start_disabled_and_enabling_uses_the_last_manual_check(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let preferences = cx.new(|_| Preferences {
        updates: updates::Settings {
            auto_check: false,
            interval_minutes: 5,
        },
        ..Preferences::default()
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let updater = cx.new(|cx| {
        Updater::with_checker(
            preferences.clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }),
            cx,
        )
    });
    updater.update(cx, |updater, cx| updater.start_automatic(cx));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    updater.update(cx, |updater, cx| updater.check(cx));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    preferences.update(cx, |preferences, cx| {
        preferences.updates.auto_check = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.executor().advance_clock(Duration::from_secs(300));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[gpui_test::test]
fn an_existing_update_survives_a_failed_periodic_check(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let preferences = cx.new(|_| Preferences::default());
    let target = updates::Target::current().unwrap();
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
    let candidate = Candidate::from_release(release, "1.0.0", target)
        .unwrap()
        .unwrap();
    let updater = cx.new(|cx| {
        Updater::with_checker(
            preferences,
            Arc::new(|| Err("Synthetic offline response".into())),
            cx,
        )
    });
    updater.update(cx, |updater, cx| {
        updater.state = State::Available(candidate);
        updater.check(cx);
    });
    assert_eq!(
        updater.read_with(cx, |updater, _| updater
            .state
            .candidate()
            .unwrap()
            .version()
            .to_owned()),
        "9.0.0"
    );
    cx.run_until_parked();
    assert_eq!(
        updater.read_with(cx, |updater, _| updater
            .state
            .candidate()
            .unwrap()
            .version()
            .to_owned()),
        "9.0.0"
    );
}

#[gpui_test::test]
fn update_options_toggle_automatic_checks_and_show_a_custom_interval(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let preferences = cx.new(|_| Preferences {
        updates: Settings {
            auto_check: false,
            interval_minutes: 37,
        },
        ..Preferences::default()
    });
    let displayed = preferences.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let updater = cx.new(|cx| Updater::with_checker(preferences, Arc::new(|| Ok(None)), cx));
        gpui_kit::base::Root::new(updater, window, cx)
    });
    cx.simulate_resize(size(px(480.0), px(560.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx
        .debug_bounds("updates-auto-toggle")
        .expect("Automatic-check switch");
    let interval = cx
        .debug_bounds("updates-interval-control")
        .expect("Interval control");
    assert!(toggle.bottom() <= interval.top());
    cx.simulate_click(toggle.center(), Modifiers::default());
    assert!(displayed.read_with(cx, |preferences, _| preferences.updates.auto_check));
    assert_eq!(
        displayed.read_with(cx, |preferences, _| preferences.updates.interval_minutes),
        37
    );
    cx.simulate_click(interval.center(), Modifiers::default());
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    assert_ne!(
        displayed.read_with(cx, |preferences, _| preferences.updates.interval_minutes),
        37
    );
}
