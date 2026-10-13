use super::super::controls::icon;
use super::*;
use gpui_kit::assets::IconName;

impl Tervia {
    pub(super) fn git_remotes(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> Stateful<Div> {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let name = ui.first.read(cx).value().to_string();
        let url = ui.second.read(cx).value().to_string();
        let mut list = self
            .git_list("real-git-remotes", 0.0)
            .child(self.git_panel_title("git_remotes"));
        for (index, remote) in snapshot.remotes.iter().enumerate() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .py(px(14.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(icon(IconName::Globe, p.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(remote.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.muted)
                                    .child(remote.url.clone()),
                            ),
                    )
                    .child(self.git_action(
                        ("git-push-remote", index),
                        "git_push",
                        Operation::Push {
                            remote: remote.name.clone(),
                            set_upstream: snapshot.status.upstream.is_none(),
                        },
                        cx,
                    ))
                    .child(
                        self.git_action(
                            ("git-remote-url", index),
                            "git_update_url",
                            Operation::SetRemoteUrl {
                                name: remote.name.clone(),
                                url: url.clone(),
                            },
                            cx,
                        )
                        .disabled(ui.busy || self.git_mutating || url.is_empty()),
                    )
                    .child(
                        self.git_action(
                            ("git-remove-remote", index),
                            "git_delete",
                            Operation::RemoveRemote(remote.name.clone()),
                            cx,
                        )
                        .text_color(p.muted),
                    ),
            );
        }
        list = list.child(
            div()
                .max_w(px(720.0))
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(self.t("git_add_remote")),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(10.0))
                        .child(
                            Input::new(&ui.first)
                                .aria_label(self.t("git_remote_name"))
                                .w(px(170.0)),
                        )
                        .child(
                            Input::new(&ui.second)
                                .aria_label(self.t("git_remote_url"))
                                .flex_1(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(
                            self.git_action(
                                "git-add-remote",
                                "git_add_remote",
                                Operation::AddRemote { name, url },
                                cx,
                            )
                            .disabled(
                                ui.busy
                                    || self.git_mutating
                                    || ui.first.read(cx).value().is_empty()
                                    || ui.second.read(cx).value().is_empty(),
                            ),
                        )
                        .child(
                            label_button("git-clone-remote-url", self.t("git_clone"), p)
                                .disabled(
                                    ui.busy
                                        || self.git_mutating
                                        || ui.second.read(cx).value().is_empty(),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let url = this.git_states[project]
                                        .second
                                        .read(cx)
                                        .value()
                                        .to_string();
                                    this.clone_url(Some(project), url, window, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(self.t("git_pull_strategy")),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(self.git_action(
                            "git-pull-merge",
                            "git_pull_merge",
                            Operation::PullMerge,
                            cx,
                        ))
                        .child(self.git_action(
                            "git-pull-rebase",
                            "git_pull_rebase",
                            Operation::PullRebase,
                            cx,
                        )),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .child(self.t("git_auth_hint")),
                ),
        );
        list
    }
}
