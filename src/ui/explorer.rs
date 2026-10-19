use crate::ui::scroll::ListScrollbars as _;
use std::path::PathBuf;

use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::{
    assets::IconName,
    component::{Disableable as _, button::ButtonVariants as _},
    *,
};
use tervia::files::{EntryKind, Row, Tree};

use super::{
    Tervia,
    controls::{icon, icon_button, label_button},
    icons::file_icon,
    theme::Palette,
};

pub(super) struct Explorer {
    pub tree: Tree,
    scroll: UniformListScrollHandle,
    pub tabs_scroll: ScrollHandle,
    pub previews: super::files::Previews,
    pub finder: super::files::Finder,
    pub definitions: tervia::lsp::Client,
    pub definition_status: Option<tervia::lsp::tools::Status>,
    pub definition_preview: super::files::definitions::Peek,
    pub management: super::files::management::Management,
    pub file_history: super::files::History,
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
            definitions: Default::default(),
            definition_status: None,
            definition_preview: Default::default(),
            management: super::files::management::Management::new(window, cx),
            file_history: Default::default(),
            #[cfg(all(test, feature = "ui-tests"))]
            rendered_rows: std::cell::Cell::new(0),
        }
    }
}

fn entry_colors(ignored: bool, selected: bool, p: Palette) -> (Hsla, Hsla) {
    if ignored {
        // Selected filenames keep the normal readable foreground; their icons remain subdued.
        (if selected { p.text } else { p.ignored }, p.ignored)
    } else if selected {
        (p.text, p.accent)
    } else {
        (p.text, p.text)
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn luminance(color: Hsla) -> f32 {
        let color = Rgba::from(color);
        let channels = [color.r, color.g, color.b].map(|value| {
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        });
        channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
    }

    #[test]
    fn ignored_explorer_entries_have_a_visible_tonal_difference_in_both_themes() {
        for light in [false, true] {
            let p = Palette::workbench(light);
            let normal = entry_colors(false, false, p);
            let ignored = entry_colors(true, false, p);
            for (part, normal, ignored) in [
                ("label", normal.0, ignored.0),
                ("icon", normal.1, ignored.1),
            ] {
                let gray = Rgba::from(ignored);
                let channels = [gray.r, gray.g, gray.b];
                let spread = channels.into_iter().fold(f32::NEG_INFINITY, f32::max)
                    - channels.into_iter().fold(f32::INFINITY, f32::min);
                assert!(spread <= 0.01, "light={light}, {part}: tinted ignore color");
                let normal = luminance(normal);
                let ignored = luminance(ignored);
                let separation = (normal.max(ignored) + 0.05) / (normal.min(ignored) + 0.05);
                // A distinct color value alone can still look identical in the file tree.
                assert!(separation >= 2.0, "light={light}, {part}: {separation}");
                assert!(if light {
                    normal < ignored
                } else {
                    normal > ignored
                });
            }
        }
    }

    #[test]
    fn ignored_explorer_icons_stay_gray_when_selected_in_both_themes() {
        for light in [false, true] {
            let p = Palette::workbench(light);
            let normal = entry_colors(false, false, p);
            let ignored = entry_colors(true, false, p);
            assert_ne!(ignored.0, normal.0);
            assert_eq!(ignored.0, ignored.1);
            assert_eq!(entry_colors(true, true, p), (p.text, ignored.1));
            assert_eq!(entry_colors(false, true, p), (p.text, p.accent));
        }
    }

    #[test]
    fn ignored_explorer_labels_stay_visible_and_selected_labels_keep_readable_contrast() {
        // Synthetic native-preview backgrounds include the gradient and selected row overlay.
        for (light, backgrounds) in [(false, [0x4a4e5a, 0x545d6c]), (true, [0xced8e7, 0xd9dfeb])] {
            for (selected, background) in [false, true].into_iter().zip(backgrounds) {
                let ink = luminance(entry_colors(true, selected, Palette::workbench(light)).0);
                let background = luminance(rgb(background).into());
                let contrast = (ink.max(background) + 0.05) / (ink.min(background) + 0.05);
                assert!(
                    // Resting entries deliberately recede; selection restores full text contrast.
                    contrast >= if selected { 4.5 } else { 2.0 },
                    "light={light}, selected={selected}: {contrast}"
                );
            }
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
                    .gap(px(2.0))
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
                            self.t("files_new_file"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("files_new_file"))
                        .disabled(!self.can_manage_files())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.edit_entry(
                                    project,
                                    super::files::management::Edit::File(PathBuf::new()),
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .child(
                        icon_button(
                            "explorer-new-folder",
                            IconName::FolderPlus,
                            self.t("files_new_folder"),
                            p,
                            cx,
                        )
                        .accessibility_label(self.t("files_new_folder"))
                        .disabled(!self.can_manage_files())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.edit_entry(
                                    project,
                                    super::files::management::Edit::Directory(PathBuf::new()),
                                    window,
                                    cx,
                                )
                            },
                        )),
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
            .child(self.entry_form(project, cx))
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
                let (text_color, icon_color) = entry_colors(entry.ignored, selected, p);
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
                            icon_color,
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
                    .child(if folder {
                        icon(
                            if expanded {
                                IconName::FolderOpen
                            } else {
                                IconName::Folder
                            },
                            icon_color,
                        )
                        .into_any_element()
                    } else if entry.kind == EntryKind::Symlink {
                        icon(IconName::Link, icon_color).into_any_element()
                    } else {
                        file_icon(
                            &path,
                            self.appearance.light_theme,
                            entry.ignored.then_some(icon_color),
                        )
                        .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_color(text_color)
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
                let menu_path = path.clone();
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
                use super::files::management::Edit;
                let mut edits = Vec::new();
                if folder {
                    edits.push((self.t("files_new_file"), Edit::File(menu_path.clone())));
                    edits.push((
                        self.t("files_new_folder"),
                        Edit::Directory(menu_path.clone()),
                    ));
                }
                edits.extend([
                    (self.t("files_rename"), Edit::Rename(menu_path.clone())),
                    (self.t("files_copy"), Edit::Copy(menu_path.clone())),
                    (self.t("files_move"), Edit::Move(menu_path.clone())),
                ]);
                let view = cx.weak_entity();
                let enabled = self.can_manage_files();
                let trash = self.t("files_trash");
                let copy_path = self.t("worktree_copy_path");
                let reveal = self.t("project_reveal");
                let working_directory = self.t("working_directory_set");
                row.context_menu(move |mut menu, _, _| {
                    for (label, edit) in edits.iter().cloned() {
                        let view = view.clone();
                        menu = menu.item(PopupMenuItem::new(label).disabled(!enabled).on_click(
                            move |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.edit_entry(project, edit.clone(), window, cx)
                                });
                            },
                        ));
                    }
                    let deleting = view.clone();
                    let path = menu_path.clone();
                    menu = menu.separator().item(
                        PopupMenuItem::new(trash).disabled(!enabled).on_click(
                            move |_, window, cx| {
                                let _ = deleting.update(cx, |this, cx| {
                                    this.trash_entry(project, path.clone(), window, cx)
                                });
                            },
                        ),
                    );
                    let copied = directory.clone();
                    let revealed = directory.clone();
                    menu = menu
                        .separator()
                        .item(PopupMenuItem::new(copy_path).on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copied.to_string_lossy().into_owned(),
                            ))
                        }))
                        .item(
                            PopupMenuItem::new(reveal)
                                .on_click(move |_, _, cx| cx.reveal_path(&revealed)),
                        );
                    if folder && let Some(path) = relative.clone() {
                        let view = view.clone();
                        menu =
                            menu.separator()
                                .item(PopupMenuItem::new(working_directory).on_click(
                                    move |_, window, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.activate_directory(
                                                parent,
                                                path.clone(),
                                                window,
                                                cx,
                                            )
                                        });
                                    },
                                ));
                    }
                    menu
                })
                .into_any_element()
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

    pub(super) fn refresh_explorer_paths(
        &mut self,
        project: usize,
        paths: &std::collections::BTreeSet<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (path, token) in self.explorers[project].tree.refresh_paths(paths) {
            self.request_directory(project, path, token, window, cx);
        }
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
