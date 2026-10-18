use super::super::controls::icon;
use super::*;
use crate::ui::scroll::ListScrollbars as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{ButtonCustomVariant, ButtonVariants as _};

impl Tervia {
    pub(super) fn git_stashes(&self, snapshot: &Arc<Snapshot>, cx: &mut Context<Self>) -> Div {
        let project = self.state.active_project;
        let ui = &self.git_states[project];
        let p = self.git_p();
        let message = ui.first.read(cx).value().to_string();
        let folded = ui.collapsed_sections.contains("git_stash_list");
        let captured = snapshot.clone();
        let list = uniform_list(
            ("git-stash-rows", project),
            if folded { 0 } else { snapshot.stashes.len() },
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| this.git_stash_row(project, index, &captured.stashes[index], cx))
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .scrollbar(&ui.stash_scroll);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.background)
            .child(
                div()
                    .id("git-stash-section")
                    .h(px(36.0))
                    .w_full()
                    .flex_shrink_0()
                    .bg(p.toolbar)
                    .hover(|style| style.bg(p.raised))
                    .child(
                        label_button("git-stash-fold", self.t("git_stash"), p)
                            .text()
                            .h_full()
                            .w_full()
                            .rounded(px(0.0))
                            .icon(icon(
                                if folded {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                },
                                p.muted,
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let folded = &mut this.git_states[project].collapsed_sections;
                                if !folded.remove("git_stash_list") {
                                    folded.insert("git_stash_list");
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .p(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(Input::new(&ui.first).aria_label(self.t("git_stash_message")))
                    .child(
                        self.git_action(
                            "git-stash-save",
                            "git_stash_save",
                            Operation::StashPush(message),
                            cx,
                        )
                        .icon(icon(IconName::Archive, p.secondary))
                        .disabled(
                            ui.busy
                                || self.git_mutating
                                || snapshot.status.files.is_empty()
                                || snapshot.status.operation.is_some(),
                        ),
                    ),
            )
            .when(snapshot.stashes.is_empty(), |view| {
                view.child(
                    div()
                        .p(px(16.0))
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .child(self.t("git_no_stashes")),
                )
            })
            .child(list)
    }

    fn git_stash_row(
        &self,
        project: usize,
        index: usize,
        stash: &tervia::git::Stash,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let ui = &self.git_states[project];
        let p = self.git_p();
        let reference = stash.hash.clone();
        #[cfg(all(test, feature = "ui-tests"))]
        ui.rendered_rows.set(ui.rendered_rows.get() + 1);
        let selected = matches!(&ui.selected,Some(Detail::Stash(hash))if hash==&reference);
        div()
            .id(("git-stash-item", index))
            .debug_selector(move || format!("git-stash-item-{index}"))
            .w_full()
            .h(px(96.0))
            .flex()
            .flex_col()
            .px(px(8.0))
            .py(px(8.0))
            .gap(px(3.0))
            .bg(if selected { p.selected } else { p.background })
            .hover(|style| style.bg(p.raised))
            .border_b_1()
            .border_color(p.border)
            .child(
                label_button(("git-stash-row", index), "", p)
                    .debug_selector(move || format!("git-stash-row-{index}"))
                    // Keep one hover surface across the content, actions, and outer padding.
                    .custom(ButtonCustomVariant::new(cx).foreground(p.secondary))
                    .h(px(48.0))
                    .w_full()
                    .accessibility_label(format!("{} {}", stash.reference, stash.message))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(icon(IconName::Archive, p.accent))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(p.muted)
                                            .child(stash.reference.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.text)
                                    .text_ellipsis()
                                    .child(stash.message.clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.git_detail(project, Detail::Stash(reference.clone()), window, cx)
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .child(
                        self.git_action_for(
                            project,
                            ("git-stash-apply", index),
                            "git_stash_apply",
                            Operation::StashApply(stash.hash.clone()),
                            cx,
                        )
                        .debug_selector(move || format!("git-stash-apply-{index}")),
                    )
                    .child(self.git_action_for(
                        project,
                        ("git-stash-pop", index),
                        "git_stash_pop",
                        Operation::StashPop {
                            reference: stash.reference.clone(),
                            expected: stash.hash.clone(),
                        },
                        cx,
                    ))
                    .child(
                        self.git_action_for(
                            project,
                            ("git-stash-drop", index),
                            "git_delete",
                            Operation::StashDrop {
                                reference: stash.reference.clone(),
                                expected: stash.hash.clone(),
                            },
                            cx,
                        )
                        .text_color(p.muted),
                    ),
            )
    }
}
