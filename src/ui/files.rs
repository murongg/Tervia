use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable as _,
    input::{Editor, EditorState, InputEvent},
    resizable::{h_resizable, resizable_panel},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::path::PathBuf;
use tervia::state::{File, Tab};

use super::{
    Tervia,
    controls::{icon, icon_button, label_button},
};
mod preview;
pub(in crate::ui) mod search;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;
pub(super) use preview::Previews;
pub(super) use search::Finder;

#[derive(Clone, Copy)]
enum Open {
    Pinned,
    Preview,
    Reload,
    Match { line: usize, column: usize },
}
#[cfg(all(test, feature = "ui-tests"))]
use core::prelude::v1::test;

#[cfg(all(test, feature = "ui-tests"))]
#[gpui_test::test]
fn project_files_render_before_any_document_is_open_and_only_build_visible_rows(
    cx: &mut gpui_test::TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/explorer".into());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            let tree = &mut this.explorers[project].tree;
            let token = tree.begin(PathBuf::new(), false).unwrap();
            tree.finish(
                "".as_ref(),
                token,
                Ok((0..2000)
                    .map(|i| tervia::files::Entry {
                        path: format!("synthetic-{i}.rs").into(),
                        kind: tervia::files::EntryKind::File,
                    })
                    .collect()),
            );
            assert!(this.state.project().files.is_empty());
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|_, cx| {
        view.update(cx, |this, _| {
            let rows = this.explorers[0].rendered_rows.get();
            assert!(
                rows > 0 && rows < 100,
                "Rendered {rows} rows with no open editor"
            );
        });
    });
}

#[cfg(all(test, feature = "ui-tests"))]
#[gpui_test::test]
fn switching_modules_invalidates_pending_file_navigation(cx: &mut gpui_test::TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/navigation".into());
            this.init_project(project, window, cx);
            this.state.set_tab(Tab::Files);
            let selection = this.file_reads.select(project);
            let request = this
                .file_reads
                .begin(project, "mock.rs".into(), None, false);
            this.change_tab(Tab::Git, window, cx);
            assert_eq!(
                this.file_reads
                    .finish(project, "mock.rs".as_ref(), request, None),
                tervia::files::ReadOutcome::Apply
            );
            assert!(
                !this.file_reads.selected(project, selection),
                "A late file may be loaded without overriding later module navigation"
            );
            assert_eq!(this.state.project().tab, Tab::Git);
        });
    });
}

#[cfg(all(test, feature = "ui-tests"))]
#[gpui_test::test]
fn automatic_disk_updates_keep_the_owner_draft_and_module(cx: &mut gpui_test::TestAppContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let alpha = this.state.open_directory("/synthetic/alpha".into());
            this.state.projects[alpha].files.push(File {
                path: "mock.rs".into(),
                disk_path: "mock.rs".into(),
                language: "rust".into(),
                saved: "old".into(),
                draft: "old".into(),
            });
            this.init_project(alpha, window, cx);
            let beta = this.state.open_directory("/synthetic/beta".into());
            this.init_project(beta, window, cx);
            this.apply_disk_updates(
                alpha,
                vec![(0, "mock.rs".into(), Ok("external".into()))],
                window,
                cx,
            );
            assert_eq!(this.state.projects[alpha].files[0].draft, "external");
            assert_eq!(this.state.active_project, beta);
            assert_eq!(this.state.project().tab, Tab::Git);
            this.state.projects[alpha].files[0].draft = "newly typed".into();
            this.apply_disk_updates(
                alpha,
                vec![(0, "mock.rs".into(), Ok("new disk text".into()))],
                window,
                cx,
            );
            assert_eq!(this.state.projects[alpha].files[0].draft, "newly typed");
            assert!(
                this.monitors[alpha]
                    .external
                    .contains_key(std::path::Path::new("mock.rs"))
            );
        })
    });
}

#[cfg(all(test, feature = "ui-tests"))]
#[gpui_test::test]
fn background_pauses_monitoring_and_foreground_refreshes_immediately(
    cx: &mut gpui_test::TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Tervia::new(window, cx));
        view = Some(entity.clone());
        gpui_kit::base::Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            let project = this.state.open_directory("/synthetic/activation".into());
            this.init_project(project, window, cx);
            this.monitors[project].enabled = true;
            this.files_active = true;
            this.monitor_activation(false, window, cx);
            assert!(!this.files_active);
            this.poll_files(true, window, cx);
            assert!(
                !this.explorers[project].tree.loading(),
                "No background scans may start"
            );
            this.monitor_activation(true, window, cx);
            assert!(this.files_active);
            assert!(
                this.explorers[project].tree.loading(),
                "Reactivation starts a refresh immediately, without waiting for the timer"
            );
        })
    });
}

impl Tervia {
    pub(super) fn files(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = super::theme::Palette::workbench(self.appearance.light_theme);
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .border_t_1()
            .border_color(p.border)
            .child(
                // The resize handle owns this seam; a sidebar border would draw it twice.
                h_resizable(("files-split", self.state.active_project))
                    .child(
                        resizable_panel()
                            .size(px(240.0))
                            .size_range(px(180.0)..px(440.0))
                            .flex_none()
                            .child(self.file_sidebar(cx)),
                    )
                    .child(resizable_panel().child(self.file_editor(window, cx))),
            )
    }

    fn file_editor(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = super::theme::Palette::workbench(self.appearance.light_theme);
        let project = self.state.project();
        let owner = self.state.active_project;
        let active = project.active_file;
        let Some(file) = project.current_file() else {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .items_center()
                .justify_center()
                .text_color(p.secondary)
                .child(self.t("files_select"))
                .child(
                    label_button("open-file-empty", self.t("git_open_file"), p)
                        .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
                );
        };
        let external = self.monitors[owner].external.get(&file.disk_path).copied();
        let tabs = self.file_tabs(cx);
        let breadcrumbs = div()
            .h(px(35.0))
            .flex_shrink_0()
            .px(px(20.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .text_size(px(11.0))
            .text_color(p.secondary)
            .when_some(self.git_return_button(cx), |bar, button| bar.child(button))
            .when(file.language == "markdown", |bar| {
                bar.child(self.source_toggle(owner, active, cx))
            })
            .child(icon(IconName::FileCode, p.muted))
            .child(
                div().flex_1().min_w_0().text_ellipsis().child(
                    file.disk_path
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("  ›  "),
                ),
            )
            .child(
                icon_button(
                    "reload-file",
                    IconName::RefreshCw,
                    self.t("git_reload"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("git_reload"))
                .on_click(cx.listener(|this, _, window, cx| this.reload_file(window, cx))),
            )
            .child(
                label_button("find-in-file", self.t("find"), p)
                    .debug_selector(|| "find-in-file".into())
                    .disabled(file.is_image())
                    .icon(icon(IconName::Search, p.secondary))
                    .tooltip(tervia::platform::Platform::CURRENT.hint(self.t("find_shortcut")))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let project = this.state.active_project;
                        let index = this.state.project().active_file;
                        this.show_file_source(project, index);
                        this.editors[project][index].update(cx, |editor, cx| {
                            editor.focus(window, cx);
                            editor.open_search(false, cx);
                        });
                    })),
            )
            .child(
                label_button("save-file", self.t("save"), p)
                    .disabled(file.is_image() || !file.dirty())
                    .icon(icon(IconName::Save, p.accent))
                    .tooltip(tervia::platform::Platform::CURRENT.hint(self.t("save_shortcut")))
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            );
        let editor = &self.editors[self.state.active_project][active];
        div().size_full().min_h_0().flex().child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .child(tabs)
                .child(breadcrumbs)
                .when(external.is_some(), |body| {
                    body.child(
                        div()
                            .h(px(32.0))
                            .flex_shrink_0()
                            .px(px(20.0))
                            .flex()
                            .items_center()
                            .text_size(px(12.0))
                            .text_color(p.secondary)
                            .child(self.t(external.unwrap())),
                    )
                })
                .child(div().flex_1().min_h_0().overflow_hidden().child(
                    if let Some(preview) = self.file_preview(owner, active) {
                        preview
                    } else {
                        Editor::new(editor)
                            .h(relative(1.0))
                            .w_full()
                            .appearance(false)
                            .bordered(false)
                            .aria_label(self.f("code_editor", &[("path", &file.path)]))
                            .font_family(self.appearance.font_family.clone())
                            .text_size(px(self.appearance.code_size as f32))
                            .into_any_element()
                    },
                )),
        )
    }
    pub(super) fn open_real_file(
        &mut self,
        project: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.read_real_file(project, path, Open::Pinned, window, cx);
    }

    pub(super) fn open_preview_file(
        &mut self,
        project: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.read_real_file(project, path, Open::Preview, window, cx);
    }

    pub(super) fn open_file_match(
        &mut self,
        project: usize,
        path: PathBuf,
        line: usize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.read_real_file(project, path, Open::Match { line, column }, window, cx);
    }

    pub(super) fn select_open_file(
        &mut self,
        project: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.active_project != project
            || !self.state.projects[project].open_files.contains(&index)
            || self.worktree_removing(
                &self.state.projects[project]
                    .root
                    .join(&self.state.projects[project].files[index].disk_path),
            )
        {
            return;
        }
        self.file_reads.select(project);
        self.state.projects[project].active_file = index;
        self.reveal_file_tab(project);
        self.focus_file_document(project, index, window, cx);
        cx.notify();
    }

    pub(super) fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.state.active_project;
        let root = self.state.projects[project].root.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("git_open_file").into()),
        });
        cx.spawn_in(window, async move |weak, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let relative = path
                    .strip_prefix(&root)
                    .map(PathBuf::from)
                    .map_err(|_| "Choose a file inside this project".to_owned());
                let _ = weak.update_in(cx, |this, window, cx| match relative {
                    Ok(path) => this.open_real_file(project, path, window, cx),
                    Err(error) => {
                        this.state.notice = error;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn reload_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.git_mutating || self.saving_file || self.file_closing {
            return;
        }
        let project = self.state.active_project;
        let Some(file) = self.state.project().current_file() else {
            return;
        };
        let path = file.disk_path.clone();
        if !file.dirty() {
            self.read_real_file(project, path, Open::Reload, window, cx);
            return;
        }
        let receiver = window.prompt(
            PromptLevel::Warning,
            self.t("git_reload"),
            Some(self.t("git_reload_hint")),
            &[self.t("git_cancel"), self.t("git_confirm")],
            cx,
        );
        self.saving_file = true;
        cx.spawn_in(window, async move |weak, cx| {
            let accepted = receiver.await == Ok(1);
            let _ = weak.update_in(cx, |this, window, cx| {
                this.saving_file = false;
                if accepted {
                    this.read_real_file(project, path, Open::Reload, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn read_real_file(
        &mut self,
        project: usize,
        path: PathBuf,
        open: Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reload = matches!(open, Open::Reload);
        self.invalidate_disk_sync(project);
        let root = self.state.projects[project].root.clone();
        if !self.state.project_visible(project) || self.worktree_removing(&root.join(&path)) {
            return;
        }
        let draft = self.state.projects[project]
            .files
            .iter()
            .find(|file| file.disk_path == path)
            .map(|file| file.draft.clone());
        let selection = if reload {
            self.file_reads.selection(project)
        } else {
            self.file_reads.select(project)
        };
        let request = self.file_reads.begin(project, path.clone(), draft, reload);
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let read_path = path.clone();
            let read_root = root.clone();
            let result = cx
                .background_executor()
                .spawn(async move { preview::load(&read_root, &read_path) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let draft = this.state.projects[project]
                    .files
                    .iter()
                    .find(|file| file.disk_path == path)
                    .map(|file| file.draft.as_str());
                match this.file_reads.finish(project, &path, request, draft) {
                    tervia::files::ReadOutcome::Stale => {
                        cx.notify();
                        return;
                    }
                    tervia::files::ReadOutcome::Edited => {
                        if this.state.active_project == project {
                            this.state.notice = "git_reload_skipped".into();
                        }
                        cx.notify();
                        return;
                    }
                    tervia::files::ReadOutcome::Apply => {}
                }
                match result {
                    Ok(content) => {
                        let (contents, image) = match content {
                            preview::Content::Text(text) => (text, None),
                            preview::Content::Image(image) => (String::new(), Some(image)),
                        };
                        if !this.state.project_visible(project)
                            || this.worktree_removing(&root.join(&path))
                        {
                            return;
                        }
                        let directory = path.parent().map(PathBuf::from).unwrap_or_default();
                        let existing = this.state.projects[project]
                            .files
                            .iter()
                            .position(|file| file.disk_path == path);
                        let index = existing.unwrap_or(this.state.projects[project].files.len());
                        if existing.is_none() {
                            this.invalidate_disk_sync(project);
                            let language = if image.is_some() {
                                "image".to_owned()
                            } else {
                                gpui_kit::component::highlighter::language_for_path(
                                    &path.to_string_lossy(),
                                )
                                .to_string()
                            };
                            this.state.projects[project].files.push(File {
                                path: path.to_string_lossy().into_owned(),
                                disk_path: path,
                                language: language.clone(),
                                saved: contents.clone(),
                                draft: contents.clone(),
                            });
                            let editor = cx.new(|cx| {
                                EditorState::new(window, cx)
                                    .language(language)
                                    .line_number(true)
                                    .default_value(contents)
                            });
                            this.subscriptions.push(cx.subscribe_in(
                                &editor,
                                window,
                                move |this, editor, event, _, cx| {
                                    if matches!(event, InputEvent::Change) {
                                        this.state.projects[project].files[index].draft =
                                            editor.read(cx).value().to_string();
                                        if this.state.projects[project].files[index].dirty() {
                                            this.state.projects[project].pin_file(index);
                                        }
                                        cx.notify();
                                    }
                                },
                            ));
                            this.editors[project].push(editor);
                        } else if reload || !this.state.projects[project].files[index].dirty() {
                            this.record_file_saved(project, index, contents.clone());
                            this.state.projects[project].files[index].draft = contents.clone();
                            this.editors[project][index]
                                .update(cx, |editor, cx| editor.set_value(contents, window, cx));
                        }
                        if let Some(image) = image {
                            this.install_image(project, index, image);
                        }
                        this.register_directory(project, directory, window, cx);
                        // A slower read may populate its document, but cannot steal a newer selection.
                        if this.state.active_project == project
                            && this.file_reads.selected(project, selection)
                        {
                            if matches!(open, Open::Preview) {
                                this.state.projects[project].preview_file(index);
                            } else if !reload {
                                this.state.open_file(index);
                            }
                            this.reveal_file_tab(project);
                            this.change_tab(Tab::Files, window, cx);
                            this.focus_file_document(project, index, window, cx);
                            this.reveal_git_file(project, selection, index, window, cx);
                            if let Open::Match { line, column } = open {
                                this.show_file_source(project, index);
                                this.editors[project][index].update(cx, |editor, cx| {
                                    editor.set_cursor_position(
                                        gpui_kit::component::input::Position::new(
                                            line.saturating_sub(1) as u32,
                                            column as u32,
                                        ),
                                        window,
                                        cx,
                                    )
                                });
                            }
                        }
                    }
                    Err(error) => {
                        if this.state.active_project == project {
                            this.state.notice = error.clone();
                        }
                        this.git_states[project].error.action(error);
                    }
                }
                this.prune_previews(project);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn can_save_file(&self) -> bool {
        self.state.project().tab == Tab::Files
            && !self.git_mutating
            && !self.saving_file
            && !self.file_closing
            && !self.file_reads.has_reloads()
            && self
                .state
                .project()
                .current_file()
                .is_some_and(|file| !file.is_image())
    }

    pub(super) fn save_real_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save_file() {
            return;
        }
        let project = self.state.active_project;
        let index = self.state.project().active_file;
        let Some(file) = self.state.project().current_file() else {
            return;
        };
        if file.is_image() {
            return;
        }
        let path = file.disk_path.clone();
        let root = self.state.projects[project].root.clone();
        let draft = file.draft.clone();
        let written = draft.clone();
        let expected = file.saved.clone();
        self.invalidate_disk_sync(project);
        self.saving_file = true;
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { tervia::files::write(&root, &path, &written, &expected) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.saving_file = false;
                match result {
                    Ok(()) => {
                        this.record_file_saved(project, index, draft);
                        if this.state.active_project == project {
                            this.state.notice = "git_saved".into();
                        }
                        this.refresh_git(project, window, cx);
                    }
                    Err(error) => {
                        if this.state.active_project == project {
                            this.state.notice = error.clone();
                        }
                        this.git_states[project].error.action(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
