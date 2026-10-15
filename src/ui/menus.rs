use gpui_kit::{App, KeyBinding, Menu, MenuItem};
use tervia::locale::Language;
use tervia::platform::Platform;

use super::{
    ClearTerminal, CloseFileTab, CopyCodeContext, CopyTerminalOutput, FilesTab, GitTab, NewSession,
    OpenFile, OpenPreferences, OpenProject, QuickOpenFile, Quit, ReopenFileTab, SaveFile,
    SearchProjectText, SplitTerminal, TerminalTab, TogglePalette, ToggleSidebar,
    ToggleTerminalDock, ToggleTheme,
};

pub(crate) fn bind_menu_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(
            if Platform::CURRENT == Platform::Mac {
                "cmd-j"
            } else {
                "ctrl-shift-j"
            },
            ToggleTerminalDock,
            Some("Tervia && !Terminal"),
        ),
        KeyBinding::new(
            if Platform::CURRENT == Platform::Mac {
                "cmd-j"
            } else {
                "ctrl-shift-j"
            },
            ToggleTerminalDock,
            Some("Tervia && !Terminal > Console"),
        ),
        KeyBinding::new(
            "secondary-shift-c",
            CopyCodeContext,
            Some("Tervia && !Terminal"),
        ),
        KeyBinding::new(
            if Platform::CURRENT == Platform::Mac {
                "secondary-o"
            } else {
                "secondary-shift-o"
            },
            OpenProject,
            Some("Tervia"),
        ),
        KeyBinding::new(
            Platform::CURRENT.clear_key(),
            ClearTerminal,
            // Binding predicates join the tokens exposed by key_context.
            Some("Tervia && Terminal"),
        ),
    ]);
}

pub(super) fn menus(language: Language) -> Vec<Menu> {
    vec![
        Menu::new("Tervia").items([
            MenuItem::action(format!("{}…", language.text("settings")), OpenPreferences),
            MenuItem::separator(),
            MenuItem::action(format!("{} Tervia", language.text("quit")), Quit),
        ]),
        Menu::new(language.text("menu_file")).items([
            MenuItem::action(language.text("git_open_repository"), OpenProject),
            MenuItem::action(language.text("git_open_file"), OpenFile),
            MenuItem::separator(),
            MenuItem::action(language.text("files_find_name"), QuickOpenFile),
            MenuItem::action(language.text("files_search_text"), SearchProjectText),
            MenuItem::separator(),
            MenuItem::action(language.text("save"), SaveFile),
            MenuItem::action(language.text("copy_code_context"), CopyCodeContext),
            MenuItem::action(language.text("files_close"), CloseFileTab),
            MenuItem::action(language.text("files_reopen"), ReopenFileTab),
        ]),
        Menu::new(language.text("menu_view")).items([
            MenuItem::action(language.text("terminal_dock"), ToggleTerminalDock),
            MenuItem::action(language.text("terminal"), TerminalTab),
            MenuItem::action("Git", GitTab),
            MenuItem::action(language.text("files"), FilesTab),
            MenuItem::separator(),
            MenuItem::action(language.text("toggle_projects"), ToggleSidebar),
            MenuItem::action(language.text("quick_actions"), TogglePalette),
            MenuItem::action(
                language.text("git_quick_switch"),
                super::git::QuickSwitchBranch,
            ),
            MenuItem::action(language.text("toggle_theme"), ToggleTheme),
        ]),
        Menu::new(language.text("terminal")).items([
            MenuItem::action(language.text("new_terminal"), NewSession),
            MenuItem::action(language.text("toggle_split"), SplitTerminal),
            MenuItem::separator(),
            MenuItem::action(language.text("clear_terminal"), ClearTerminal),
            MenuItem::action(language.text("copy_terminal"), CopyTerminalOutput),
        ]),
    ]
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use core::prelude::v1::test;
    use gpui_kit::*;
    use gpui_test::TestAppContext;
    use tervia::{locale::Language, state::Tab};

    use super::super::Tervia;

    #[gpui_test::test]
    fn terminal_dock_keeps_its_console_and_the_current_module(cx: &mut TestAppContext) {
        let (view, cx) = workspace(cx, Tab::Files);
        let original =
            cx.update(|window, cx| view.update(cx, |this, cx| this.ensure_console(0, window, cx)));
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.toggle_terminal_dock(window, cx));
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("terminal-dock").is_some());
        cx.update(|window, cx| window.dispatch_action(Box::new(super::super::NewSession), cx));
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx).state.project().tab,
                Tab::Files,
                "New sessions in a visible dock keep the document module"
            )
        });
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                assert_eq!(this.state.project().tab, Tab::Files);
                assert_eq!(this.ensure_console(0, window, cx), original);
                this.split_terminal(window, cx);
                assert_eq!(
                    this.state.project().tab,
                    Tab::Files,
                    "Splitting the dock must keep the document module"
                );
                this.change_tab(Tab::Git, window, cx);
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("terminal-dock").is_some());
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.toggle_terminal_dock(window, cx));
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("terminal-dock").is_none());
        cx.update(|_, cx| assert_eq!(view.read(cx).state.project().tab, Tab::Git));
    }

    fn menu_action(key: &str) -> Box<dyn Action> {
        super::super::menus(Language::English)
            .into_iter()
            .flat_map(|menu| menu.items)
            .find_map(|item| match item {
                MenuItem::Action { name, action, .. }
                    if name.as_ref() == Language::English.text(key) =>
                {
                    Some(action)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing menu action: {key}"))
    }

    fn workspace(
        cx: &mut TestAppContext,
        tab: Tab,
    ) -> (Entity<Tervia>, &mut gpui_test::VisualTestContext) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut view = Tervia::new(window, cx);
                let project = view.state.open_directory("/synthetic/menu-project".into());
                view.state.projects[project]
                    .files
                    .push(tervia::state::File {
                        path: "mock.txt".into(),
                        disk_path: "mock.txt".into(),
                        language: "plaintext".into(),
                        saved: "synthetic original".into(),
                        draft: "synthetic draft".into(),
                    });
                view.state.projects[project].open_file(0);
                view.init_project(project, window, cx);
                view.state.set_tab(tab);
                view.focus.focus(window, cx);
                view
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (view.unwrap(), cx)
    }

    #[test]
    fn native_menus_group_file_view_and_terminal_actions_in_each_language() {
        for language in Language::ALL {
            let menus = super::super::menus(language);
            let names: Vec<_> = menus.iter().map(|menu| menu.name.as_ref()).collect();
            assert_eq!(
                names,
                [
                    "Tervia",
                    language.text("menu_file"),
                    language.text("menu_view"),
                    language.text("terminal")
                ]
            );
        }
    }

    #[gpui_test::test]
    fn native_menu_new_terminal_switches_from_files_and_focuses_the_session(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = workspace(cx, Tab::Files);
        cx.update(|window, cx| window.dispatch_action(menu_action("new_terminal"), cx));
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.state.project().tab, Tab::Terminal);
            assert_eq!(view.state.project().sessions.len(), 2);
            assert!(
                view.terminals[0][1]
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .focus
                    .is_focused(window)
            );
        });
    }

    #[gpui_test::test]
    fn native_menu_split_opens_two_distinct_sessions_and_restores_one_pane(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = workspace(cx, Tab::Git);
        cx.update(|window, cx| window.dispatch_action(menu_action("toggle_split"), cx));
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(view.state.project().tab, Tab::Terminal);
            assert_eq!(view.state.project().sessions.len(), 2);
            assert!(view.state.project().split_terminal);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| window.dispatch_action(menu_action("toggle_split"), cx));
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(!view.state.project().split_terminal);
            assert_eq!(view.state.project().sessions.len(), 2);
        });
    }

    #[gpui_test::test]
    fn native_menu_project_actions_are_unavailable_on_welcome(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| Tervia::new(window, cx));
            gpui_kit::base::Root::new(entity, window, cx)
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let available = window.available_actions(cx);
            for key in ["new_terminal", "toggle_split", "clear_terminal", "save"] {
                let action = menu_action(key);
                assert!(
                    !available
                        .iter()
                        .any(|candidate| candidate.name() == action.name()),
                    "{key} needs an open project"
                );
            }
            let open = menu_action("git_open_repository");
            assert!(
                available
                    .iter()
                    .any(|candidate| candidate.name() == open.name())
            );
        });
    }

    #[gpui_test::test]
    fn native_menu_file_actions_match_module_and_busy_guards(cx: &mut TestAppContext) {
        let (view, cx) = workspace(cx, Tab::Files);
        let save = menu_action("save");
        let close = menu_action("files_close");
        for tab in [Tab::Git, Tab::Terminal, Tab::Files] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.change_tab(tab, window, cx));
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                let available = window.available_actions(cx);
                for action in [&save, &close] {
                    assert_eq!(
                        available
                            .iter()
                            .any(|candidate| candidate.name() == action.name()),
                        tab == Tab::Files
                    );
                }
            });
        }
        for (git, saving, closing, reload) in [
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, false, false, true),
        ] {
            view.update(cx, |view, cx| {
                view.git_mutating = git;
                view.saving_file = saving;
                view.file_closing = closing;
                view.file_reads = tervia::files::Reads::default();
                if reload {
                    view.file_reads.begin(
                        0,
                        "mock.txt".into(),
                        Some("synthetic draft".into()),
                        true,
                    );
                }
                cx.notify();
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                let available = window.available_actions(cx);
                assert!(
                    !available
                        .iter()
                        .any(|candidate| candidate.name() == save.name())
                );
                assert_eq!(
                    available
                        .iter()
                        .any(|candidate| candidate.name() == close.name()),
                    git
                );
            });
        }
    }

    #[gpui_test::test]
    fn native_menu_search_keeps_dirty_files_and_output_actions_follow_the_module(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut view = Tervia::new(window, cx);
                let project = view.state.open_directory("/synthetic/menu-search".into());
                view.state.projects[project]
                    .files
                    .push(tervia::state::File {
                        path: "mock.txt".into(),
                        disk_path: "mock.txt".into(),
                        language: "plaintext".into(),
                        saved: "synthetic original".into(),
                        draft: "synthetic draft".into(),
                    });
                view.state.projects[project].open_file(0);
                view.init_project(project, window, cx);
                view.state.set_tab(Tab::Git);
                view.focus.focus(window, cx);
                view
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| window.dispatch_action(menu_action("files_search_text"), cx));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let view = view.read(cx);
            assert_eq!(view.state.project().tab, Tab::Files);
            assert_eq!(
                view.state.project().current_file().unwrap().draft,
                "synthetic draft"
            );
            let available = window.available_actions(cx);
            for key in ["clear_terminal", "copy_terminal"] {
                let action = menu_action(key);
                assert!(
                    !available
                        .iter()
                        .any(|candidate| candidate.name() == action.name())
                );
            }
        });
    }
}
