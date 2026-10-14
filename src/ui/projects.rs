use super::{
    Tervia,
    controls::{icon, icon_style},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    popover::{Popover, PopoverState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use tervia::files::Row;

mod picker;
mod tree;
pub(super) use tree::Navigation;
use tree::Node;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;

gpui_kit::actions!(
    projects,
    [
        NextDirectory,
        PreviousDirectory,
        ExpandDirectory,
        CollapseDirectory,
        SelectDirectory
    ]
);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", NextDirectory, Some("DirectoryPicker")),
        KeyBinding::new("up", PreviousDirectory, Some("DirectoryPicker")),
        KeyBinding::new("right", ExpandDirectory, Some("DirectoryPicker")),
        KeyBinding::new("left", CollapseDirectory, Some("DirectoryPicker")),
        KeyBinding::new("enter", SelectDirectory, Some("DirectoryPicker")),
    ]);
}

impl Tervia {
    fn directory_menu(&self, parent: usize, menu: PopupMenu, view: WeakEntity<Self>) -> PopupMenu {
        let source = view.clone();
        let p = self.p();
        let mut menu = menu
            .item(
                PopupMenuItem::new(self.t("files_refresh"))
                    .icon(icon(IconName::RefreshCw, p.secondary))
                    .on_click(move |_, window, cx| {
                        let _ = source
                            .update(cx, |this, cx| this.refresh_navigation(parent, window, cx));
                    }),
            )
            .item(
                PopupMenuItem::new(self.t("git_scan_repositories"))
                    .icon(icon(IconName::GitBranch, p.secondary))
                    .disabled(self.scanning_project(parent))
                    .on_click(move |_, window, cx| {
                        let _ = view
                            .update(cx, |this, cx| this.rescan_repositories(parent, window, cx));
                    }),
            );
        for notice in self.scan_notices(parent) {
            menu = menu.label(notice);
        }
        menu
    }

    fn directory_row(
        &self,
        parent: usize,
        path: &Path,
        depth: usize,
        picker: Option<WeakEntity<PopoverState>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(all(test, feature = "ui-tests"))]
        self.navigation
            .rendered_rows
            .set(self.navigation.rendered_rows.get() + 1);
        let p = self.p();
        let root = path.as_os_str().is_empty();
        let absolute = self.state.projects[parent].root.join(path);
        let active = self.state.project().root == absolute;
        let external = root
            && self.state.folder(self.state.active_project) == Some(parent)
            && !self.state.project().root.starts_with(&absolute);
        // Linked worktrees can live outside the physical tree; their owner still identifies the active workspace.
        let contains_active = self.state.project().root.starts_with(&absolute) || external;
        let ui = self.navigation.roots.get(&parent);
        let expanded = if root {
            picker.is_some() || self.state.folder_expanded(parent)
        } else {
            ui.is_some_and(|ui| ui.tree.is_expanded(path))
        };
        let has_children = ui
            .and_then(|ui| ui.tree.entries(path))
            .is_none_or(|entries| !entries.is_empty());
        let selected = active || (!expanded && contains_active) || external;
        let cursor = picker.is_some() && ui.is_some_and(|ui| ui.cursor.as_deref() == Some(path));
        let name = if root {
            self.state.projects[parent].name.clone()
        } else {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let known = self.directory_index(parent, path);
        let branch = known
            .filter(|index| {
                self.git_states[*index]
                    .repository
                    .as_ref()
                    .is_some_and(|repo| repo.root == absolute)
            })
            .map(|index| {
                let branch = &self.state.projects[index].branch;
                if branch == "—" {
                    "Git".to_owned()
                } else {
                    branch.clone()
                }
            });
        let mut content = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .when(root, |s| s.font_weight(FontWeight::MEDIUM))
                    .text_ellipsis()
                    .child(name.clone()),
            );
        if (!expanded || external) && contains_active && !active {
            content = content.child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.secondary)
                    .text_ellipsis()
                    .child(
                        self.state
                            .project()
                            .root
                            .strip_prefix(&absolute)
                            .unwrap_or(&self.state.project().root)
                            .to_string_lossy()
                            .into_owned(),
                    ),
            );
        }
        let key = format!("directory-{parent}-{}", path.display());
        let selector = if root {
            format!("project-root-{parent}")
        } else {
            key.clone()
        };
        let relative = path.to_path_buf();
        let close = picker.clone();
        let button = Button::new(SharedString::from(key.clone()))
            // One wrapper owns the complete row highlight, including the disclosure arrow.
            .custom(
                icon_style(p, cx)
                    .foreground(p.text)
                    .hover(p.sidebar.opacity(0.0)),
            )
            .compact()
            .flex_1()
            .min_w_0()
            .h(px(40.0))
            .rounded(px(4.0))
            .debug_selector(move || selector.clone())
            .accessibility_label(self.f("switch_project", &[("name", &name)]))
            .tooltip(if contains_active && !active {
                format!(
                    "{}\n{}",
                    absolute.display(),
                    self.state.project().root.display()
                )
            } else {
                absolute.to_string_lossy().into_owned()
            })
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(icon(
                        if expanded {
                            IconName::FolderOpen
                        } else {
                            IconName::Folder
                        },
                        if contains_active {
                            p.accent
                        } else {
                            p.secondary
                        },
                    ))
                    .child(content)
                    .when_some(branch, |row, branch| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(3.0))
                                .max_w(px(68.0))
                                .text_size(px(10.0))
                                .text_color(p.secondary)
                                .text_ellipsis()
                                .child(icon(IconName::GitBranch, p.secondary).size(px(11.0)))
                                .child(div().min_w_0().text_ellipsis().child(branch)),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                Self::dismiss_directory_picker(&close, window, cx);
                this.activate_directory(parent, relative.clone(), window, cx);
            }));
        let mut row = div()
            .id(SharedString::from(format!("{key}-row")))
            .when(root, |row| {
                row.debug_selector(move || format!("project-row-{parent}"))
            })
            .w_full()
            .h(px(40.0))
            .flex()
            .items_center()
            .rounded(px(4.0))
            .bg(if cursor {
                p.raised
            } else if selected {
                p.selected
            } else {
                p.sidebar.opacity(0.0)
            })
            .hover(move |s| {
                s.bg(if selected && !cursor {
                    p.selected
                } else {
                    p.raised
                })
            })
            .child(div().w(px(depth as f32 * 14.0)).flex_shrink_0());
        if has_children && !(root && picker.is_some()) {
            let relative = path.to_path_buf();
            let selector = if root {
                format!("project-fold-{parent}")
            } else {
                format!("directory-fold-{parent}-{}", path.display())
            };
            row = row.child(
                Button::new(SharedString::from(format!("{key}-fold")))
                    .custom(icon_style(p, cx).hover(p.sidebar.opacity(0.0)))
                    .compact()
                    .w(px(24.0))
                    .h(px(40.0))
                    .flex_shrink_0()
                    .icon(icon(
                        if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        p.secondary,
                    ))
                    .debug_selector(move || selector.clone())
                    .accessibility_label(format!(
                        "{} {name}",
                        self.t(if expanded {
                            "project_collapse"
                        } else {
                            "project_expand"
                        })
                    ))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_navigation(parent, relative.clone(), window, cx)
                    })),
            );
        } else {
            row = row.child(div().w(px(24.0)).flex_shrink_0());
        }
        row = row.child(button).child(div().w(px(4.0)).flex_shrink_0());
        if root && picker.is_none() {
            let owner = cx.weak_entity();
            row.context_menu(move |menu, _, cx| {
                if let Some(view) = owner.upgrade() {
                    view.read(cx).directory_menu(parent, menu, owner.clone())
                } else {
                    menu
                }
            })
            .into_any_element()
        } else {
            row.into_any_element()
        }
    }

    fn navigation_row(
        &self,
        node: &Node,
        picker: Option<WeakEntity<PopoverState>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            Node::Directory {
                parent,
                path,
                depth,
            } => self.directory_row(*parent, path, *depth, picker, cx),
            Node::Status { parent, row } => {
                let p = self.p();
                let (path, depth, text) = match row {
                    Row::Loading { path, depth } => {
                        (path, depth, self.t("files_loading").to_owned())
                    }
                    Row::Empty { path, depth } => {
                        (path, depth, self.t("directories_empty").to_owned())
                    }
                    Row::Error {
                        path,
                        depth,
                        message,
                    } => (path, depth, message.clone()),
                    Row::Entry { .. } => unreachable!(),
                };
                let mut row_view = div()
                    .h(px(40.0))
                    .pl(px(38.0 + *depth as f32 * 14.0))
                    .pr(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .text_color(p.secondary);
                if matches!(row, Row::Error { .. }) {
                    let parent = *parent;
                    let path = path.clone();
                    row_view = row_view.child(
                        Button::new(SharedString::from(format!(
                            "directory-retry-{parent}-{}",
                            path.display()
                        )))
                        .ghost()
                        .compact()
                        .w_full()
                        .tooltip(text)
                        .label(self.t("files_retry"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.load_navigation(parent, path.clone(), true, window, cx)
                            },
                        )),
                    );
                } else {
                    row_view = row_view.child(div().min_w_0().text_ellipsis().child(text));
                }
                row_view.into_any_element()
            }
        }
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let expanded = self.state.sidebar_expanded;
        let mut content = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(8.0))
            .py(px(12.0));
        if expanded {
            let rows: Vec<_> = self
                .state
                .project_rows()
                .into_iter()
                .flat_map(|parent| {
                    self.navigation
                        .rows(parent, self.state.folder_expanded(parent))
                })
                .collect();
            content = content
                .child(
                    div()
                        .px(px(8.0))
                        .mb(px(8.0))
                        .text_size(px(11.0))
                        .text_color(p.secondary)
                        .child(self.t("projects")),
                )
                .child(
                    uniform_list(
                        "directory-tree",
                        rows.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.navigation_row(&rows[index], None, cx))
                                .collect()
                        }),
                    )
                    .size_full()
                    .track_scroll(&self.navigation.scroll),
                );
        } else {
            let mut rail = div()
                .id("project-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(10.0));
            for parent in self.state.project_rows() {
                let name = self.state.projects[parent].name.clone();
                let active = self.state.folder(self.state.active_project) == Some(parent)
                    || self.state.active_project == parent;
                let caption = if active && self.state.active_project != parent {
                    format!(
                        "{}\n{}",
                        self.state.projects[parent].path,
                        self.state.project().path
                    )
                } else {
                    self.state.projects[parent].path.clone()
                };
                let trigger =
                    Button::new(("project", parent))
                        .ghost()
                        .compact()
                        .w_full()
                        .h(px(40.0))
                        .rounded(px(6.0))
                        .bg(if active {
                            p.selected
                        } else {
                            p.sidebar.opacity(0.0)
                        })
                        .debug_selector(move || format!("project-root-{parent}"))
                        .accessibility_label(self.f("project_choose_directory", &[("name", &name)]))
                        .tooltip(caption)
                        .child(
                            div()
                                .relative()
                                .child(
                                    div()
                                        .size(px(28.0))
                                        .rounded(px(7.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(if active { p.accent } else { p.secondary })
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_size(px(13.0))
                                        .child(
                                            name.chars()
                                                .next()
                                                .unwrap_or('T')
                                                .to_uppercase()
                                                .to_string(),
                                        ),
                                )
                                .child(div().absolute().bottom(px(-2.0)).right(px(-4.0)).child(
                                    icon(IconName::ChevronDown, p.secondary).size(px(11.0)),
                                )),
                        );
                if let Some(ui) = self.navigation.roots.get(&parent) {
                    let owner = cx.weak_entity();
                    let opening = owner.clone();
                    rail = rail.child(
                        Popover::new(("directory-picker", parent))
                            .trigger(trigger)
                            .trigger_style(StyleRefinement::default().w_full())
                            .track_focus(&ui.focus)
                            .anchor(Anchor::TopLeft)
                            .on_open_change(move |open, window, cx| {
                                if *open {
                                    let _ = opening.update(cx, |this, cx| {
                                        this.navigation.roots.get_mut(&parent).unwrap().cursor =
                                            None;
                                        this.load_navigation(
                                            parent,
                                            PathBuf::new(),
                                            false,
                                            window,
                                            cx,
                                        );
                                        cx.notify();
                                    });
                                }
                            })
                            .content(move |_, _, cx| {
                                let close = cx.weak_entity();
                                owner
                                    .update(cx, |this, cx| this.directory_picker(parent, close, cx))
                                    .unwrap_or_else(|_| div().into_any_element())
                            }),
                    );
                } else {
                    rail = rail.child(trigger.on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_directory(parent, PathBuf::new(), window, cx)
                    })));
                }
            }
            content = content.child(rail);
        }
        div()
            .w(px(if expanded { 232.0 } else { 56.0 }))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.chrome_border())
            .child(content)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(8.0))
                    .pb(px(12.0))
                    .child(
                        Button::new("add-project")
                            .ghost()
                            .compact()
                            .h(px(36.0))
                            .w_full()
                            .icon(icon(IconName::Plus, p.secondary))
                            .when(expanded, |button| button.label(self.t("add_project")))
                            .accessibility_label(self.t("git_open_repository"))
                            .tooltip(self.t("git_open_repository"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_repository(window, cx)
                            })),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .compact()
                            .h(px(36.0))
                            .w_full()
                            .icon(icon(IconName::Settings, p.secondary))
                            .when(expanded, |button| button.label(self.t("settings")))
                            .accessibility_label(self.t("settings"))
                            .tooltip(self.t("appearance_shortcuts"))
                            .on_click(cx.listener(|this, _, _, cx| this.open_preferences(cx))),
                    ),
            )
    }
}
