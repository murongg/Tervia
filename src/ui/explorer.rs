use crate::ui::scroll::ListScrollbars as _;
use std::path::PathBuf;

use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::{assets::IconName, component::button::ButtonVariants as _, *};
use tervia::files::{EntryKind, Row, Tree};

use super::{
    Tervia,
    controls::{icon, icon_button, label_button},
    theme::Palette,
};

pub(super) struct Explorer {
    pub tree: Tree,
    scroll: UniformListScrollHandle,
    pub tabs_scroll: ScrollHandle,
    pub previews: super::files::Previews,
    pub finder: super::files::Finder,
    #[cfg(all(test, feature = "ui-tests"))]
    pub rendered_rows: std::cell::Cell<usize>,
}

impl Explorer {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Tervia>) -> Self {
        Self {
            tree: Tree::default(),
            scroll: UniformListScrollHandle::new(),
            tabs_scroll: ScrollHandle::new(),
            previews: Default::default(),
            finder: super::files::Finder::new(window, cx),
            #[cfg(all(test, feature = "ui-tests"))]
            rendered_rows: std::cell::Cell::new(0),
        }
    }
}

impl Tervia {
    pub(super) fn file_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        if self.explorers[project].finder.mode != super::files::search::Mode::Browse {
            return self.search_sidebar(cx);
        }
        let ui = &self.explorers[project];
        let p = Palette::workbench(self.appearance.light_theme);
        let rows = ui.tree.rows();
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .h(px(44.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .font_weight(FontWeight::MEDIUM)
                            .child(self.t("file_explorer")),
                    )
                    .child(
                        icon_button(
                            "files-quick-open",
                            IconName::FileSearch,
                            self.t("files_find_name"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("files_find_name"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_file_search(super::files::search::Mode::Files, window, cx)
                        })),
                    )
                    .child(
                        icon_button(
                            "files-project-search",
                            IconName::Search,
                            self.t("files_search_text"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("files_search_text"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_file_search(super::files::search::Mode::Text, window, cx)
                        })),
                    )
                    .child(
                        icon_button(
                            "explorer-open",
                            IconName::FilePlus,
                            self.t("git_open_file"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("git_open_file"))
                        .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
                    )
                    .child(
                        icon_button(
                            "explorer-refresh",
                            IconName::RefreshCw,
                            self.t("files_refresh"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("files_refresh"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.refresh_explorer(project, window, cx),
                        )),
                    ),
            )
            .child(
                uniform_list(
                    ("explorer-rows", project),
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.explorer_row(project, index, &rows[index], cx))
                            .collect()
                    }),
                )
                .size_full()
                .scrollbar(&ui.scroll),
            )
    }

    fn explorer_row(
        &self,
        project: usize,
        index: usize,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(all(test, feature = "ui-tests"))]
        self.explorers[project]
            .rendered_rows
            .set(self.explorers[project].rendered_rows.get() + 1);
        let p = Palette::workbench(self.appearance.light_theme);
        match row {
            Row::Entry { entry, depth } => {
                let path = entry.path.clone();
                let folder = entry.kind == EntryKind::Directory;
                let nested = self.explorers[project].tree.has_nested(&path);
                let expanded = self.explorers[project].tree.is_expanded(&path);
                let owner = &self.state.projects[project];
                let file = owner.files.iter().position(|file| file.disk_path == path);
                let selected = file.is_some_and(|index| {
                    owner.active_file == index && owner.open_files.contains(&index)
                });
                let dirty = file.is_some_and(|index| owner.files[index].dirty());
                let display = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let mut content = div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(px(6.0))
                    .pr(px(12.0));
                let disclosure = if nested {
                    let path = path.clone();
                    div()
                        .debug_selector(move || format!("explorer-nesting-{}", path.display()))
                        .child(
                            icon_button(
                                ("explorer-nesting", index),
                                if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                },
                                self.t(if expanded {
                                    "git_collapse"
                                } else {
                                    "git_expand"
                                }),
                                p,
                                cx,
                            )
                            .w(px(20.0))
                            .h(px(28.0))
                            .accessibility_label(format!(
                                "{} {}",
                                self.t(if expanded {
                                    "git_collapse"
                                } else {
                                    "git_expand"
                                }),
                                entry.path.display()
                            ))
                            .on_click(cx.listener({
                                let path = entry.path.clone();
                                move |this, _, _, cx| {
                                    this.explorers[project].tree.toggle_nested(&path);
                                    cx.notify();
                                }
                            })),
                        )
                        .into_any_element()
                } else if folder {
                    let path = path.clone();
                    div()
                        .w(px(20.0))
                        .flex()
                        .justify_center()
                        .child(icon(
                            if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            },
                            p.secondary,
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.toggle_directory(project, path.clone(), window, cx);
                            }),
                        )
                        .into_any_element()
                } else {
                    div().w(px(20.0)).into_any_element()
                };
                content = content
                    .child(icon(
                        if folder {
                            if expanded {
                                IconName::FolderOpen
                            } else {
                                IconName::Folder
                            }
                        } else if entry.kind == EntryKind::Symlink {
                            IconName::Link
                        } else if tervia::files::image_mime(&path).is_some() {
                            IconName::Image
                        } else if path.extension().is_some_and(|ext| ext == "md") {
                            IconName::FileText
                        } else {
                            IconName::FileCode
                        },
                        if selected { p.accent } else { p.secondary },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_color(if selected { p.text } else { p.secondary })
                            .child(display),
                    );
                if dirty {
                    content = content.child(div().size(px(5.0)).rounded_full().bg(p.accent));
                }
                let description = if folder {
                    format!(
                        "{} {}",
                        self.t(if expanded {
                            "git_collapse"
                        } else {
                            "git_expand"
                        }),
                        path.display()
                    )
                } else {
                    self.f("open_file", &[("path", &path.to_string_lossy())])
                };
                let parent = self.working_directory_owner();
                let directory = self.state.projects[project].root.join(&path);
                let relative = directory
                    .strip_prefix(&self.state.projects[parent].root)
                    .ok()
                    .map(PathBuf::from);
                let row = div()
                    .id(("explorer-row", index))
                    .h(px(30.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .pl(px(8.0 + *depth as f32 * 14.0))
                    .bg(if selected { p.selected } else { p.background })
                    .hover(|style| style.bg(p.raised))
                    .child(disclosure)
                    .child(
                        div()
                            .debug_selector({
                                let path = path.clone();
                                move || format!("explorer-file-{}", path.display())
                            })
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(
                                label_button(("explorer-entry", index), "", p)
                                    .text()
                                    .h_full()
                                    .w_full()
                                    .rounded(px(0.0))
                                    .accessibility_label(description)
                                    .tooltip(path.to_string_lossy().into_owned())
                                    .child(content)
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, window, cx| {
                                            if folder {
                                                this.toggle_directory(
                                                    project,
                                                    path.clone(),
                                                    window,
                                                    cx,
                                                );
                                            } else {
                                                if event.click_count() >= 2 {
                                                    this.open_real_file(
                                                        project,
                                                        path.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                } else {
                                                    this.open_preview_file(
                                                        project,
                                                        path.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            }
                                        },
                                    )),
                            ),
                    );
                if folder && let Some(path) = relative {
                    let view = cx.weak_entity();
                    let label = self.t("working_directory_set");
                    row.context_menu(move |menu, _, _| {
                        let view = view.clone();
                        let path = path.clone();
                        menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.activate_directory(parent, path.clone(), window, cx)
                            });
                        }))
                    })
                    .into_any_element()
                } else {
                    row.into_any_element()
                }
            }
            Row::Loading { depth, .. } | Row::Empty { depth, .. } => div()
                .h(px(30.0))
                .w_full()
                .pl(px(28.0 + *depth as f32 * 14.0))
                .flex()
                .items_center()
                .text_size(px(12.0))
                .text_color(p.secondary)
                .child(self.t(if matches!(row, Row::Loading { .. }) {
                    "files_loading"
                } else {
                    "files_empty"
                }))
                .into_any_element(),
            Row::Error {
                path,
                depth,
                message,
            } => {
                let path = path.clone();
                label_button(("explorer-retry", index), self.t("files_retry"), p)
                    .text()
                    .h(px(30.0))
                    .w_full()
                    .rounded(px(0.0))
                    .pl(px(28.0 + *depth as f32 * 14.0))
                    .icon(icon(IconName::RefreshCw, p.secondary))
                    .tooltip(message.clone())
                    .accessibility_label(format!("{}: {}", self.t("files_retry"), message))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.load_directory(project, path.clone(), true, window, cx)
                    }))
                    .into_any_element()
            }
        }
    }

    fn toggle_directory(
        &mut self,
        project: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorers[project].tree.toggle(&path) {
            self.load_directory(project, path, false, window, cx);
        }
        cx.notify();
    }

    pub(super) fn load_directory(
        &mut self,
        project: usize,
        path: PathBuf,
        refresh: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_watch(project, window, cx);
        let Some(token) = self.explorers[project].tree.begin(path.clone(), refresh) else {
            return;
        };
        self.request_directory(project, path, token, window, cx);
    }

    fn request_directory(
        &mut self,
        project: usize,
        path: PathBuf,
        token: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.state.projects[project].root.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let directory = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { tervia::files::list(&root, &directory) })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // The captured owner and generation keep late results away from another project.
                let listed = result.is_ok();
                if this.explorers[project].tree.finish(&path, token, result) {
                    if listed {
                        this.register_directory(project, path.clone(), window, cx);
                    }
                    cx.notify();
                }
                this.tree_settled(project, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn refresh_explorer(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (path, token) in self.explorers[project].tree.refresh() {
            self.request_directory(project, path, token, window, cx);
        }
    }
}
