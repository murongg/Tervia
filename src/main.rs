#![cfg_attr(windows, windows_subsystem = "windows")]

extern crate gpui_kit as gpui;

mod ui;

use gpui_kit::base::actions::Cancel;
use gpui_kit::*;
use tervia::{config::Config, platform::Platform};
use ui::{
    CloseFileTab, FilesTab, GitTab, NewSession, NextFileTab, OpenPreferences, PreviousFileTab,
    QuickOpenFile, Quit, ReopenFileTab, SaveFile, SearchProjectText, TerminalTab, Tervia,
    TogglePalette, ToggleSidebar,
};

fn main() {
    if std::env::args_os()
        .nth(1)
        .as_deref()
        .is_some_and(tervia::sessions::is_host_argument)
    {
        if let Err(error) = tervia::sessions::run_from_args() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    application().with_assets(assets::AllAssets).run(|cx| {
        gpui_kit::init(cx);
        ui::console::init(cx);
        ui::bind_menu_keys(cx);
        cx.bind_keys([
            KeyBinding::new(Platform::CURRENT.quit_key(), Quit, None),
            KeyBinding::new(
                Platform::CURRENT.sidebar_key(),
                ToggleSidebar,
                Some("Tervia"),
            ),
            KeyBinding::new("secondary-1", TerminalTab, Some("Tervia")),
            KeyBinding::new("secondary-2", GitTab, Some("Tervia")),
            KeyBinding::new("secondary-3", FilesTab, Some("Tervia")),
            KeyBinding::new("secondary-s", SaveFile, Some("Tervia")),
            KeyBinding::new(
                "secondary-p",
                QuickOpenFile,
                Some(if Platform::CURRENT == Platform::Mac {
                    "Tervia"
                } else {
                    "FileTabs"
                }),
            ),
            KeyBinding::new("secondary-shift-f", SearchProjectText, Some("Tervia")),
            KeyBinding::new("secondary-w", CloseFileTab, Some("FileTabs")),
            KeyBinding::new("ctrl-tab", NextFileTab, Some("FileTabs")),
            KeyBinding::new("ctrl-shift-tab", PreviousFileTab, Some("FileTabs")),
            KeyBinding::new("secondary-shift-t", ReopenFileTab, Some("FileTabs")),
            KeyBinding::new("secondary-shift-p", TogglePalette, Some("Tervia")),
            KeyBinding::new(Platform::CURRENT.new_key(), NewSession, Some("Tervia")),
            KeyBinding::new("secondary-,", OpenPreferences, Some("Tervia")),
            KeyBinding::new("escape", Cancel, Some("Tervia")),
            KeyBinding::new("escape", Cancel, Some("TerviaSettings")),
        ]);
        let bounds = Bounds::centered(None, size(px(1220.0), px(790.0)), cx);
        let (workspace_window, view) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(ui::window_title("Tervia")),
                    appears_transparent: true,
                    traffic_light_position: (Platform::CURRENT == Platform::Mac)
                        .then(|| point(px(16.0), px(18.0))),
                }),
                window_min_size: Some(size(px(880.0), px(560.0))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Tervia::configured(Config::discover(), window, cx)),
        )
        .expect("Unable to open Tervia's native window");
        let guarded_view = view.clone();
        workspace_window
            .update(cx, |_, window, cx| {
                window.on_window_should_close(cx, move |_, cx| {
                    guarded_view.update(cx, |view, cx| view.try_quit(cx))
                });
            })
            .expect("Unable to register the update close guard");
        // The application has one workspace: do not leave an orphaned Settings-only app.
        let closing_view = view.clone();
        cx.on_window_closed(move |cx, closed| {
            if closed == workspace_window.window_id() {
                closing_view.update(cx, |view, cx| view.shutdown_terminals(cx));
                cx.quit();
            }
        })
        .detach();
        let quitting_view = view.clone();
        cx.on_action(move |_: &Quit, cx| {
            if !quitting_view.update(cx, |view, cx| view.try_quit(cx)) {
                return;
            }
            quitting_view.update(cx, |view, cx| view.shutdown_terminals(cx));
            cx.quit();
        });
        cx.activate(true);
        if std::env::args().any(|argument| argument == "--settings") {
            view.update(cx, |view, cx| view.open_preferences(cx));
        }
    });
}
