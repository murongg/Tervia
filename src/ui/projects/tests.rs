use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;

#[gpui_test::test]
fn sidebar_long_project_lists_scroll_without_covering_global_tools(cx: &mut TestAppContext) {
    let (root, view, cx) = project_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for index in 2..32 {
                let directory = root.path().join(format!("mock-project-{index}"));
                std::fs::create_dir(&directory).unwrap();
                let project = view.state.open_directory(directory);
                view.init_project(project, window, cx);
                view.state.projects[project].tab = tervia::state::Tab::Files;
            }
            view.state.switch_project(0);
            cx.notify();
        });
    });
    cx.simulate_resize(size(px(880.0), px(560.0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let panel = cx.debug_bounds("project-navigation").unwrap();
    let tools = cx.debug_bounds("open-welcome").unwrap();
    assert!(
        panel.bottom() <= tools.top(),
        "The project panel stays above global tools"
    );
    cx.simulate_event(ScrollWheelEvent {
        position: panel.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-1600.0))),
        ..Default::default()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let last = cx
        .debug_bounds("project-row-31")
        .expect("The last project remains reachable by scrolling");
    assert!(last.center().y < panel.bottom());
    assert_eq!(cx.debug_bounds("open-welcome").unwrap(), tools);
}

#[gpui_test::test]
fn sidebar_expansion_is_a_compact_project_panel_and_keeps_global_controls_in_the_rail(
    cx: &mut TestAppContext,
) {
    let (_root, _view, cx) = project_window(cx);
    let local = cx.debug_bounds("local-terminal-entry").unwrap();
    assert_eq!(
        local.size.width,
        px(39.0),
        "Global controls stay in their original narrow rail"
    );
    let panel = cx.debug_bounds("project-navigation").unwrap();
    assert!(panel.top() > local.bottom());
    assert!(
        panel.size.height < px(160.0),
        "A short project list must not become a full-height sheet"
    );
    assert!(cx.debug_bounds("project-row-0").unwrap().size.width > px(200.0));
}

#[gpui_test::test]
fn sidebar_navigation_clears_temporary_expansion_but_keeps_pinning(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    for pinned in [false, true] {
        view.update(cx, |view, cx| {
            view.state.sidebar_expanded = pinned;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let row = cx.debug_bounds("project-row-0").unwrap();
        cx.simulate_mouse_move(row.center(), None, Modifiers::default());
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("project-row-0").unwrap().size.width,
            px(216.0)
        );
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_welcome(window, cx)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_move(point(px(500.0), px(400.0)), None, Modifiers::default());
        cx.update(|window, cx| view.update(cx, |view, cx| view.change_project(0, window, cx)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("project-row-0").unwrap().size.width,
            px(if pinned { 216.0 } else { 39.0 }),
            "Returning to a workspace restores only an explicitly pinned panel"
        );
        view.update(cx, |view, _| {
            assert_eq!(view.state.sidebar_expanded, pinned)
        });
    }
}

#[gpui_test::test]
fn sidebar_hover_stays_expanded_while_reordering_projects(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    view.update(cx, |view, cx| {
        view.state.sidebar_expanded = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds("project-row-0").unwrap();
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let from = cx.debug_bounds("project-row-0").unwrap();
    let to = cx.debug_bounds("project-row-1").unwrap();
    cx.simulate_mouse_down(from.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        from.center() + point(px(0.0), px(10.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_move(to.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        from.size.width,
        "Dragging must not collapse the drop target"
    );
    cx.simulate_mouse_up(to.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.project_rows(), vec![1, 0])
    });
    cx.simulate_mouse_move(point(px(500.0), to.center().y), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        row.size.width
    );
}

#[gpui_test::test]
fn sidebar_overlay_keeps_terminal_bounds_and_escape_only_dismisses_the_peek(
    cx: &mut TestAppContext,
) {
    let (_root, view, cx) = project_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.change_tab(tervia::state::Tab::Terminal, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    let terminal = cx.debug_bounds("terminal-scrollbar").unwrap();
    view.update(cx, |view, cx| {
        view.state.sidebar_expanded = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("terminal-scrollbar").unwrap(),
        terminal,
        "Pinning or collapsing the sidebar must not resize the terminal"
    );
    let row = cx.debug_bounds("project-row-0").unwrap();
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        px(216.0)
    );
    assert_eq!(cx.debug_bounds("terminal-scrollbar").unwrap(), terminal);
    cx.dispatch_action(gpui_kit::base::actions::Cancel);
    cx.run_until_parked();
    assert_eq!(
        cx.windows().len(),
        1,
        "Escape closes the transient sidebar without closing the workspace"
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        px(39.0)
    );
    assert_eq!(cx.debug_bounds("terminal-scrollbar").unwrap(), terminal);
}

#[gpui_test::test]
fn sidebar_hover_immediately_overlays_content_and_closes_after_leaving(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    view.update(cx, |view, cx| {
        view.state.sidebar_expanded = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let collapsed = cx.debug_bounds("project-row-0").unwrap();
    let content = cx.debug_bounds("find-in-file").unwrap();
    assert_eq!(collapsed.size.width, px(39.0));

    cx.simulate_mouse_move(collapsed.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let expanded = cx.debug_bounds("project-row-0").unwrap();
    assert_eq!(
        expanded.size.width,
        px(216.0),
        "Hover reveals all project names without advancing the clock"
    );
    assert_eq!(
        cx.debug_bounds("find-in-file").unwrap(),
        content,
        "The content never moves or resizes"
    );
    view.update(cx, |view, _| {
        assert!(
            !view.state.sidebar_expanded,
            "Hover is never persisted as pinned"
        )
    });

    let other = cx.debug_bounds("project-row-1").unwrap();
    let target = point(other.right() - px(36.0), other.center().y);
    cx.simulate_mouse_move(target, None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap(),
        expanded,
        "The whole overlay remains hoverable"
    );
    cx.simulate_click(target, Modifiers::default());
    view.update(cx, |view, _| {
        assert_eq!(
            view.state.active_project, 1,
            "Overlay clicks reach the project, not the covered content"
        )
    });

    cx.simulate_mouse_move(point(px(500.0), target.y), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        collapsed.size.width
    );
}

#[gpui_test::test]
fn sidebar_immediate_hover_cancels_pending_close_on_reentry(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    view.update(cx, |view, cx| {
        view.state.sidebar_expanded = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds("project-row-0").unwrap();
    let outside = point(px(500.0), row.center().y);
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let expanded = cx.debug_bounds("project-row-0").unwrap();
    assert_eq!(expanded.size.width, px(216.0));
    cx.simulate_mouse_move(outside, None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.simulate_mouse_move(expanded.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap(),
        expanded,
        "Reentry cancels an earlier close timer"
    );
}

#[gpui_test::test]
fn sidebar_pin_and_keyboard_toggle_keep_content_bounds_and_persist_only_pinning(
    cx: &mut TestAppContext,
) {
    let (root, view, cx) = project_window(cx);
    let path = root.path().join("workspace.json");
    view.update(cx, |view, cx| {
        let mut store = tervia::workspace::Store::new(path.clone());
        store.load().unwrap();
        view.workspace = Some(store);
        view.state.sidebar_expanded = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let content = cx.debug_bounds("find-in-file").unwrap();
    let row = cx.debug_bounds("project-row-0").unwrap();
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let pin = cx
        .debug_bounds("pin-project-sidebar")
        .expect("The expanded sidebar exposes a pin control");
    let search = cx.debug_bounds("project-search").unwrap();
    assert!(
        search.right() <= pin.left(),
        "Search and pin controls have separate click targets"
    );
    cx.simulate_click(pin.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.state.sidebar_expanded);
        assert!(
            !view.navigation.switcher.open,
            "Pinning must not invoke project search underneath the control"
        );
    });
    assert!(
        tervia::workspace::Store::new(path.clone())
            .load()
            .unwrap()
            .sidebar_expanded
    );
    cx.simulate_mouse_move(point(px(500.0), row.center().y), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        px(216.0)
    );
    assert_eq!(cx.debug_bounds("find-in-file").unwrap(), content);

    cx.dispatch_action(crate::ui::ToggleSidebar);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.update(cx, |view, _| assert!(!view.state.sidebar_expanded));
    assert_eq!(cx.debug_bounds("project-row-0").unwrap(), row);
    assert!(
        !tervia::workspace::Store::new(path)
            .load()
            .unwrap()
            .sidebar_expanded
    );
    assert_eq!(cx.debug_bounds("find-in-file").unwrap(), content);

    cx.dispatch_action(crate::ui::ToggleSidebar);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("project-row-0").unwrap().size.width,
        px(216.0)
    );
    assert_eq!(cx.debug_bounds("find-in-file").unwrap(), content);
}

#[gpui_test::test]
fn closing_and_reopening_a_task_project_does_not_restore_a_task_without_its_host(
    cx: &mut TestAppContext,
) {
    let (root, view, cx) = project_window(cx);
    let path = root.path().join("workspace.json");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let session = &mut view.state.projects[0].terminal.sessions[0];
            session.name = "mock:test".into();
            session.task = Some(tervia::tasks::Task {
                label: "mock:test".into(),
                source: "package.json".into(),
                program: "npm".into(),
                args: vec!["run".into(), "mock:test".into()],
            });
            session.connection = Some("a".repeat(32));
            let mut store = tervia::workspace::Store::new(path.clone());
            store.load().unwrap();
            store.save(&view.state).unwrap();
            view.workspace = Some(store);
            view.ensure_console(0, window, cx);
            view.request_project_close(0, window, cx);
        })
    });
    cx.simulate_prompt_answer(tervia::locale::Language::System.text("git_confirm"));
    cx.run_until_parked();
    view.update(cx, |view, _| assert!(view.state.project_closed(0)));
    let saved = tervia::workspace::Store::new(path).load().unwrap();
    assert!(saved.projects[0].terminal.sessions[0].task.is_none());
    assert!(saved.projects[0].terminal.sessions[0].connection.is_none());
    cx.update(|window, cx| view.update(cx, |view, cx| view.reopen_closed_project(0, window, cx)));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert_eq!(view.state.active_project, 0);
        assert!(view.state.project().terminal.sessions[0].task.is_none());
        assert_eq!(view.state.project().terminal.sessions[0].name, "shell");
        assert_eq!(
            view.state.project().current_file().unwrap().draft,
            "synthetic draft"
        );
    });
}

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
fn escape_keeps_idle_workspace_tabs_open(cx: &mut TestAppContext) {
    use tervia::state::Tab;

    let (_root, view, cx) = project_window(cx);
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "escape",
            gpui_kit::base::actions::Cancel,
            Some("Tervia"),
        )]);
    });
    for tab in [Tab::Git, Tab::Files, Tab::Terminal] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.change_tab(tab, window, cx);
                view.focus.focus(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        for _ in 0..2 {
            cx.simulate_keystrokes("escape");
            assert_eq!(cx.windows().len(), 1, "Escape keeps the workspace open");
        }
        cx.dispatch_action(gpui_kit::component::input::Escape);
        assert_eq!(cx.windows().len(), 1);
        view.update(cx, |view, _| {
            assert_eq!(view.state.active_tab(), tab);
            assert_eq!(view.state.active_project, 0);
            assert_eq!(
                view.state.project().current_file().unwrap().draft,
                "synthetic draft"
            );
        });
    }
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
    cx.dispatch_action(gpui_kit::base::actions::Cancel);
    assert_eq!(
        cx.windows().len(),
        1,
        "Escape after dismissing the picker keeps the workspace open"
    );
}

#[gpui_test::test]
fn project_favorite_shares_the_row_hover_surface(cx: &mut TestAppContext) {
    let (_root, view, cx) = project_window(cx);
    for light in [false, true] {
        for selected in [false, true] {
            view.update(cx, |this, cx| {
                this.appearance.theme = light.into();
                super::super::theme::apply_palette(light, cx);
                this.state.switch_project(usize::from(selected));
                cx.notify();
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                window.draw(cx).clear(cx);
            });
            let bounds = cx.debug_bounds("project-row-1").unwrap();
            let favorite = cx.debug_bounds("favorite-project-1").unwrap();
            let content = point(bounds.left() + px(20.0), bounds.center().y);
            let p = view.read_with(cx, |this, _| this.p());
            for (position, hovered) in [
                (content, true),
                (favorite.center(), true),
                (point(bounds.right() - px(1.0), bounds.center().y), true),
                (point(px(0.0), px(0.0)), false),
            ] {
                cx.simulate_mouse_move(position, None, Modifiers::default());
                cx.update(|window, cx| {
                    window.draw(cx).clear(cx);
                    window.draw(cx).clear(cx);
                });
                let expected = if hovered {
                    p.text.opacity(0.12)
                } else if selected {
                    p.selected
                } else {
                    p.sidebar.opacity(0.0)
                };
                cx.update(|window, _| {
                    let scale = window.scale_factor();
                    let bounds = bounds.scale(scale);
                    let sample = content.scale(scale);
                    let surfaces: Vec<_> = window.painted_quads().into_iter().filter(|quad| {
                        !quad.background.is_transparent()
                            && quad.bounds.contains(&sample)
                            && quad.content_mask.bounds.contains(&sample)
                            && quad.bounds.top() >= bounds.top()
                            && quad.bounds.bottom() <= bounds.bottom()
                    }).collect();
                    assert_eq!(surfaces.len(), usize::from(!expected.is_transparent()), "Project hover spans its favorite control: light={light}, selected={selected}, hovered={hovered}, pointer={position:?}");
                    if let Some(surface) = surfaces.first() {
                        assert_eq!(surface.bounds, bounds);
                        assert_eq!(surface.background, expected.into());
                    }
                });
            }
        }
    }
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
fn directory_picker_hides_linked_worktrees_but_keeps_normal_directories(cx: &mut TestAppContext) {
    let (root, view, cx) = project_window(cx);
    let directory = root.path().canonicalize().unwrap();
    let main = directory.join("mock-alpha");
    let linked = main.join(".worktree/mock-feature");
    let manual = main.join(".worktree/mock-src");
    std::fs::create_dir_all(&linked).unwrap();
    std::fs::create_dir(&manual).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let parent = view.state.register_directory(directory.clone());
            view.init_project(parent, window, cx);
            let worktree = view.state.register_directory(linked.clone());
            view.init_project(worktree, window, cx);
            let child = view.state.register_directory(manual.clone());
            view.init_project(child, window, cx);
            let identity = main.join(".git");
            for index in [0, worktree, child] {
                view.state
                    .associate_repository(index, identity.clone(), Some(main.clone()));
            }
            view.state.group_folder(parent, &[0, 1, worktree]);
            view.state.attach_directory(parent, worktree);
            view.state.attach_directory(parent, child);
            view.state.switch_project(worktree);
            view.git_states[0].snapshot = Some(std::sync::Arc::new(tervia::git::Snapshot {
                worktrees: vec![
                    tervia::git::Worktree {
                        path: main.clone(),
                        main: true,
                        ..Default::default()
                    },
                    tervia::git::Worktree {
                        path: linked.clone(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }));
            view.toggle_directory_picker(true, window, cx);
            assert_eq!(
                view.working_directory_rows(parent, cx),
                vec![parent, 0, 1, child]
            );
            assert_eq!(view.navigation.selected, 0);
            assert_eq!(view.state.active_project, worktree);
            view.navigation.input.update(cx, |input, cx| {
                input.set_value("mock-feature", window, cx);
            });
            assert!(view.working_directory_rows(parent, cx).is_empty());
            view.navigation.input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            assert_eq!(view.working_directory_rows(worktree, cx), vec![worktree]);
        });
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
