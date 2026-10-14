use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use std::fs;

#[gpui_test::test]
fn escape_leaves_file_search_before_closing_the_workspace(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        entity.update(cx, |this, cx| {
            let project = this.state.open_directory(root.path().into());
            this.init_project(project, window, cx);
            this.show_file_search(super::search::Mode::Files, window, cx);
            this.dismiss(window, cx);
            assert!(this.explorers[project].finder.mode == super::search::Mode::Browse);
        });
        gpui_kit::base::Root::new(entity, window, cx)
    });
    cx.run_until_parked();
}

#[gpui_test::test]
fn file_search_opens_unicode_matches_at_their_source_position_and_keeps_project_results_separate(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("mock.rs"),
        "// Synthetic\n// 中文 synthetic needle\n",
    )
    .unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root.path().into());
            this.init_project(project, window, cx);
            this.show_file_search(super::search::Mode::Text, window, cx);
            this.explorers[project]
                .finder
                .input
                .update(cx, |input, cx| {
                    input.set_value("needle", window, cx);
                    cx.emit(InputEvent::Change);
                });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let hit = cx
        .debug_bounds("files-search-result-0")
        .expect("Text search exposes navigable results");
    cx.simulate_click(hit.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            let position = this.editors[0][0].read(cx).cursor_position();
            assert_eq!(position.line, 1);
            assert_eq!(position.character, 16);
            assert_eq!(
                this.state.project().preview_file,
                None,
                "A precise search jump opens a pinned file"
            );
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let second = this
                .state
                .open_directory("/synthetic/second-project".into());
            this.init_project(second, window, cx);
            assert!(this.explorers[second].finder.rows.is_empty());
            this.state.switch_project(0);
            this.show_file_search(super::search::Mode::Files, window, cx);
            this.explorers[0].finder.input.update(cx, |input, cx| {
                input.set_value("mkrs", window, cx);
                cx.emit(InputEvent::Change);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.explorers[0].finder.rows.len(), 1)
        })
    });
}

#[gpui_test::test]
fn browsing_replaces_the_preview_and_editing_pins_its_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("first.rs"), "// Synthetic first\n").unwrap();
    fs::write(root.path().join("second.rs"), "// Synthetic second\n").unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root.path().into());
            this.init_project(project, window, cx);
            this.open_preview_file(project, "first.rs".into(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_preview_file(0, "second.rs".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.project().open_files.len(), 1);
            let index = this.state.project().active_file;
            assert_eq!(this.state.project().preview_file, Some(index));
            this.editors[0][index].update(cx, |editor, cx| {
                editor.set_value("// Synthetic draft", window, cx);
                cx.emit(InputEvent::Change);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.state.project().preview_file, None);
            this.open_preview_file(0, "first.rs".into(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            assert_eq!(this.state.project().open_files.len(), 2);
            assert!(
                this.state.projects[0]
                    .files
                    .iter()
                    .any(|file| file.draft == "// Synthetic draft")
            );
        })
    });
}

#[gpui_test::test]
fn markdown_and_images_have_native_previews_without_writing_image_bytes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("mock.md"),
        "# Synthetic heading\n\n**Synthetic bold**\n",
    )
    .unwrap();
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\"><rect width=\"32\" height=\"32\" fill=\"#abcdef\"/></svg>";
    fs::write(root.path().join("mock.svg"), svg).unwrap();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory(root.path().into());
            this.init_project(project, window, cx);
            this.open_preview_file(project, "mock.md".into(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("files-markdown-preview").is_some());
    let find = cx.debug_bounds("find-in-file").unwrap();
    cx.simulate_click(find.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("files-markdown-preview").is_none(),
        "Finding in a Markdown file exposes the source search control"
    );
    let read = cx.debug_bounds("files-source-toggle").unwrap();
    cx.simulate_click(read.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx.debug_bounds("files-source-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("files-markdown-preview").is_none());
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_preview_file(0, "mock.svg".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("files-image-preview").is_some());
    cx.update(|window, cx| view.update(cx, |this, cx| this.save_real_file(window, cx)));
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(root.path().join("mock.svg")).unwrap(),
        svg
    );
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_preview_file(0, "mock.md".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            let image = this.state.projects[0]
                .files
                .iter()
                .position(|file| file.path == "mock.svg")
                .unwrap();
            assert!(
                this.file_preview(0, image).is_none(),
                "Replacing an image preview releases its retained image bytes"
            );
        })
    });
}
