use super::*;
use gpui_kit::component::menu::PopupMenuItem;

impl Tervia {
    pub(in crate::ui::git) fn worktree_menu_items(
        &self,
        project: usize,
        tree: &Worktree,
        view: WeakEntity<Self>,
    ) -> Vec<PopupMenuItem> {
        let p = self.git_p();
        let current = tree.path == self.state.projects[project].root;
        let available = !tree.bare && !tree.missing && tree.prunable.is_none();
        let busy = self.worktree_busy(project);
        // Actions retain the clicked directory even if the active directory changes while the menu is open.
        let opening = view.clone();
        let target = tree.clone();
        let path = tree.path.clone();
        let reveal = path.clone();
        let mut items = vec![
            PopupMenuItem::label(self.worktree_caption(tree)),
            PopupMenuItem::new(self.t("worktree_open"))
                .icon(icon(IconName::FolderOpen, p.secondary))
                .disabled(current || !available || busy)
                .on_click(move |_, window, cx| {
                    let _ = opening.update(cx, |this, cx| {
                        this.open_worktree(project, target.clone(), window, cx);
                    });
                }),
            PopupMenuItem::separator(),
            PopupMenuItem::new(self.t("worktree_copy_path"))
                .icon(icon(IconName::Copy, p.secondary))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        path.to_string_lossy().into_owned(),
                    ));
                }),
            PopupMenuItem::new(self.t("project_reveal"))
                .icon(icon(IconName::Folder, p.secondary))
                .disabled(!available)
                .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
        ];
        if !tree.main && !tree.bare {
            let locking = view.clone();
            let target = tree.clone();
            let locked = tree.locked.is_some();
            items.push(PopupMenuItem::separator());
            items.push(
                PopupMenuItem::new(self.t(if locked {
                    "worktree_unlock"
                } else {
                    "worktree_lock"
                }))
                .icon(icon(
                    if locked {
                        IconName::LockOpen
                    } else {
                        IconName::Lock
                    },
                    p.secondary,
                ))
                .disabled(busy)
                .on_click(move |_, window, cx| {
                    let _ = locking.update(cx, |this, cx| {
                        let operation = if locked {
                            WorktreeOperation::Unlock(target.clone())
                        } else {
                            WorktreeOperation::Lock {
                                tree: target.clone(),
                                reason: String::new(),
                            }
                        };
                        this.request_worktree_operation(project, operation, window, cx);
                    });
                }),
            );
        }
        let removable = self.worktree_removable(project, tree);
        let target = tree.clone();
        items.push(PopupMenuItem::separator());
        items.push(
            PopupMenuItem::new(self.t("worktree_remove"))
                .icon(icon(IconName::Trash, p.red))
                .disabled(!removable)
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.request_worktree_operation(
                            project,
                            WorktreeOperation::Remove(target.clone()),
                            window,
                            cx,
                        );
                    });
                }),
        );
        items
    }
}
