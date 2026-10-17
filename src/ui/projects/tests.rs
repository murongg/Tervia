use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

fn project_window(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Entity<Tervia>,
    &mut gpui_test::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        // Match main(): console forwarding precedes Tervia's application shortcuts.
        super::super::console::init(cx);
        // Exercise the non-macOS console collision on macOS too.
        cx.bind_keys([KeyBinding::new(
            "secondary-alt-p",
            super::super::console::CopySelection,
            Some("Console"),
        )]);
    });
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    for name in ["mock-alpha", "mock-beta"] {
        std::fs::create_dir(root_path.join(name)).unwrap();
    }
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            for name in ["mock-alpha", "mock-beta"] {
                let index = view.state.open_directory(root_path.join(name));
                if index == 0 {
                    view.state.projects[index].files.push(tervia::state::File {
                        path: "mock.rs".into(),
                        disk_path: "mock.rs".into(),
                        language: "rust".into(),
                        saved: "synthetic base".into(),
                        draft: "synthetic draft".into(),
                    });
                    view.state.projects[index].open_file(0);
                }
                view.init_project(index, window, cx);
                view.state.projects[index].tab = tervia::state::Tab::Files;
            }
            view.state.switch_project(0);
            view.state.sidebar_expanded = true;
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (root, entity.unwrap(), cx)
}

#[gpui_test::test]
fn global_project_search_accepts_keyboard_selection_and_escape_restores_focus(
    cx: &mut TestAppContext,
) {
    let (_root, view, cx) = project_window(cx);
    cx.update(|window, cx| window.dispatch_action(Box::new(super::QuickSwitchProject), cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("project-switcher").is_some());
    cx.simulate_input("MKBT");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |view, _| assert_eq!(view.state.active_project, 1));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.show_project_switcher(false, window, cx));
        window.draw(cx).clear(cx);
    });
    cx.simulate_input("no-synthetic-match");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("project-switcher").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("project-switcher").is_none());
    view.update(cx, |view, cx| {
        assert_eq!(view.state.active_project, 1);
        assert_eq!(cx.windows().len(), 1);
    });
}

#[gpui_test::test]
fn favorite_controls_and_drag_reorder_keep_project_identity(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    let favorite = cx.debug_bounds("favorite-project-1").unwrap();
    cx.simulate_click(favorite.center(), Modifiers::default());
    view.update(cx, |view, _| {
        assert_eq!(view.state.project_rows(), vec![1, 0]);
        assert_eq!(view.state.active_project, 0, "Favoriting does not navigate");
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let favorite = cx.debug_bounds("favorite-project-1").unwrap();
    cx.simulate_click(favorite.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let from = cx.debug_bounds("project-row-0").unwrap().center();
    let to = cx.debug_bounds("project-row-1").unwrap().center();
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        from + point(px(0.0), px(10.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.project_rows(), vec![1, 0])
    });
}

#[gpui_test::test]
fn closing_confirms_owned_terminals_and_reopening_retains_drafts(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    let console = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let console = view.ensure_console(0, window, cx);
            view.request_project_close(0, window, cx);
            console
        })
    });
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_cancel"));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.project_rows(), vec![0, 1]);
        assert_eq!(view.terminals[0][0].as_ref(), Some(&console));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.request_project_close(0, window, cx)));
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_confirm"));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.project_rows(), vec![1]);
        assert!(view.terminals[0][0].is_none());
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.reopen_closed_project(0, window, cx)));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 0);
        assert_eq!(
            view.state.project().current_file().unwrap().draft,
            "synthetic draft"
        );
    });
}

#[gpui_test::test]
fn failed_close_persistence_keeps_the_project_and_console_open(cx: &mut TestAppContext) {
    let (root, view, cx) = project_window(cx);
    let path = root.path().join("workspace.json");
    let console = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut store = tervia::workspace::Store::new(path.clone());
            store.load().unwrap();
            std::fs::write(&path, "synthetic external update").unwrap();
            view.workspace = Some(store);
            let console = view.ensure_console(0, window, cx);
            view.request_project_close(0, window, cx);
            console
        })
    });
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_confirm"));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.project_rows(), vec![0, 1]);
        assert_eq!(view.terminals[0][0].as_ref(), Some(&console));
        assert!(!view.state.notice.is_empty());
        assert_eq!(
            view.state.projects[0].current_file().unwrap().draft,
            "synthetic draft"
        );
    });
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "synthetic external update"
    );
}

#[gpui_test::test]
fn reopening_cannot_override_later_navigation_and_busy_operations_block_close(
    cx: &mut TestAppContext,
) {
    let (_root, view, cx) = project_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git_mutating = true;
            view.request_project_close(0, window, cx);
            assert_eq!(view.state.project_rows(), vec![0, 1]);
            view.git_mutating = false;
            view.request_project_close(0, window, cx);
            view.reopen_closed_project(0, window, cx);
            view.change_project(1, window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 1);
        assert!(view.state.project_closed(0));
    });
}

#[gpui_test::test]
fn project_switch_shortcut_works_from_a_console_and_escape_restores_that_console(
    cx: &mut TestAppContext,
) {
    let (_root, view, cx) = project_window(cx);
    let console = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.change_tab(tervia::state::Tab::Terminal, window, cx);
            let console = view.ensure_console(0, window, cx);
            console.read(cx).focus.clone().focus(window, cx);
            console
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-alt-p"
    } else {
        "ctrl-alt-p"
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("project-switcher").is_some());
    cx.simulate_input("synthetic unmatched query");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(console.read(cx).focus.is_focused(window));
    });
    assert!(cx.debug_bounds("project-switcher").is_none());
}

#[gpui_test::test]
fn later_navigation_dismisses_a_project_search(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_project_switcher(false, window, cx);
            view.change_project(1, window, cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("project-switcher").is_none());
    view.update(cx, |view, _| assert_eq!(view.state.active_project, 1));
}

#[gpui_test::test]
fn all_closed_projects_show_the_welcome_screen_and_can_be_reopened(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.request_project_close(0, window, cx);
            view.request_project_close(1, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("welcome-recent-projects").is_some());
    let recent = cx.debug_bounds("welcome-recent-projects").unwrap();
    cx.simulate_click(recent.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |view, _| assert_eq!(view.state.project_rows().len(), 1));
}

#[gpui_test::test]
fn the_titlebar_switches_working_directories_while_the_project_rail_restores_the_last_one(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    std::fs::create_dir(directory.join("mock-child")).unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory(directory.clone());
            view.init_project(parent, window, cx);
            let child = view.state.register_directory(directory.join("mock-child"));
            view.init_project(child, window, cx);
            view.state.attach_directory(parent, child);
            view.state.projects[parent].tab = tervia::state::Tab::Files;
            view.state.projects[child].tab = tervia::state::Tab::Files;
            let other = view
                .state
                .register_directory(other.path().canonicalize().unwrap());
            view.init_project(other, window, cx);
            view.state.projects[other].tab = tervia::state::Tab::Files;
            view.state.switch_project(child);
            view.state.project_mut().terminal.sessions[0].name = "synthetic child session".into();
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let trigger = cx
        .debug_bounds("working-directory-switcher")
        .expect("Directory switching belongs in the titlebar");
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let parent = cx.debug_bounds("working-directory-0").unwrap();
    cx.simulate_click(parent.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 0);
        assert_eq!(view.state.project_rows(), vec![0, 2]);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let child = cx.debug_bounds("working-directory-1").unwrap();
    cx.simulate_click(child.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let other = cx.debug_bounds("project-root-2").unwrap();
    cx.simulate_click(other.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(view.state.active_project, 2);
        view.state.sidebar_expanded = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let project = cx.debug_bounds("project-root-0").unwrap();
    cx.simulate_click(project.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 1);
        assert_eq!(
            view.state.project().terminal.sessions[0].name,
            "synthetic child session"
        );
    });
}

#[gpui_test::test]
fn directory_search_supports_keyboard_selection_empty_results_and_escape(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys([KeyBinding::new(
            "escape",
            gpui_kit::base::actions::Cancel,
            Some("Tervia"),
        )]);
    });
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    std::fs::create_dir_all(directory.join("mock-apps/mock-web")).unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory(directory.clone());
            view.init_project(parent, window, cx);
            let child = view
                .state
                .register_directory(directory.join("mock-apps/mock-web"));
            view.init_project(child, window, cx);
            view.state.attach_directory(parent, child);
            view.state.projects[parent].tab = tervia::state::Tab::Files;
            view.state.projects[child].tab = tervia::state::Tab::Files;
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_directory_picker(true, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes("down");
    view.update(cx, |view, _| {
        assert_eq!(
            view.navigation.selected, 1,
            "Down moves the picker selection"
        )
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 1);
        assert!(!view.navigation.open);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_directory_picker(true, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    cx.simulate_input("no-match");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.update(cx, |view, cx| {
        assert!(view.working_directory_rows(0, cx).is_empty())
    });
    cx.simulate_keystrokes("down enter");
    view.update(cx, |view, _| {
        assert!(
            view.navigation.open,
            "Enter with no results keeps the picker open"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.update(cx, |_, cx| {
        assert_eq!(cx.windows().len(), 1, "Escape dismisses only the picker")
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.navigation.open);
            assert_eq!(view.state.active_project, 1);
            assert!(
                view.focus.is_focused(window),
                "Closing the picker restores workspace focus"
            );
            view.toggle_directory_picker(true, window, cx);
            view.navigation
                .input
                .update(cx, |input, cx| input.set_value("MOCK-WEB", window, cx));
        });
        window.draw(cx).clear(cx);
    });
    view.update(cx, |view, cx| {
        assert_eq!(view.working_directory_rows(0, cx), vec![1])
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |view, _| assert_eq!(view.state.active_project, 1));
}

#[gpui_test::test]
fn adding_a_directory_is_scoped_to_the_project_and_later_navigation_wins(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    std::fs::create_dir(directory.join("mock-child")).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Tervia::new(window, cx);
            let parent = view.state.open_directory(directory.clone());
            view.init_project(parent, window, cx);
            view.state.set_tab(tervia::state::Tab::Files);
            view.choose_working_directory(parent, window, cx);
            view
        });
        entity = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    let view = entity.unwrap();
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files);
        Some(vec![outside.path().to_path_buf()])
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 0);
        assert!(!view.state.notice.is_empty());
        assert_eq!(view.state.working_directories(0), vec![0]);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.choose_working_directory(0, window, cx))
    });
    cx.simulate_path_prompt_response(|_| Some(vec![directory.join("mock-child")]));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.project().root, directory.join("mock-child"));
        assert_eq!(view.state.working_directories(0), vec![0, 1]);
        assert_eq!(view.state.project_rows(), vec![0]);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.activate_directory(0, PathBuf::new(), window, cx);
            view.change_tab(tervia::state::Tab::Files, window, cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, _| assert_eq!(view.state.active_project, 1));
}
