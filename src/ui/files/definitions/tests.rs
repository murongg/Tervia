use super::*;
use core::prelude::v1::test;
use gpui_test::{TestAppContext, VisualTestContext};
use std::fs;

pub(in crate::ui::files) fn mount<'a>(
    cx: &'a mut TestAppContext,
    mode: &str,
    target_open: bool,
) -> (tempfile::TempDir, Entity<Tervia>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::menus::bind_menu_keys(cx);
    });
    let root = tempfile::tempdir().unwrap();
    let source = if mode == "samefile" {
        "mock()\nfunction mock() {}\n"
    } else {
        "mock()\n"
    };
    fs::write(root.path().join("mock.ts"), source).unwrap();
    let (target_name, target_language, target_saved, target_draft) = if mode == "json" {
        let json = "              {\"mock\": \"synthetic\"}\n";
        ("target.json", "json", json, json)
    } else {
        (
            "target.ts",
            "typescript",
            "export function mock() {}\n",
            "\nexport function mock() {}\n",
        )
    };
    fs::write(root.path().join(target_name), target_saved).unwrap();
    let server = Server {
        program: if cfg!(windows) { "python" } else { "python3" }.into(),
        args: vec![
            "-u".into(),
            format!("{}/tests/fixtures/lsp.py", env!("CARGO_MANIFEST_DIR")),
            mode.into(),
        ],
    };
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        entity.update(cx, |this, cx| {
            let owner = this.state.open_directory(root.path().into());
            this.state.projects[owner].files.push(File {
                path: "mock.ts".into(),
                disk_path: "mock.ts".into(),
                language: "typescript".into(),
                saved: source.into(),
                draft: source.into(),
            });
            this.state.projects[owner].open_file(0);
            if target_open {
                this.state.projects[owner].files.push(File {
                    path: target_name.into(),
                    disk_path: target_name.into(),
                    language: target_language.into(),
                    saved: target_saved.into(),
                    draft: target_draft.into(),
                });
                this.state.projects[owner].open_file(1);
                this.state.projects[owner].active_file = 0;
            }
            this.init_project(owner, window, cx);
            this.state.set_tab(Tab::Files);
            Definitions::attach(
                cx.weak_entity(),
                owner,
                0,
                server,
                &this.editors[owner][0],
                cx,
            );
            this.editors[owner][0].update(cx, |editor, cx| editor.focus(window, cx));
        });
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (root, view.unwrap(), cx)
}

fn modifier() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    } else {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }
}

fn source_point(view: &Entity<Tervia>, cx: &mut VisualTestContext) -> Point<Pixels> {
    cx.update(|_, cx| {
        let editor = view.read(cx).editors[0][0].read(cx);
        let text = editor.value();
        let offset = text.find("mock").unwrap();
        let bounds = editor.range_to_bounds(&(offset..offset + 4)).unwrap();
        point(bounds.left() + px(8.0), bounds.center().y)
    })
}

fn source_is_underlined(view: &Entity<Tervia>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        let bounds = view.read(cx).editors[0][0]
            .read(cx)
            .range_to_bounds(&(0..4))
            .unwrap()
            .scale(window.scale_factor());
        window.painted_underlines().iter().any(|line| {
            line.wavy == false.into() && line.color.a > 0.0 && line.bounds.intersects(&bounds)
        })
    })
}

#[gpui_test::test]
fn definitions_into_open_json_targets_keep_their_hover_links(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "json", true);
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(20));
    cx.run_until_parked();
    assert!(source_is_underlined(&view, cx));
    cx.simulate_click(position, modifier());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1)
    });
}

#[gpui_test::test]
fn dirty_json_targets_do_not_expose_positions_from_disk_as_hover_links(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "json", true);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][1].update(cx, |editor, cx| {
                editor.set_value("{\"edited\": true}\n", window, cx)
            });
        })
    });
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert!(!source_is_underlined(&view, cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn definition_links_appear_before_the_preview_delay(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(20));
    cx.run_until_parked();
    assert!(
        source_is_underlined(&view, cx),
        "A confirmed definition should underline without waiting for its preview"
    );
    assert!(cx.debug_bounds("definition-preview").is_none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(180));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_some());
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_eq!(log["definition"], 1);
}

#[gpui_test::test]
fn target_edits_clear_a_definition_link_before_its_preview_is_ready(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(20));
    cx.run_until_parked();
    assert!(source_is_underlined(&view, cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][1].update(cx, |editor, cx| {
                editor.set_value("export function mock() {} // synthetic edit\n", window, cx);
            });
        });
    });
    cx.run_until_parked();
    assert!(!source_is_underlined(&view, cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn moving_within_a_symbol_does_not_postpone_its_definition_link(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    for delta in [1.0, 2.0, 3.0, 4.0] {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        cx.simulate_mouse_move(position + point(px(delta), px(0.0)), None, modifier());
        cx.run_until_parked();
    }
    assert!(
        source_is_underlined(&view, cx),
        "Small pointer movements within one symbol must not restart the hover delay"
    );
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_eq!(log["definition"], 1);
}

#[gpui_test::test]
fn pressing_modifier_over_a_stationary_symbol_underlines_it(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.run_until_parked();
    assert!(!source_is_underlined(&view, cx));
    cx.simulate_modifiers_change(modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert!(
        source_is_underlined(&view, cx),
        "Pressing Cmd/Ctrl must update the symbol already under the pointer"
    );
}

#[gpui_test::test]
fn stationary_definition_hover_works_without_editor_focus_and_clears_on_release(
    cx: &mut TestAppContext,
) {
    let (_root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.update(|window, cx| view.update(cx, |this, cx| this.focus.focus(window, cx)));
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.simulate_modifiers_change(modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert!(source_is_underlined(&view, cx));
    cx.update(|window, cx| view.update(cx, |this, _| assert!(this.focus.is_focused(window))));
    cx.simulate_modifiers_change(Modifiers::default());
    cx.run_until_parked();
    assert!(!source_is_underlined(&view, cx));
}

#[gpui_test::test]
fn symbols_without_definitions_do_not_get_a_modifier_underline(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "empty", true);
    let position = source_point(&view, cx);
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.simulate_modifiers_change(modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert!(!source_is_underlined(&view, cx));
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 0)
    });
}

#[gpui_test::test]
fn definition_links_paint_underlines_in_both_themes_and_clear_on_release(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    for light in [false, true] {
        if light {
            cx.update(|window, cx| view.update(cx, |this, cx| this.toggle_theme(window, cx)));
            cx.run_until_parked();
        }
        let position = source_point(&view, cx);
        cx.simulate_mouse_move(position, None, modifier());
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        cx.run_until_parked();
        assert!(
            source_is_underlined(&view, cx),
            "light={light}: a confirmed definition must paint an underline"
        );
        cx.simulate_modifiers_change(Modifiers::default());
        cx.run_until_parked();
        assert!(!source_is_underlined(&view, cx));
    }
}

fn show_preview(view: &Entity<Tervia>, cx: &mut VisualTestContext) -> Point<Pixels> {
    let position = source_point(view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("definition-preview").is_some(),
        "Preview missing: {}",
        view.update(cx, |this, cx| {
            let editor = this.editors[0][0].read(cx);
            format!(
                "point={position:?}, input={:?}, scroll={:?}, notice={}, log={:?}",
                editor.input_bounds(),
                editor.scroll_offset(),
                this.state.notice,
                fs::read_to_string(this.state.project().root.join("mock-log.json"))
            )
        })
    );
    position
}

#[gpui_test::test]
fn preview_dismissal_allows_reopening_the_same_symbol(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    let position = show_preview(&view, cx);
    cx.simulate_modifiers_change(Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_some());
}

#[gpui_test::test]
fn preview_escape_dismisses_before_the_workspace_quit_fallback(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    show_preview(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.dismiss(window, cx);
            assert!(
                !this.dismiss_definition_preview(cx),
                "Escape must already consume the preview"
            );
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn preview_source_scroll_and_target_draft_changes_hide_the_overlay(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][0].update(cx, |editor, cx| {
                editor.set_value(
                    format!("mock()\n{}", "// synthetic\n".repeat(120)),
                    window,
                    cx,
                );
                editor.set_cursor_position(lsp_types::Position::new(0, 0), window, cx);
            });
        });
        window.draw(cx).clear(cx);
    });
    let position = show_preview(&view, cx);
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.0), px(-32.0))),
        modifiers: modifier(),
        ..Default::default()
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][0].update(cx, |editor, cx| {
                editor.set_scroll_offset(point(px(0.0), px(0.0)), cx);
            })
        });
        window.draw(cx).clear(cx);
    });
    show_preview(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][1].update(cx, |editor, cx| {
                editor.set_value("// changed target draft\n", window, cx);
            })
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn quick_modifier_click_starts_one_jump_without_waiting_for_hover(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_click(position, modifier());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1)
    });
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_eq!(log["definition"], 1);
}

#[gpui_test::test]
fn preview_open_remains_clickable_and_reuses_the_lookup(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    show_preview(&view, cx);
    let open = cx.debug_bounds("definition-preview-open").unwrap();
    cx.simulate_mouse_move(open.center(), None, modifier());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview-open").is_some());
    cx.simulate_click(open.center(), modifier());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1)
    });
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_eq!(log["definition"], 1);
}

#[gpui_test::test]
fn pending_modifier_release_cancels_before_service_io(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.simulate_modifiers_change(Modifiers::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert!(!root.path().join("mock-log.json").exists());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn changed_pending_targets_require_a_fresh_lookup_before_click_navigation(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    let position = source_point(&view, cx);
    cx.simulate_modifiers_change(modifier());
    cx.simulate_mouse_move(position, None, modifier());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][1].update(cx, |editor, cx| {
                editor.set_value(
                    "// synthetic changed target\nexport function mock() {}\n",
                    window,
                    cx,
                );
            })
        });
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
    cx.simulate_click(position, modifier());
    cx.run_until_parked();
    let log: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("mock-log.json")).unwrap())
            .unwrap();
    assert_eq!(
        log["definition"], 2,
        "A stale draft position must not stay cached as a link"
    );
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1)
    });
}

#[gpui_test::test]
fn explicit_f12_works_while_the_modifier_is_held_away_from_the_symbol(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_modifiers_change(modifier());
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::GoToDefinition), cx)
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 1)
    });
}

#[gpui_test::test]
fn unsupported_file_actions_do_not_consume_the_next_supported_hover(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    let hooks = view.update(cx, |this, cx| {
        this.state.projects[0].files[0].language = "plaintext".into();
        this.editors[0][0].update(cx, |editor, _| {
            (
                editor.lsp_mut().definition_provider.take(),
                editor.lsp_mut().show_document.take(),
            )
        })
    });
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::GoToDefinition), cx)
    });
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        this.state.projects[0].files[0].language = "typescript".into();
        this.editors[0][0].update(cx, |editor, _| {
            editor.lsp_mut().definition_provider = hooks.0;
            editor.lsp_mut().show_document = hooks.1;
        });
    });
    show_preview(&view, cx);
}

#[gpui_test::test]
fn preview_near_the_bottom_keeps_the_source_symbol_uncovered(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_resize(size(px(1000.0), px(600.0)));
    let text = format!("{}mock()\n", "// synthetic\n".repeat(12));
    let offset = text.find("mock").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][0].update(cx, |editor, cx| {
                editor.set_value(text, window, cx);
                editor.set_cursor_position(lsp_types::Position::new(12, 1), window, cx);
            });
            let target = format!(
                "\nexport function mock() {{}}\n{}",
                "// synthetic target\n".repeat(12)
            );
            this.state.projects[0].files[1].draft = target.clone();
            this.editors[0][1].update(cx, |editor, cx| editor.set_value(target, window, cx));
        });
        window.draw(cx).clear(cx);
    });
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][0].update(cx, |editor, cx| {
                editor.set_cursor_position(lsp_types::Position::new(12, 1), window, cx);
            })
        });
        window.draw(cx).clear(cx);
    });
    show_preview(&view, cx);
    let trigger = view.update(cx, |this, cx| {
        this.editors[0][0]
            .read(cx)
            .range_to_bounds(&(offset..offset + 4))
            .unwrap()
    });
    let overlay = cx.debug_bounds("definition-preview").unwrap();
    assert!(
        overlay.bottom() <= trigger.top() || overlay.top() >= trigger.bottom(),
        "Overlay {overlay:?} must leave source {trigger:?} clickable"
    );
}

#[gpui_test::test]
fn f12_uses_lsp_draft_positions_and_preserves_the_open_target(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 1);
        assert_eq!(
            this.state.project().files[1].draft,
            "\nexport function mock() {}\n"
        );
        assert!(this.state.project().files[1].dirty());
        let position = this.editors[0][1].read(cx).cursor_position();
        assert_eq!(position.line, 1);
        assert_eq!(position.character, 16);
    });
}

#[gpui_test::test]
fn definition_jumps_support_back_and_forward_without_losing_drafts(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.editors[0][1].update(cx, |editor, cx| {
                editor.set_cursor_position(lsp_types::Position::new(1, 8), window, cx);
            });
        });
        window.draw(cx).clear(cx);
    });
    let back = cx
        .debug_bounds("file-back")
        .expect("Back is available after a definition jump");
    cx.simulate_click(back.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 0);
        assert_eq!(
            this.editors[0][0].read(cx).cursor_position(),
            lsp_types::Position::new(0, 0)
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let forward = cx.debug_bounds("file-forward").unwrap();
    cx.simulate_click(forward.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 1);
        assert_eq!(
            this.editors[0][1].read(cx).cursor_position(),
            lsp_types::Position::new(1, 8)
        );
        assert_eq!(
            this.editors[0][1].read(cx).value().as_str(),
            "\nexport function mock() {}\n"
        );
        assert!(this.state.project().files[1].dirty());
    });
}

#[cfg(unix)]
#[gpui_test::test]
fn canonical_definition_targets_reuse_an_open_symlink_draft(cx: &mut TestAppContext) {
    let (root, view, cx) = mount(cx, "normal", true);
    std::os::unix::fs::symlink("target.ts", root.path().join("alias.ts")).unwrap();
    view.update(cx, |this, _| {
        let file = &mut this.state.projects[0].files[1];
        file.path = "alias.ts".into();
        file.disk_path = "alias.ts".into();
    });
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 1);
        assert_eq!(this.state.project().open_files.len(), 2);
        assert_eq!(
            this.editors[0][1].read(cx).value().as_str(),
            "\nexport function mock() {}\n"
        );
    });
}

#[gpui_test::test]
fn f12_opens_cross_file_locations_and_reports_empty_and_failed_results(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "location", false);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(
            this.state.project().current_file().unwrap().disk_path,
            PathBuf::from("target.ts")
        );
        assert_eq!(this.editors[0][1].read(cx).cursor_position().character, 16);
        assert_eq!(this.state.project().preview_file, None);
    });
}

#[gpui_test::test]
fn late_definition_results_do_not_override_later_module_navigation(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::GoToDefinition), cx);
        view.update(cx, |this, cx| this.change_tab(Tab::Git, window, cx));
    });
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().tab, Tab::Git);
        assert_eq!(this.state.project().active_file, 0);
        assert_ne!(this.state.notice, "definition_searching");
    });
}

#[gpui_test::test]
fn definition_errors_are_visible_and_do_not_move_the_caret(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "empty", false);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.notice, "definition_not_found");
        assert_eq!(this.state.project().active_file, 0);
        assert_eq!(this.editors[0][0].read(cx).cursor_position().character, 0);
    });
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            Definitions::attach(
                cx.weak_entity(),
                0,
                0,
                Server {
                    program: "synthetic-missing-language-server".into(),
                    args: vec![],
                },
                &this.editors[0][0],
                cx,
            );
            this.editors[0][0].update(cx, |editor, cx| editor.focus(window, cx));
        });
    });
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert!(
            this.state
                .notice
                .contains("synthetic-missing-language-server")
        );
        assert_eq!(this.state.project().active_file, 0);
    });
}

#[gpui_test::test]
fn modifier_click_follows_a_definition_without_a_prior_keyboard_request(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", false);
    let position = cx.update(|_, cx| {
        let cursor = view.read(cx).editors[0][0]
            .read(cx)
            .cursor_layout()
            .unwrap()
            .0;
        point(cursor.left() + px(8.0), cursor.center().y)
    });
    let modifiers = if tervia::platform::Platform::CURRENT == tervia::platform::Platform::Mac {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    } else {
        Modifiers {
            control: true,
            ..Default::default()
        }
    };
    cx.simulate_mouse_move(position, None, modifiers);
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(position, modifiers);
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(
            this.state.project().current_file().unwrap().disk_path,
            PathBuf::from("target.ts")
        );
    });
}

#[gpui_test::test]
fn modifier_hover_previews_the_definition_without_opening_it(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    let position = cx.update(|_, cx| {
        let bounds = view.read(cx).editors[0][0]
            .read(cx)
            .cursor_layout()
            .unwrap()
            .0;
        point(bounds.left() + px(8.0), bounds.center().y)
    });
    let modifiers = if tervia::platform::Platform::CURRENT == tervia::platform::Platform::Mac {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    } else {
        Modifiers {
            control: true,
            ..Default::default()
        }
    };
    cx.simulate_modifiers_change(modifiers);
    cx.simulate_mouse_move(position, None, modifiers);
    cx.run_until_parked();
    assert!(cx.debug_bounds("definition-preview").is_none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_some());
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 0);
        assert_eq!(this.state.project().open_files.len(), 2);
    });
    cx.simulate_modifiers_change(Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn dismissing_a_pending_hover_prevents_a_late_preview(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", false);
    let position = cx.update(|_, cx| {
        let bounds = view.read(cx).editors[0][0]
            .read(cx)
            .cursor_layout()
            .unwrap()
            .0;
        point(bounds.left() + px(8.0), bounds.center().y)
    });
    let modifiers = if cfg!(target_os = "macos") {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    } else {
        Modifiers {
            control: true,
            ..Default::default()
        }
    };
    cx.simulate_mouse_move(position, None, modifiers);
    cx.run_until_parked();
    view.update(cx, |this, cx| assert!(this.dismiss_definition_preview(cx)));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-preview").is_none());
}

#[gpui_test::test]
fn source_edits_discard_pending_definition_results(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "normal", true);
    cx.update(|window, cx| {
        let provider = view.read(cx).editors[0][0]
            .read(cx)
            .lsp()
            .definition_provider
            .clone()
            .unwrap();
        let task = provider.definitions(&Rope::from("mock()\n"), 0, window, cx);
        view.update(cx, |this, cx| {
            this.editors[0][0].update(cx, |editor, cx| {
                editor.set_value("synthetic edit", window, cx)
            });
        });
        task.detach();
    });
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 0);
        assert_eq!(
            this.editors[0][0].read(cx).value().as_str(),
            "synthetic edit"
        );
        assert!(this.state.notice.is_empty());
    });
}

#[gpui_test::test]
fn same_file_definitions_move_without_reentering_the_editor_update(cx: &mut TestAppContext) {
    let (_root, view, cx) = mount(cx, "samefile", false);
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    view.update(cx, |this, cx| {
        assert_eq!(this.state.project().active_file, 0);
        let editor = this.editors[0][0].read(cx);
        assert_eq!(
            editor.cursor_position(),
            gpui_kit::component::input::Position::new(1, 9)
        );
        assert_eq!(editor.value().as_str(), "mock()\nfunction mock() {}\n");
    });
}

#[gpui_test::test]
fn a_missing_runtime_has_a_download_entry_and_retry_without_installing_a_toolchain(
    cx: &mut TestAppContext,
) {
    let (root, view, cx) = mount(cx, "normal", false);
    let cache = root.path().join("synthetic-services");
    let before = cx.update(|_, cx| {
        view.read(cx).editors[0][0]
            .read(cx)
            .cursor_layout()
            .unwrap()
            .0
    });
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.explorers[0].definitions = tervia::lsp::Client::with_tools(std::sync::Arc::new(
                tervia::lsp::tools::Tools::with_paths(cache.clone(), Vec::new()),
            ));
            let editor = this.editors[0][0].clone();
            this.install_definitions(0, 0, &editor, cx);
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        });
    });
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("definition-setup").is_some());
    assert!(cx.debug_bounds("definition-runtime").is_some());
    let after = cx.update(|_, cx| {
        view.read(cx).editors[0][0]
            .read(cx)
            .cursor_layout()
            .unwrap()
            .0
    });
    assert_eq!(
        before.top(),
        after.top(),
        "Preparation must keep the hovered code in place"
    );
    let retry = cx
        .debug_bounds("definition-retry")
        .expect("A runtime installed outside Tervia can be detected on retry");
    cx.simulate_click(retry.center(), Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |this, _| {
        assert_eq!(this.state.project().active_file, 0);
        assert!(this.state.notice.is_empty());
    });
    assert!(!cache.exists());
}
