use std::sync::Arc;

use gpui_kit::base::Disableable;

use super::*;

pub(super) type Action = Arc<dyn Fn(Option<bool>) -> Result<bool, String> + Send + Sync>;

impl PreferencesWindow {
    pub(super) fn configure_startup(&mut self, enabled: Option<bool>, cx: &mut Context<Self>) {
        if self.startup_busy {
            return;
        }
        self.startup_busy = true;
        self.startup_error = None;
        let action = self.startup_action.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let (result, actual) = cx
                .background_executor()
                .spawn(async move {
                    let result = action(enabled);
                    // Registration can fail after a partial OS write; re-read before displaying the switch.
                    let actual = if result.is_err() {
                        action(None).ok()
                    } else {
                        None
                    };
                    (result, actual)
                })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.startup_busy = false;
                match result {
                    Ok(enabled) => this.startup_enabled = Some(enabled),
                    Err(error) => {
                        this.startup_enabled = actual.or(this.startup_enabled);
                        this.startup_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn startup_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.theme);
        div()
            .flex()
            .flex_col()
            .child(self.heading("settings_startup"))
            .child(
                self.row(
                    "autostart",
                    "autostart-control",
                    div()
                        .debug_selector(|| "autostart-switch".into())
                        .flex()
                        .items_center()
                        .h_full()
                        .child(
                            Switch::new("autostart")
                                .checked(self.startup_enabled.unwrap_or(false))
                                .disabled(self.startup_busy || self.startup_enabled.is_none())
                                .accessibility_label(self.t("autostart"))
                                .on_change(cx.listener(|this, enabled, _, cx| {
                                    this.configure_startup(Some(*enabled), cx);
                                })),
                        ),
                ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.t(if self.startup_busy {
                        "autostart_busy"
                    } else {
                        "autostart_hint"
                    })),
            )
            .when_some(self.startup_error.clone(), |group, error| {
                group.child(
                    div()
                        .debug_selector(|| "autostart-error".into())
                        .mt(px(8.0))
                        .flex()
                        .items_start()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(11.0))
                                .text_color(p.red)
                                .child(format!("{} {error}", self.t("autostart_error"))),
                        )
                        .child(
                            label_button("autostart-retry", self.t("autostart_retry"), p)
                                .debug_selector(|| "autostart-retry".into())
                                .flex_shrink_0()
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.configure_startup(None, cx)),
                                ),
                        ),
                )
            })
    }
}
