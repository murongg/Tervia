use super::super::{
    controls::{icon, icon_button, separator},
    theme::Palette,
};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::resizable::{
    h_resizable, resizable_panel, resize_handle_appearance_without_line, v_resizable,
};
use gpui_kit::component::scroll::ScrollableElement as _;

impl Tervia {
    pub(super) fn git_p(&self) -> Palette {
        Palette::workbench(&self.appearance.theme)
    }

    pub(crate) fn git(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.git_p();
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let mut body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.background)
            .text_color(p.text)
            .border_t_1()
            .border_color(p.border);
        let Some(snapshot) = &ui.snapshot else {
            let ordinary = ui.plain_directory;
            let failed = ui.repository.is_some() || (!ordinary && !ui.error.is_empty());
            return body.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(14.0))
                    .child(icon(IconName::FolderGit2, p.muted))
                    .child(
                        div()
                            .id(if failed {
                                "git-failure-heading"
                            } else if ordinary {
                                "git-empty-heading"
                            } else {
                                "git-pending-heading"
                            })
                            .debug_selector(move || {
                                if failed {
                                    "git-failure-heading".into()
                                } else if ordinary {
                                    "git-empty-heading".into()
                                } else {
                                    "git-pending-heading".into()
                                }
                            })
                            .child(self.t(if ui.busy {
                                "git_running"
                            } else if failed {
                                "git_refresh_failed"
                            } else if ordinary {
                                "git_not_repository"
                            } else {
                                "git_check_status"
                            })),
                    )
                    .when(
                        ordinary
                            && self
                                .state
                                .folder(project)
                                .is_some_and(|root| !self.state.folder_children(root).is_empty()),
                        |body| {
                            body.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.secondary)
                                    .child(self.t("git_select_repository")),
                            )
                        },
                    )
                    .when(!ui.error.is_empty(), |body| {
                        body.child(
                            div()
                                .w_full()
                                .max_w(px(720.0))
                                .child(self.git_error(project, cx)),
                        )
                    })
                    .when(!ordinary, |body| {
                        body.child(
                            label_button("git-retry", self.t("refresh_git"), p)
                                .debug_selector(|| "git-retry".into())
                                .accessibility_label(self.t("refresh_git"))
                                .disabled(ui.busy || self.git_mutating)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.refresh_git(project, window, cx)
                                })),
                        )
                    })
                    .when(ordinary, |body| {
                        body.child(
                            label_button("git-init", self.t("git_init"), p)
                                .debug_selector(|| "git-init".into())
                                .disabled(ui.busy || self.git_mutating)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.git_job(project, None, true, window, cx)
                                })),
                        )
                        .child(
                            Input::new(&ui.first)
                                .aria_label(self.t("git_clone_url"))
                                .w(px(380.0)),
                        )
                        .child(
                            label_button("git-clone", self.t("git_clone"), p)
                                .debug_selector(|| "git-clone".into())
                                .disabled(ui.busy || self.git_mutating)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.clone_repository(project, window, cx)
                                })),
                        )
                    }),
            );
        };
        let status = &snapshot.status;
        let mut toolbar = div()
            .h(px(44.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(5.0))
            .px(px(12.0))
            .bg(p.toolbar)
            .border_b_1()
            .border_color(p.border)
            .child(self.branch_switcher(SwitchAnchor::Toolbar, cx))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.muted)
                    .child(format!("↑ {}  ↓ {}", status.ahead, status.behind)),
            )
            .child(div().flex_1());
        for (id, key, glyph, operation) in [
            (
                "git-fetch",
                "git_fetch",
                IconName::RefreshCw,
                Operation::Fetch,
            ),
            (
                "git-pull",
                "git_pull",
                IconName::ArrowDownToLine,
                Operation::Pull,
            ),
        ] {
            toolbar = toolbar.child(
                self.git_action(id, key, operation, cx)
                    .icon(icon(glyph, p.secondary)),
            );
        }
        if let Some(remote) = &snapshot.push_remote {
            toolbar = toolbar.child(
                self.git_action(
                    "git-push",
                    "git_push",
                    Operation::Push {
                        remote: remote.clone(),
                        set_upstream: status.upstream.is_none(),
                    },
                    cx,
                )
                .icon(icon(IconName::ArrowUpFromLine, p.secondary)),
            );
        }
        toolbar = toolbar
            .child(separator(p))
            .when(ui.busy && self.git_mutating, |toolbar| {
                toolbar.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.accent)
                        .child(self.t("git_running")),
                )
            })
            .child(
                icon_button(
                    "git-refresh",
                    IconName::RotateCw,
                    self.t("refresh_git"),
                    p,
                    cx,
                )
                .accessibility_label(self.t("refresh_git"))
                .disabled(ui.busy || self.git_mutating)
                .on_click(
                    cx.listener(move |this, _, window, cx| this.refresh_git(project, window, cx)),
                ),
            );
        body = body.child(toolbar);
        // Background polling never inserts a loading row: stable geometry matters while reviewing a diff.
        if !ui.error.is_empty() {
            body = body.child(self.git_error(project, cx));
        }
        if let Some(operation) = &status.operation {
            body = body.child(
                div()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .bg(p.selected)
                    .child(icon(IconName::GitMerge, p.accent))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .child(format!("{} · {operation}", self.t("git_in_progress"))),
                    )
                    .child(div().flex_1())
                    .child(
                        self.git_action("git-continue", "git_continue", Operation::Continue, cx)
                            .disabled(
                                ui.busy
                                    || self.git_mutating
                                    || status.files.iter().any(|file| file.conflicted),
                            ),
                    )
                    .child(self.git_action("git-abort", "git_abort", Operation::Abort, cx)),
            );
        }
        let focused =
            ui.review.focused && matches!(ui.mode, Mode::Changes | Mode::History | Mode::Stash);
        let content = if focused {
            self.git_viewer(cx).into_any_element()
        } else {
            match ui.mode {
                Mode::Changes => h_resizable(("git-changes-layout", project))
                    .child(
                        resizable_panel()
                            .size(px(260.0))
                            .size_range(px(200.0)..px(480.0))
                            .flex_none()
                            .child(self.git_changes(snapshot, cx)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(220.0)..Pixels::MAX)
                            .child(self.git_viewer(cx)),
                    )
                    .into_any_element(),
                Mode::History => v_resizable(("git-history-layout", project))
                    .child(
                        resizable_panel()
                            .size(px(250.0))
                            .size_range(px(135.0)..px(500.0))
                            .flex_none()
                            .child(self.git_history(snapshot, cx)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(150.0)..Pixels::MAX)
                            .child(self.git_viewer(cx)),
                    )
                    .into_any_element(),
                Mode::Stash => h_resizable(("git-stash-layout", project))
                    .child(
                        resizable_panel()
                            .size(px(300.0))
                            .size_range(px(240.0)..px(500.0))
                            .flex_none()
                            .child(self.git_stashes(snapshot, cx)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(220.0)..Pixels::MAX)
                            .child(self.git_viewer(cx)),
                    )
                    .into_any_element(),
                Mode::Remotes => self.git_remotes(snapshot, cx).into_any_element(),
                Mode::Worktrees => self.git_worktrees(snapshot, cx).into_any_element(),
            }
        };
        let content = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(content);
        body.child(div().flex_1().min_w_0().min_h_0().flex().map(|body| {
            if focused {
                body.child(content)
            } else {
                body.child(
                    h_resizable(("git-sidebar-layout", project))
                        // The sidebar paints the workbench hairline; keep only the native engagement indicator.
                        .with_handle_appearance(resize_handle_appearance_without_line())
                        // Keep the split alive while focus mode or project navigation hides it.
                        .with_state(&ui.sidebar_split)
                        .child(
                            resizable_panel()
                                .size(px(174.0))
                                // Localized worktree badges still need room for a branch name.
                                .size_range(px(174.0)..px(400.0))
                                .flex_none()
                                .child(self.git_sidebar(snapshot, cx)),
                        )
                        .child(
                            resizable_panel()
                                .size_range(px(420.0)..Pixels::MAX)
                                .child(content),
                        ),
                )
            }
        }))
        .child(
            div()
                .h(px(24.0))
                .flex_shrink_0()
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_t_1()
                .border_color(p.border)
                .bg(p.toolbar)
                .text_size(px(11.0))
                .text_color(p.muted)
                .child(icon(IconName::GitCommitHorizontal, p.muted))
                .child(
                    status
                        .upstream
                        .as_deref()
                        .unwrap_or(self.t("git_no_upstream"))
                        .to_owned(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .child(ui.output.lines().next().unwrap_or_default().to_owned()),
                ),
        )
    }

    pub(super) fn git_list(
        &self,
        id: &'static str,
        _width: f32,
    ) -> gpui_kit::component::scroll::Scrollable<Stateful<Div>> {
        div()
            .id(id)
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .bg(self.git_p().background)
    }

    pub(super) fn git_panel_title(&self, title: &'static str) -> Div {
        let p = self.git_p();
        div()
            .h(px(36.0))
            .flex_shrink_0()
            .px(px(12.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .bg(p.toolbar)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .child(self.t(title))
    }
}
