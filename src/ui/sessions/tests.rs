use crate::ui::{NewSession, Tervia};
use core::prelude::v1::test;
use gpui_kit::*;
use gpui_test::TestAppContext;
use tervia::state::{Screen, Tab};

fn window(cx: &mut TestAppContext) -> (Entity<Tervia>, &mut gpui_test::VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::console::init(cx);
        crate::ui::menus::bind_menu_keys(cx);
        cx.bind_keys([KeyBinding::new(
            tervia::platform::Platform::CURRENT.new_key(),
            NewSession,
            Some("Tervia"),
        )]);
    });
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    (view.unwrap(), cx)
}

#[gpui_test::test]
fn quitting_retains_connections_but_explicit_tab_close_forgets_them(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_local_terminal(window, cx);
            this.new_session(window, cx);
            this.state.local_terminal.sessions[0].connection = Some("a".repeat(32));
            this.state.local_terminal.sessions[1].connection = Some("b".repeat(32));
            this.shutdown_terminals(cx);
            assert_eq!(
                this.state.local_terminal.sessions[0].connection,
                Some("a".repeat(32))
            );
            this.close_terminal_session(1, window, cx);
            assert_eq!(this.state.local_terminal.sessions.len(), 1);
            assert_eq!(
                this.state.local_terminal.sessions[0].connection,
                Some("a".repeat(32))
            );
        });
    });
}

#[gpui_test::test]
fn welcome_new_terminal_opens_a_focused_local_shell_without_a_project(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.dispatch_action(Box::new(NewSession), cx);
        window.draw(cx).clear(cx);
    });
    view.update(cx, |this, cx| {
        assert_eq!(this.state.screen, Screen::Local);
        assert!(this.state.projects.is_empty());
        assert_eq!(this.local_terminals.len(), 1);
        assert!(this.local_terminals[0].is_some());
        assert!(this.git_states.is_empty());
        assert!(this.explorers.is_empty());
        assert_eq!(cx.windows().len(), 1);
    });
    assert!(cx.debug_bounds("local-terminal-entry").is_some());
    cx.update(|window, cx| {
        let console = &view.read(cx).local_terminals[0].as_ref().unwrap();
        assert!(console.read(cx).focus.is_focused(window));
    });
}

#[gpui_test::test]
fn local_and_project_consoles_are_independent_and_switching_keeps_entities(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_local_terminal(window, cx);
            let local = this.ensure_console(0, window, cx);
            let project = this
                .state
                .open_directory("/synthetic/mock-native-project".into());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Terminal);
            let owned = this.ensure_console(0, window, cx);
            assert_ne!(local, owned);
            this.open_local_terminal(window, cx);
            assert_eq!(this.ensure_console(0, window, cx), local);
            this.new_session(window, cx);
            assert_eq!(this.state.local_terminal.sessions.len(), 2);
            assert_eq!(this.terminals[project].len(), 1);
            this.state.switch_project(project);
            assert_eq!(this.ensure_console(0, window, cx), owned);
            this.open_local_terminal(window, cx);
            this.close_terminal_session(1, window, cx);
            this.close_terminal_session(0, window, cx);
            assert!(this.local_terminals.is_empty());
            assert!(this.state.local_terminal.sessions.is_empty());
            assert_eq!(this.state.screen, Screen::Local);
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("local-terminal-empty").is_some());
}

#[gpui_test::test]
fn project_actions_cannot_change_the_inactive_project_from_local_terminal(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory("/synthetic/mock-native-project".into());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this.open_local_terminal(window, cx);
            this.change_tab(Tab::Git, window, cx);
            this.toggle_terminal_dock(window, cx);
            assert_eq!(this.state.screen, Screen::Local);
            assert_eq!(this.state.projects[project].tab, Tab::Files);
            assert!(!this.state.projects[project].dock.visible);
            assert!(!this.navigation.open);
        });
    });
}

#[gpui_test::test]
fn a_finished_clone_cannot_steal_focus_after_entering_the_local_terminal(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let generation = this.navigation.generation();
            this.open_local_terminal(window, cx);
            let console = this.ensure_console(0, window, cx);
            this.finish_clone(
                "/synthetic/mock-cloned-project".into(),
                generation,
                window,
                cx,
            );
            assert_eq!(this.state.screen, Screen::Local);
            assert_eq!(this.ensure_console(0, window, cx), console);
            assert_eq!(this.state.project_rows().len(), 1);
            assert!(console.read(cx).focus.is_focused(window));
        });
    });
}

#[gpui_test::test]
fn local_shortcuts_work_from_welcome_console_and_the_empty_surface(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes(tervia::platform::Platform::CURRENT.new_key());
    cx.run_until_parked();
    view.update(cx, |this, _| assert_eq!(this.state.screen, Screen::Local));
    cx.simulate_keystrokes("secondary-shift-n");
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.local_terminal.sessions.len(), 2)
    });
    for _ in 0..2 {
        cx.simulate_keystrokes(
            if tervia::platform::Platform::CURRENT == tervia::platform::Platform::Mac {
                "cmd-w"
            } else {
                "ctrl-shift-w"
            },
        );
        cx.run_until_parked();
    }
    view.update(cx, |this, _| {
        assert!(this.state.local_terminal.sessions.is_empty())
    });
    cx.simulate_keystrokes(tervia::platform::Platform::CURRENT.new_key());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.local_terminal.sessions.len(), 1)
    });
}

#[gpui_test::test]
fn searching_an_empty_local_terminal_does_not_start_or_index_a_session(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_local_terminal(window, cx);
            this.close_terminal_session(0, window, cx);
        });
        window.draw(cx).clear(cx);
    });
    let search = cx.debug_bounds("terminal-search").unwrap();
    cx.simulate_click(search.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert!(this.local_terminals.is_empty());
        assert!(this.state.local_terminal.sessions.is_empty());
    });
}

#[gpui_test::test]
fn a_pending_folder_selection_cannot_override_a_later_local_terminal_choice(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let selected = temporary.path().to_path_buf();
    let (view, cx) = window(cx);
    cx.update(|window, cx| view.update(cx, |this, cx| this.choose_repository(window, cx)));
    cx.simulate_path_prompt_response(|_| Some(vec![selected]));
    cx.update(|window, cx| view.update(cx, |this, cx| this.open_local_terminal(window, cx)));
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.screen, Screen::Local);
        assert!(this.state.projects.is_empty());
    });
}

#[gpui_test::test]
fn a_pending_file_picker_cannot_open_an_invisible_editor_over_the_local_shell(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::write(temporary.path().join("mock.rs"), "synthetic source").unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let selected = root.join("mock.rs");
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root);
            this.init_project(project, window, cx);
            this.choose_file(window, cx);
        })
    });
    cx.simulate_path_prompt_response(|_| Some(vec![selected]));
    cx.update(|window, cx| view.update(cx, |this, cx| this.open_local_terminal(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.screen, Screen::Local);
            assert!(this.state.projects[0].open_files.is_empty());
            assert!(
                this.local_terminals[0]
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .focus
                    .is_focused(window)
            );
        })
    });
}

#[gpui_test::test]
fn finishing_a_file_close_does_not_take_focus_from_a_later_local_shell(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this
                .state
                .open_directory("/synthetic/mock-editor-project".into());
            this.state.projects[project]
                .files
                .push(tervia::state::File {
                    path: "mock.rs".into(),
                    disk_path: "mock.rs".into(),
                    language: "rust".into(),
                    saved: "synthetic original".into(),
                    draft: "synthetic draft".into(),
                });
            this.state.projects[project].open_file(0);
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this.close_active_file_tab(window, cx);
            this.open_local_terminal(window, cx);
        })
    });
    let discard = view.update(cx, |this, _| this.t("files_discard").to_owned());
    cx.simulate_prompt_answer(&discard);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.screen, Screen::Local);
            assert!(
                this.local_terminals[0]
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .focus
                    .is_focused(window)
            );
        })
    });
}

#[gpui_test::test]
fn closing_a_project_stops_only_its_owned_consoles(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_local_terminal(window, cx);
            let project = this
                .state
                .open_directory("/synthetic/mock-closing-project".into());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Terminal);
            this.ensure_console(0, window, cx);
            this.open_local_terminal(window, cx);
            this.request_project_close(project, window, cx);
        })
    });
    let original = view.update(cx, |this, _| {
        this.local_terminals[0].as_ref().unwrap().clone()
    });
    let confirm = view.update(cx, |this, _| this.t("git_confirm").to_owned());
    cx.simulate_prompt_answer(&confirm);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.screen, Screen::Local);
            assert_eq!(this.local_terminals[0].as_ref(), Some(&original));
            assert!(this.terminals[0][0].is_none());
            assert!(original.read(cx).focus.is_focused(window));
        })
    });
}
