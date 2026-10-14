use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

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
            view.state.project_mut().sessions[0].name = "synthetic child session".into();
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
            view.state.project().sessions[0].name,
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
