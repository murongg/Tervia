use super::*;
use gpui_kit::component::{
    menu::{DropdownMenu as _, PopupMenuItem},
    switch::Switch,
};
use tervia::locale::Language;

fn interval_label(language: Language, minutes: u16) -> String {
    if minutes.is_multiple_of(1440) {
        language.format(
            "update_interval_days",
            &[("days", &(minutes / 1440).to_string())],
        )
    } else if minutes.is_multiple_of(60) {
        language.format(
            "update_interval_hours",
            &[("hours", &(minutes / 60).to_string())],
        )
    } else {
        language.format(
            "update_interval_minutes",
            &[("minutes", &minutes.to_string())],
        )
    }
}

impl Updater {
    pub(super) fn options(&self, cx: &mut Context<Self>) -> Div {
        let preferences = self.preferences.read(cx);
        let language = preferences.language;
        let settings = preferences.updates;
        let p = Palette::new(&preferences.theme);
        let shared_preferences = self.preferences.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .border_t_1()
            .border_color(p.border)
            .pt(px(24.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(language.text("update_auto_check"))
                    .child(
                        div()
                            .debug_selector(|| "updates-auto-toggle".into())
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .child(
                                Switch::new("automatic-update-checks")
                                    .checked(settings.auto_check)
                                    .accessibility_label(language.text("update_auto_check"))
                                    .on_change(cx.listener(|this, enabled, _, cx| {
                                        this.preferences.update(cx, |preferences, cx| {
                                            preferences.updates.auto_check = *enabled;
                                            cx.notify();
                                        });
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(language.text("update_interval"))
                    .child(
                        div()
                            .debug_selector(|| "updates-interval-control".into())
                            .child(
                                label_button(
                                    "update-check-interval",
                                    &interval_label(language, settings.interval_minutes),
                                    p,
                                )
                                .outline()
                                .h(px(32.0))
                                .px(px(12.0))
                                .icon(icon(IconName::ChevronDown, p.secondary))
                                .accessibility_label(language.text("update_interval"))
                                .dropdown_menu(
                                    move |mut menu, _, _| {
                                        let mut intervals = vec![
                                            5,
                                            15,
                                            30,
                                            60,
                                            180,
                                            360,
                                            720,
                                            1440,
                                            10080,
                                            settings.interval_minutes,
                                        ];
                                        intervals.sort_unstable();
                                        intervals.dedup();
                                        for minutes in intervals {
                                            let preferences = shared_preferences.clone();
                                            menu = menu.item(
                                                PopupMenuItem::new(interval_label(
                                                    language, minutes,
                                                ))
                                                .checked(minutes == settings.interval_minutes)
                                                .on_click(move |_, _, cx| {
                                                    preferences.update(cx, |preferences, cx| {
                                                        preferences.updates.interval_minutes =
                                                            minutes;
                                                        cx.notify();
                                                    });
                                                }),
                                            );
                                        }
                                        menu
                                    },
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(p.secondary)
                    .child(language.text(if settings.auto_check {
                        "update_auto_hint"
                    } else {
                        "update_manual_hint"
                    })),
            )
    }
}
