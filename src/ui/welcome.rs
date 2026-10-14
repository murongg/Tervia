use super::{
    OpenPreferences, OpenProject, Tervia, ToggleTheme,
    controls::{icon, label_button, titlebar_mouse_down},
    theme,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::actions::Cancel;
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants},
    input::{Escape, Input},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;

impl Tervia {
    pub(super) fn welcome(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = theme::Palette::workbench(self.appearance.light_theme);
        let update = self.workspace_update(cx);
        let introduction = div()
            .w(px(232.0))
            .flex_shrink_0()
            .p(px(24.0))
            .bg(p.sidebar)
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        img(Arc::new(Image::from_bytes(
                            ImageFormat::Svg,
                            include_bytes!("../../assets/tervia.svg").to_vec(),
                        )))
                        .size(px(40.0))
                        .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Tervia"),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(p.secondary)
                    .child(self.t("git_welcome_hint")),
            )
            .child(div().flex_1())
            .child(
                label_button("welcome-settings", self.t("settings"), p)
                    .icon(icon(IconName::Settings, p.muted))
                    .on_click(cx.listener(|this, _, _, cx| this.open_preferences(cx))),
            );
        let actions =
            div()
                .flex_1()
                .min_w_0()
                .p(px(24.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    Button::new("welcome-open")
                        .primary()
                        .bg(p.accent)
                        .text_color(p.on_accent)
                        .label(self.t("git_open_repository"))
                        .icon(icon(IconName::FolderOpen, p.on_accent))
                        .w_full()
                        .h(px(32.0))
                        .disabled(self.git_mutating)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.choose_repository(window, cx)),
                        ),
                )
                .child(div().h(px(1.0)).bg(p.border).my(px(8.0)))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(self.t("git_clone_title")),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.secondary)
                        .child(self.t("git_clone_url")),
                )
                .child(Input::new(&self.welcome_url).aria_label(self.t("git_clone_url")))
                .child(
                    label_button("welcome-clone", self.t("git_clone"), p)
                        .h(px(32.0))
                        .border_1()
                        .border_color(p.border)
                        .w_full()
                        .disabled(
                            self.git_mutating
                                || self.welcome_url.read(cx).value().trim().is_empty(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            let url = this.welcome_url.read(cx).value().to_string();
                            this.clone_url(None, url, window, cx);
                        })),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.secondary)
                        .child(self.t("git_clone_hint")),
                )
                .child(div().text_size(px(12.0)).text_color(p.secondary).child(
                    if self.git_mutating {
                        self.t("git_running").to_owned()
                    } else {
                        self.state.notice.clone()
                    },
                ));
        div()
            .id("tervia-welcome")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context("Tervia")
            .bg(p.background.alpha(1.0))
            .text_color(p.text)
            .font_family(tervia::platform::Platform::CURRENT.ui_font())
            .on_action(cx.listener(|this, _: &OpenPreferences, _, cx| this.open_preferences(cx)))
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.choose_repository(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleTheme, window, cx| this.toggle_theme(window, cx)),
            )
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.dismiss(window, cx)))
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px(px(16.0))
                    .text_size(px(12.0))
                    .child(div().w(px(72.0)))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("Tervia")
                            .on_mouse_down(MouseButton::Left, titlebar_mouse_down),
                    )
                    .child(
                        div()
                            .w(px(72.0))
                            .flex()
                            .justify_end()
                            .when_some(update, |slot, update| slot.child(update)),
                    ),
            )
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    div()
                        .w(px(608.0))
                        .flex()
                        .rounded(px(8.0))
                        .overflow_hidden()
                        // An opaque surface keeps the shadow from showing through the card.
                        .bg(p.toolbar.alpha(1.0))
                        .shadow(vec![
                            BoxShadow::new(
                                px(0.0),
                                px(4.0),
                                hsla(
                                    0.0,
                                    0.0,
                                    0.0,
                                    if self.appearance.light_theme {
                                        0.06
                                    } else {
                                        0.12
                                    },
                                ),
                            )
                            .blur_radius(px(16.0)),
                        ])
                        .child(introduction)
                        .child(actions),
                ),
            )
    }
}
