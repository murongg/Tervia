use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

fn mount(cx: &mut TestAppContext) -> (Entity<Tervia>, &mut gpui_test::VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::console::init(cx);
        crate::ui::menus::bind_menu_keys(cx);
    });
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        entity.update(cx, |this, cx| this.open_local_terminal(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view.unwrap(), cx)
}

fn double_click(cx: &mut gpui_test::VisualTestContext, selector: &'static str) {
    let position = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
    });
    cx.run_until_parked();
}

#[gpui_test::test]
fn double_click_renames_a_terminal_and_keeps_custom_names_over_agent_titles(
    cx: &mut TestAppContext,
) {
    let (view, cx) = mount(cx);
    double_click(cx, "session-title-0");
    assert!(cx.debug_bounds("terminal-rename-input").is_some());
    cx.simulate_input("mock frontend");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(
                this.state.terminals().sessions[0].custom_name.as_deref(),
                Some("mock frontend")
            );
            let console = this.local_terminals[0].as_ref().unwrap().clone();
            console.update(cx, |console, _| console.agent = Some(Agent::Codex));
            assert_eq!(this.terminal_session_name(0, cx), "mock frontend");
            assert!(console.read(cx).focus.is_focused(window));
            assert!(this.terminal_rename.is_none());
        })
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    double_click(cx, "session-title-0");
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.terminals().sessions[0].custom_name, None);
        assert_eq!(this.terminal_session_name(0, cx), "Codex");
    });
}

#[gpui_test::test]
fn context_menu_renames_the_clicked_tab_and_escape_cancels(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    cx.update(|window, cx| view.update(cx, |this, cx| this.new_session(window, cx)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let position = cx.debug_bounds("session-tab-0").unwrap().center();
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("terminal-rename-input").is_some());
    cx.simulate_input("mock selected");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(
            this.state.terminals().sessions[0].custom_name.as_deref(),
            Some("mock selected")
        );
        assert_eq!(this.state.terminals().sessions[1].custom_name, None);
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    double_click(cx, "session-title-0");
    cx.simulate_input("mock cancelled");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(
            this.state.terminals().sessions[0].custom_name.as_deref(),
            Some("mock selected")
        );
        assert!(this.terminal_rename.is_none());
    });
}

#[gpui_test::test]
fn switching_owner_or_closing_a_tab_cancels_pending_terminal_renames(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.new_session(window, cx);
            this.rename_terminal_session(1, window, cx);
            this.close_terminal_session(0, window, cx);
            assert!(this.terminal_rename.is_none());
            assert_eq!(this.state.terminals().sessions[0].custom_name, None);
            this.rename_terminal_session(0, window, cx);
            this.open_welcome(window, cx);
            assert!(this.terminal_rename.is_none());
        })
    });
}

#[gpui_test::test]
fn project_dock_renames_persist_without_changing_the_document_module(cx: &mut TestAppContext) {
    use tervia::{state::Tab, workspace::Store};
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("mock-project");
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let path = temporary.path().join("workspace.json");
    let (view, cx) = mount(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root.clone());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Git);
            this.state.project_mut().dock.visible = true;
            let mut store = Store::new(path.clone());
            store.load().unwrap();
            this.workspace = Some(store);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    double_click(cx, "session-title-0");
    cx.simulate_input("mock backend");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let restored = Store::new(path).load().unwrap();
    assert_eq!(restored.project().tab, Tab::Git);
    assert!(restored.project().dock.visible);
    assert_eq!(
        restored.terminals().sessions[0].custom_name.as_deref(),
        Some("mock backend")
    );
    assert_eq!(restored.local_terminal.sessions[0].custom_name, None);

    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.rename_terminal_session(0, window, cx);
            this.change_tab(Tab::Files, window, cx);
            assert!(this.terminal_rename.is_none());
            this.rename_terminal_session(0, window, cx);
            let other = this
                .state
                .open_directory("/synthetic/mock-other-project".into());
            this.init_project(other, window, cx);
            this.change_project(other, window, cx);
            assert!(this.terminal_rename.is_none());
            assert_eq!(this.state.terminals().sessions[0].custom_name, None);
        })
    });
}
