use super::{
    Tervia,
    controls::{icon, icon_style},
};
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;

mod lifecycle;
mod navigation;
mod picker;
mod sidebar;
mod switcher;
pub(super) use navigation::Navigation;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;

gpui_kit::actions!(
    projects,
    [
        SwitchWorkingDirectory,
        QuickSwitchProject,
        PreviousProject,
        ReopenProject,
        CloseProject
    ]
);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-alt-o", SwitchWorkingDirectory, Some("Tervia")),
        KeyBinding::new("secondary-alt-p", QuickSwitchProject, Some("Tervia")),
        KeyBinding::new(
            "secondary-alt-p",
            QuickSwitchProject,
            Some("Tervia > Console"),
        ),
    ]);
}

impl Tervia {
    fn project_menu(&self, parent: usize, menu: PopupMenu, view: WeakEntity<Self>) -> PopupMenu {
        let opening = view.clone();
        let scanning = view.clone();
        let p = self.p();
        let favorite = view.clone();
        let closing = view.clone();
        let copying = view.clone();
        let root = self.state.project_root(parent).clone();
        let reveal = root.clone();
        let rows: Vec<_> = self
            .state
            .project_rows()
            .into_iter()
            .filter(|index| {
                self.state.project_favorite(*index) == self.state.project_favorite(parent)
            })
            .collect();
        let position = rows.iter().position(|index| *index == parent).unwrap_or(0);
        let mut menu = menu
            .item(
                PopupMenuItem::new(self.t(if self.state.project_favorite(parent) {
                    "project_unfavorite"
                } else {
                    "project_favorite"
                }))
                .icon(icon(IconName::Star, p.secondary))
                .on_click(move |_, _, cx| {
                    let _ = favorite.update(cx, |this, cx| {
                        this.state.toggle_project_favorite(parent);
                        this.persist_workspace(cx);
                        cx.notify();
                    });
                }),
            )
            .item(
                PopupMenuItem::new(self.t("project_copy_path")).on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        root.to_string_lossy().into_owned(),
                    ));
                }),
            )
            .item(
                PopupMenuItem::new(self.t("project_reveal"))
                    .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
            )
            .separator()
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
                        let _ = scanning
                            .update(cx, |this, cx| this.rescan_repositories(parent, window, cx));
                    }),
            );
        for notice in self.scan_notices(parent) {
            menu = menu.label(notice);
        }
        menu = menu.separator();
        for (label, target) in [
            (
                "project_move_up",
                position
                    .checked_sub(1)
                    .and_then(|position| rows.get(position).copied()),
            ),
            ("project_move_down", rows.get(position + 1).copied()),
        ] {
            let owner = copying.clone();
            menu = menu.item(
                PopupMenuItem::new(self.t(label))
                    .disabled(target.is_none())
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if let Some(target) = target {
                                this.move_sidebar_project(parent, target, cx);
                            }
                        });
                    }),
            );
        }
        menu = menu.separator().item(
            PopupMenuItem::new(self.t("project_recent_closed")).on_click(move |_, window, cx| {
                let _ = closing.update(cx, |this, cx| this.show_project_switcher(true, window, cx));
            }),
        );
        let closing = view.clone();
        menu = menu.item(
            PopupMenuItem::new(self.t("project_close"))
                .disabled(
                    self.navigation.closing
                        || self.git_mutating
                        || self.saving_file
                        || self.file_closing,
                )
                .on_click(move |_, window, cx| {
                    let _ = closing.update(cx, |this, cx| {
                        this.request_project_close(parent, window, cx)
                    });
                }),
        );
        menu
    }

    fn move_sidebar_project(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if self.state.move_project(from, to) {
            self.persist_workspace(cx);
            cx.notify();
        }
    }
}
