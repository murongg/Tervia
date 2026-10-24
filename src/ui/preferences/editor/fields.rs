use super::*;

pub(super) const SYNTAX_KEYS: &[&str] = &[
    "syntax.keyword",
    "syntax.string",
    "syntax.number",
    "syntax.comment",
    "syntax.function",
    "syntax.type",
];
const BASIC: &[&str] = &[
    "ui.workspace_start",
    "ui.workspace_end",
    "ui.text",
    "ui.secondary",
    "ui.workspace_accent",
    "ui.workspace_selected",
    "ui.workspace_border",
];

impl Editor {
    pub(super) fn fields(&self, cx: &mut Context<Self>) -> Div {
        let p = self.chrome;
        let keys: Vec<&str> = match self.tab {
            0 => BASIC.to_vec(),
            1 => SYNTAX_KEYS.to_vec(),
            _ => vec![
                "terminal.background",
                "terminal.foreground",
                "terminal.cursor",
            ],
        };
        div()
            .w(px(364.0))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .px(px(20.0))
                    .py(px(10.0))
                    .children(
                        ["theme_tab_ui", "theme_tab_code", "theme_tab_terminal"]
                            .into_iter()
                            .enumerate()
                            .map(|(tab, key)| {
                                label_button(format!("theme-editor-tab-{tab}"), self.t(key), p, cx)
                                    .flex_1()
                                    .toggled(self.tab == tab)
                                    .bg(if self.tab == tab {
                                        p.selected
                                    } else {
                                        p.background.opacity(0.0)
                                    })
                                    .text_color(if self.tab == tab { p.text } else { p.secondary })
                                    .debug_selector(move || format!("theme-editor-tab-{tab}"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.tab = tab;
                                        this.advanced = false;
                                        this.scroll.set_offset(point(px(0.0), px(0.0)));
                                        cx.notify();
                                    }))
                            }),
                    ),
            )
            .child(
                div()
                    .id("theme-color-fields")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(20.0))
                    .pb(px(20.0))
                    .children(keys.into_iter().map(|key| self.color_field(key, cx)))
                    .when(self.tab != 1, |body| {
                        body.child(
                            label_button(
                                "theme-editor-advanced",
                                self.t(if self.advanced {
                                    "theme_advanced_hide"
                                } else {
                                    "theme_advanced"
                                }),
                                p,
                                cx,
                            )
                            .mt(px(12.0))
                            .w_full()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advanced = !this.advanced;
                                cx.notify();
                            })),
                        )
                    })
                    .when(self.advanced, |body| {
                        let prefix = if self.tab == 0 {
                            "ui."
                        } else {
                            "terminal.ansi"
                        };
                        body.children(
                            tervia::themes::COLOR_KEYS
                                .iter()
                                .copied()
                                .filter(|key| key.starts_with(prefix) && !BASIC.contains(key))
                                .map(|key| self.color_field(key, cx)),
                        )
                    }),
            )
    }

    fn color_field(&self, key: &'static str, cx: &mut Context<Self>) -> Div {
        let p = self.chrome;
        let control = &self.controls[key];
        let invalid = self.draft.invalid(key);
        let picker = if self.saving {
            div()
                .size(px(28.0))
                .rounded(px(4.0))
                .bg(super::super::super::theme::color(self.draft.data(), key))
                .opacity(0.5)
                .into_any_element()
        } else {
            ColorPicker::new(&control.picker)
                .small()
                .accessibility_label(
                    self.language
                        .text(&format!("theme_color_{}", key.replace('.', "_")))
                        .to_owned(),
                )
                .into_any_element()
        };
        let label = self
            .language
            .text(&format!("theme_color_{}", key.replace('.', "_")))
            .to_owned();
        div()
            .flex()
            .flex_col()
            .py(px(5.0))
            .child(
                div()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(label.clone()),
                    )
                    .child(picker)
                    .child(
                        Input::new(&control.input)
                            .small()
                            .w(px(104.0))
                            .disabled(self.saving)
                            .aria_label(label.clone()),
                    )
                    .child(
                        icon_button(
                            format!("theme-reset-{key}"),
                            IconName::RotateCcw,
                            self.t("theme_reset_color"),
                            p,
                            cx,
                        )
                        .tooltip(self.t("theme_reset_color"))
                        .disabled(self.saving)
                        .accessibility_label(format!("{} · {}", self.t("theme_reset_color"), label))
                        .on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.reset_color(key, window, cx)
                            }),
                        ),
                    ),
            )
            .when(invalid, |row| {
                row.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.red)
                        .child(self.t("theme_invalid_color")),
                )
            })
    }
}
