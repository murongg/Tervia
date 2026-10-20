use super::{
    Tervia,
    controls::{icon_button, label_button},
    icons::file_icon,
    theme::Palette,
};
use gpui_kit::{
    assets::IconName,
    component::{
        Disableable as _,
        button::ButtonVariants as _,
        menu::{ContextMenuExt as _, PopupMenuItem},
    },
    prelude::FluentBuilder as _,
    *,
};
use tervia::state::{FileClose, Tab};

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

#[derive(Clone)]
struct FileDrag {
    project: usize,
    index: usize,
    name: String,
    path: std::path::PathBuf,
    theme: tervia::preferences::Theme,
}
impl Render for FileDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(&self.theme);
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(5.0))
            .bg(p.raised)
            .text_color(p.text)
            .child(file_icon(&self.path, self.theme.is_light(), None))
            .child(self.name.clone())
    }
}

type TabWrites = Vec<(usize, String, Result<(), String>)>;

impl Tervia {
    pub(super) fn file_tabs(&self, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let owner = &self.state.projects[project];
        let p = Palette::workbench(&self.appearance.theme);
        let mut strip = div()
            .id(("file-tabs-strip", project))
            .size_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(2.0))
            .px(px(8.0))
            .overflow_x_scroll()
            .track_scroll(&self.explorers[project].tabs_scroll);
        for &index in &owner.open_files {
            let file = &owner.files[index];
            let selected = owner.active_file == index;
            let name = file
                .disk_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let drag = FileDrag {
                project,
                index,
                name: name.clone(),
                path: file.disk_path.clone(),
                theme: self.appearance.theme.current(),
            };
            let weak = cx.entity().downgrade();
            let labels = [
                self.t("files_close"),
                self.t("files_close_others"),
                self.t("files_close_right"),
                self.t("files_close_all"),
            ];
            let scopes = [
                FileClose::One(index),
                FileClose::Others(index),
                FileClose::Right(index),
                FileClose::All,
            ];
            let reopen = self.t("files_reopen");
            let can_reopen = !owner.closed_files.is_empty();
            strip = strip.child(
                div()
                    .id(("file-tab", index))
                    .debug_selector(move || format!("file-tab-{index}"))
                    .flex_shrink_0()
                    .min_w(px(110.0))
                    .max_w(px(220.0))
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .bg(if selected { p.selected } else { p.background })
                    .hover(|style| style.bg(p.raised))
                    .child(
                        label_button(("editor-tab", index), "", p)
                            .text()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .rounded(px(0.0))
                            .accessibility_label(self.f("open_file", &[("path", &file.path)]))
                            .tooltip(if owner.preview_file == Some(index) {
                                format!("{}\n{}", file.path, self.t("files_preview_hint"))
                            } else {
                                file.path.clone()
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w_full()
                                    .min_w_0()
                                    .gap(px(6.0))
                                    .px(px(2.0))
                                    .child(file_icon(
                                        &file.disk_path,
                                        self.appearance.theme.is_light(),
                                        None,
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .when(owner.preview_file == Some(index), |name| {
                                                name.italic()
                                            })
                                            .text_color(if selected { p.text } else { p.secondary })
                                            .child(name.clone()),
                                    )
                                    .when(file.dirty(), |row| {
                                        row.child(div().size(px(5.0)).rounded_full().bg(p.accent))
                                    }),
                            )
                            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                if event.click_count() >= 2 {
                                    this.state.projects[project].pin_file(index);
                                }
                                this.select_open_file(project, index, window, cx)
                            })),
                    )
                    .child(
                        icon_button(
                            ("close-file-tab", index),
                            IconName::X,
                            self.t("files_close"),
                            p,
                            cx,
                        )
                        .w(px(24.0))
                        .h(px(24.0))
                        .accessibility_label(self.f("close_named", &[("name", &file.path)]))
                        .disabled(self.file_closing || self.saving_file)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.close_file_tabs(project, FileClose::One(index), window, cx)
                            },
                        )),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close_file_tabs(project, FileClose::One(index), window, cx);
                        }),
                    )
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_drop(cx.listener(move |this, drag: &FileDrag, _, cx| {
                        if drag.project == project {
                            this.state.projects[project].move_file(drag.index, index);
                            this.reveal_file_tab(project);
                            cx.notify();
                        }
                    }))
                    .context_menu(move |mut menu, _, _| {
                        for (label, scope) in labels.into_iter().zip(scopes) {
                            let weak = weak.clone();
                            menu = menu.item(PopupMenuItem::new(label).on_click(
                                move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.close_file_tabs(project, scope, window, cx)
                                    });
                                },
                            ));
                        }
                        let weak = weak.clone();
                        menu.separator().item(
                            PopupMenuItem::new(reopen).disabled(!can_reopen).on_click(
                                move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.reopen_file_tab(project, window, cx)
                                    });
                                },
                            ),
                        )
                    }),
            );
        }
        div()
            .relative()
            .h(px(41.0))
            .flex_shrink_0()
            .min_w_0()
            .border_b_1()
            .border_color(p.chrome_border())
            .child(strip)
            .child(super::scroll::overlay(
                &self.explorers[project].tabs_scroll,
                gpui_kit::component::scroll::ScrollbarAxis::Horizontal,
            ))
    }

    pub(super) fn reveal_file_tab(&self, project: usize) {
        let owner = &self.state.projects[project];
        if let Some(position) = owner
            .open_files
            .iter()
            .position(|index| *index == owner.active_file)
        {
            self.explorers[project].tabs_scroll.scroll_to_item(position);
        }
    }

    pub(super) fn close_active_file_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.state.active_project;
        self.close_file_tabs(
            project,
            FileClose::One(self.state.project().active_file),
            window,
            cx,
        );
    }

    pub(super) fn cycle_file_tab(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.state.active_project;
        self.state.projects[project].cycle_file(backwards);
        if !self.state.projects[project].open_files.is_empty() {
            self.select_open_file(
                project,
                self.state.projects[project].active_file,
                window,
                cx,
            );
        }
    }

    pub(super) fn reopen_file_tab(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.active_project != project || self.file_closing {
            return;
        }
        if let Some(index) = self.state.projects[project].reopen_file() {
            let path = self.state.projects[project].files[index].disk_path.clone();
            self.open_real_file(project, path, window, cx);
            self.reveal_file_tab(project);
            cx.notify();
        }
    }

    pub(super) fn can_close_file_tabs(&self) -> bool {
        !self.file_closing && !self.saving_file && !self.file_reads.has_reloads()
    }

    fn close_file_tabs(
        &mut self,
        project: usize,
        scope: FileClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_close_file_tabs() {
            return;
        }
        let targets = self.state.projects[project].file_targets(scope);
        if targets.is_empty() {
            return;
        }
        let dirty = targets
            .iter()
            .filter(|index| self.state.projects[project].files[**index].dirty())
            .count();
        self.file_reads.select(project);
        if dirty == 0 {
            self.finish_file_close(project, &targets, window, cx);
            return;
        }
        let hint = self.f(
            "files_close_dirty",
            &[
                ("count", &dirty.to_string()),
                ("name", &self.state.projects[project].name),
            ],
        );
        let receiver = window.prompt(
            PromptLevel::Warning,
            self.t("files_close"),
            Some(&hint),
            &[
                self.t("git_cancel"),
                self.t("files_discard"),
                self.t("files_save_close"),
            ],
            cx,
        );
        self.file_closing = true;
        cx.spawn_in(window, async move |weak, cx| {
            let choice = receiver.await;
            let _ = weak.update_in(cx, |this, window, cx| match choice {
                Ok(1) => {
                    this.invalidate_disk_sync(project);
                    for index in &targets {
                        let file = &mut this.state.projects[project].files[*index];
                        if file.dirty() {
                            file.draft = file.saved.clone();
                            let contents = file.saved.clone();
                            this.editors[project][*index]
                                .update(cx, |editor, cx| editor.set_value(contents, window, cx));
                        }
                    }
                    this.file_closing = false;
                    this.finish_file_close(project, &targets, window, cx);
                }
                Ok(2) => this.save_tabs_and_close(project, targets, window, cx),
                _ => {
                    this.file_closing = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn save_tabs_and_close(
        &mut self,
        project: usize,
        targets: Vec<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_mutating {
            self.file_closing = false;
            if self.state.active_project == project {
                self.state.notice = "git_running".into();
            }
            cx.notify();
            return;
        }
        let files: Vec<_> = targets
            .iter()
            .filter_map(|index| {
                let file = &self.state.projects[project].files[*index];
                file.dirty().then(|| {
                    (
                        *index,
                        file.disk_path.clone(),
                        file.draft.clone(),
                        file.saved.clone(),
                    )
                })
            })
            .collect();
        self.invalidate_disk_sync(project);
        self.saving_file = true;
        let root = self.state.projects[project].root.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let writes = cx
                .background_executor()
                .spawn(async move {
                    files
                        .into_iter()
                        .map(|(index, path, draft, saved)| {
                            let result = tervia::files::write(&root, &path, &draft, &saved);
                            (index, draft, result)
                        })
                        .collect::<TabWrites>()
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.finish_tab_saves(project, targets, writes, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_tab_saves(
        &mut self,
        project: usize,
        targets: Vec<usize>,
        writes: TabWrites,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_closing = false;
        self.saving_file = false;
        let mut error = None;
        for (index, contents, result) in writes {
            match result {
                Ok(()) => self.record_file_saved(project, index, contents),
                Err(message) => {
                    error.get_or_insert(message);
                }
            }
        }
        let edited = targets
            .iter()
            .any(|index| self.state.projects[project].files[*index].dirty());
        if error.is_some() || edited {
            if self.state.active_project == project {
                self.state.notice = error.unwrap_or_else(|| "files_close_edited".into());
            }
            cx.notify();
        } else {
            self.finish_file_close(project, &targets, window, cx);
        }
        self.refresh_git(project, window, cx);
    }

    fn finish_file_close(
        &mut self,
        project: usize,
        targets: &[usize],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.projects[project].close_files(targets);
        self.prune_previews(project);
        self.reveal_file_tab(project);
        // Local screens retain the last project index, so a completed close must also check the visible module.
        if self.state.active_project == project && self.state.active_tab() == Tab::Files {
            if self.state.project().current_file().is_some() {
                self.focus_file_document(project, self.state.project().active_file, window, cx);
            } else {
                self.focus.focus(window, cx);
            }
        }
        cx.notify();
    }
}
