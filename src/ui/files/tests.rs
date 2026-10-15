use super::*;
use core::prelude::v1::test;
use gpui_test::TestAppContext;
use std::fs;

struct FilesSurface(Entity<Tervia>);

impl Render for FilesSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "files-test-surface".into())
            .w(px(800.0))
            .h(px(500.0))
            .flex()
            .flex_col()
            .child(self.0.update(cx, |this, cx| this.files(window, cx)))
    }
}

fn divider_surface(cx: &mut TestAppContext) -> (Entity<Tervia>, &mut gpui_test::VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut this = Tervia::new(window, cx);
            let project = this.state.open_directory("/synthetic/dividers".into());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            this
        });
        view = Some(entity.clone());
        let surface = cx.new(|_| FilesSurface(entity));
        gpui_kit::base::Root::new(surface, window, cx)
    });
    (view.unwrap(), cx)
}

#[gpui_test::test]
fn opening_files_uses_their_grammar_and_matches_git_diff(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
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
        });
    });
    for (path, source, expected) in [
        ("mock.py", "count = 42\n", "python"),
        ("mock.go", "package mock\n", "go"),
        ("mock.js", "const count = 42;\n", "javascript"),
        ("mock.jsx", "const mock = <p>Mock</p>;\n", "javascript"),
        ("mock.tsx", "const mock = <p>Mock</p>;\n", "tsx"),
        ("mock.JSON", "{\"mock\": 42}\n", "json"),
        ("mock.yaml", "mock: 42\n", "yaml"),
        ("mock.html", "<p>Mock</p>\n", "html"),
        ("mock.css", "p { color: red; }\n", "css"),
        ("Makefile", "MOCK = value\n", "make"),
        ("Cargo.lock", "version = 4\n", "toml"),
        ("mock.unknown", "Synthetic text\n", "text"),
    ] {
        fs::write(root.path().join(path), source).unwrap();
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.open_preview_file(0, path.into(), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                let file = this.state.project().active_file;
                assert_eq!(
                    this.state.project().files[file].language,
                    expected,
                    "{path}"
                );
                assert_eq!(
                    this.editors[0][file].read(cx).language_name().as_str(),
                    expected,
                    "{path}"
                );
                assert_eq!(
                    gpui_kit::component::diff::DiffFile::unchanged(path, source)
                        .language()
                        .as_str(),
                    expected,
                    "{path} must use the same grammar in Files and Git"
                );
            });
        });
    }
}

#[gpui_test::test]
fn files_top_divider_spans_the_explorer_and_empty_editor(cx: &mut TestAppContext) {
    let (view, cx) = divider_surface(cx);
    for light in [false, true] {
        view.update(cx, |this, cx| {
            this.appearance.light_theme = light;
            super::super::theme::apply_palette(light, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let surface = cx.debug_bounds("files-test-surface").unwrap();
        cx.update(|window, _| {
            let scaled = surface.scale(window.scale_factor());
            let color = super::super::theme::Palette::workbench(light).border;
            assert!(
                window.painted_quads().iter().any(|quad| {
                    quad.bounds.origin == scaled.origin
                        && quad.bounds.size.width == scaled.size.width
                        && quad.border_widths.top == px(1.0).scale(window.scale_factor())
                        && quad.border_color == color
                }),
                "Files must have the same full-width top hairline as the Git workbench"
            );
        });
    }
}

#[gpui_test::test]
fn files_split_divider_is_painted_once_in_browse_and_search(cx: &mut TestAppContext) {
    let (view, cx) = divider_surface(cx);
    for light in [false, true] {
        for mode in [
            search::Mode::Browse,
            search::Mode::Files,
            search::Mode::Text,
        ] {
            view.update(cx, |this, cx| {
                this.appearance.light_theme = light;
                this.explorers[0].finder.mode = mode;
                super::super::theme::apply_palette(light, cx);
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let surface = cx.debug_bounds("files-test-surface").unwrap();
            cx.update(|window, _| {
                let scale = window.scale_factor();
                let height = surface.size.height.scale(scale);
                let quads = window.painted_quads();
                let seams = quads
                    .iter()
                    .filter(|quad| {
                        quad.bounds.size.height >= height - px(1.0).scale(scale)
                            && quad.bounds.size.height <= height
                            && ((quad.border_widths.right > px(0.0).scale(scale)
                                && !quad.border_color.is_transparent())
                                || (quad.bounds.size.width == px(1.0).scale(scale)
                                    && !quad.background.is_transparent()))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    seams.len(),
                    1,
                    "The draggable divider must be the only full-height vertical rule: {:?}",
                    seams
                        .iter()
                        .map(|quad| (&quad.bounds, &quad.border_widths))
                        .collect::<Vec<_>>()
                );
            });
        }
    }
}

#[gpui_test::test]
fn files_split_divider_keeps_its_drag_target(cx: &mut TestAppContext) {
    let (_, cx) = divider_surface(cx);
    let divider = |cx: &mut gpui_test::VisualTestContext| {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            window
                .painted_quads()
                .iter()
                .find(|quad| {
                    quad.bounds.size.width == px(1.0).scale(scale)
                        && quad.bounds.size.height > px(400.0).scale(scale)
                        && !quad.background.is_transparent()
                })
                .unwrap()
                .bounds
                .map(|value| px(value.as_f32() / scale))
        })
    };
    let before = divider(cx);
    let destination = before.center() + point(px(40.0), px(0.0));
    cx.simulate_mouse_down(before.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_move(destination, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
    assert!(
        (divider(cx).left() - before.left() - px(40.0)).abs() <= px(1.0),
        "The seam follows the drag within its one-pixel visual width"
    );
}

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
