use super::{
    Tervia,
    controls::{icon, icon_style},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;

mod navigation;
mod picker;
pub(super) use navigation::Navigation;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;

gpui_kit::actions!(projects, [SwitchWorkingDirectory]);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "secondary-alt-o",
        SwitchWorkingDirectory,
        Some("Tervia"),
    )]);
}

impl Tervia {
    fn project_menu(&self, parent: usize, menu: PopupMenu, view: WeakEntity<Self>) -> PopupMenu {
        let opening = view.clone();
        let p = self.p();
        let mut menu = menu
            .item(
                PopupMenuItem::new(self.t("working_directory_add"))
                    .icon(icon(IconName::FolderPlus, p.secondary))
                    .on_click(move |_, window, cx| {
                        let _ = opening.update(cx, |this, cx| {
                            this.choose_working_directory(parent, window, cx)
                        });
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

    fn project_row(&self, parent: usize, expanded: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p();
        let active = self.working_directory_owner() == parent;
        let name = self.state.project_label(parent);
        let target = self.state.project_target(parent);
        let branch = self.state.projects[target].branch.clone();
        let button = Button::new(("project", parent))
            .custom(icon_style(p, cx))
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
            .accessibility_label(self.f("switch_project", &[("name", &name)]))
            .tooltip(self.state.projects[parent].path.clone())
            .relative()
            .when(self.project_attention(parent, cx), |button| {
                button
                    .accessibility_label(format!(
                        "{} · {}",
                        self.f("switch_project", &[("name", &name)]),
                        self.t("terminal_attention")
                    ))
                    .tooltip(format!(
                        "{} · {}",
                        self.state.projects[parent].path,
                        self.t("terminal_attention")
                    ))
                    .child(
                        div()
                            .absolute()
                            .top(px(3.0))
                            .right(px(3.0))
                            .size(px(6.0))
                            .rounded_full()
                            .bg(p.accent),
                    )
            })
            .child(if expanded {
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(icon(
                        IconName::Folder,
                        if active { p.accent } else { p.secondary },
                    ))
                    .child(div().flex_1().min_w_0().text_ellipsis().child(name))
                    .when(branch != "—", |row| {
                        row.child(
                            div()
                                .max_w(px(68.0))
                                .flex()
                                .items_center()
                                .gap(px(3.0))
                                .text_size(px(10.0))
                                .text_color(p.secondary)
                                .child(icon(IconName::GitBranch, p.secondary).size(px(11.0)))
                                .child(div().min_w_0().text_ellipsis().child(branch)),
                        )
                    })
            } else {
                div()
                    .size(px(28.0))
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
                    )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.change_project(this.state.project_target(parent), window, cx);
            }));
        let owner = cx.weak_entity();
        div()
            .id(("project-row", parent))
            .debug_selector(move || format!("project-row-{parent}"))
            .h(px(40.0))
            .w_full()
            .child(button)
            .context_menu(move |menu, _, cx| {
                if let Some(view) = owner.upgrade() {
                    view.read(cx).project_menu(parent, menu, owner.clone())
                } else {
                    menu
                }
            })
            .into_any_element()
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let expanded = self.state.sidebar_expanded;
        let projects = self.state.project_rows();
        let mut content = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(8.0))
            .py(px(12.0));
        if expanded {
            content = content.child(
                div()
                    .px(px(8.0))
                    .mb(px(8.0))
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.t("projects")),
            );
        }
        content = content.child(
            uniform_list(
                "project-list",
                projects.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| this.project_row(projects[index], expanded, cx))
                        .collect()
                }),
            )
            .size_full()
            .track_scroll(&self.navigation.sidebar_scroll),
        );
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
