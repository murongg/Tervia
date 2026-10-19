use super::*;
use core::prelude::v1::test;
use gpui_test::{TestAppContext, VisualTestContext};
use std::fs;

fn mount<'a>(
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
    fs::write(root.path().join("target.ts"), "export function mock() {}\n").unwrap();
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
                    path: "target.ts".into(),
                    disk_path: "target.ts".into(),
                    language: "typescript".into(),
                    saved: "export function mock() {}\n".into(),
                    draft: "\nexport function mock() {}\n".into(),
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
