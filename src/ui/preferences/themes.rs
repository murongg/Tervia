use super::*;

impl PreferencesWindow {
    pub(super) fn theme_control(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.theme);
        div()
            .flex()
            .w_full()
            .h(px(32.0))
            .p(px(2.0))
            .gap(px(2.0))
            .bg(p.raised)
            .rounded(px(7.0))
            .children(Mode::ALL.into_iter().map(|mode| {
                let selected = mode == self.current.theme.mode();
                let key = match mode {
                    Mode::Dark => "theme_mode_dark",
                    Mode::Light => "theme_mode_light",
                    Mode::System => "theme_system",
                };
                let symbol = match mode {
                    Mode::Dark => Some(IconName::Moon),
                    Mode::Light => Some(IconName::Sun),
                    Mode::System => None,
                };
                Button::new(format!("preferences-{}", mode.id()))
                    .debug_selector(move || format!("theme-mode-{}", mode.id()))
                    .ghost()
                    .small()
                    .flex_1()
                    .min_w_0()
                    .px(px(6.0))
                    .h(px(28.0))
                    .label(self.t(key))
                    .tooltip(self.t("theme_pair_hint"))
                    .when_some(symbol, |button, symbol| {
                        button.icon(icon(symbol, if selected { p.text } else { p.secondary }))
                    })
                    .toggled(selected)
                    .bg(if selected { p.background } else { p.raised })
                    .border_1()
                    .border_color(if selected { p.border } else { p.raised })
                    .rounded(px(5.0))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let light = super::system_is_light(window.appearance());
                        this.preferences.update(cx, |prefs, cx| {
                            prefs.theme.set_mode(mode);
                            prefs.theme.sync_system(light);
                            cx.notify();
                        });
                    }))
            }))
    }

    pub(super) fn theme_settings(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .pt(px(6.0))
            .pb(px(6.0))
            .children(
                [false, true]
                    .into_iter()
                    .map(|light| self.theme_group(light, cx)),
            )
    }

    fn theme_group(&self, light: bool, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.theme);
        let choices: Vec<_> = ColorTheme::ALL
            .into_iter()
            .filter(|theme| theme.is_light() == light)
            .collect();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(p.secondary)
                    .child(self.t(if light { "light_theme" } else { "dark_theme" })),
            )
            .children(choices.chunks(3).map(|row| {
                div()
                    .flex()
                    .w_full()
                    .gap(px(8.0))
                    .children(row.iter().map(|theme| self.theme_choice(*theme, cx)))
                    // Keep the last row's previews the same width as every other row.
                    .children((row.len()..3).map(|_| div().flex_1().min_w_0()))
            }))
    }

    fn theme_choice(&self, theme: ColorTheme, cx: &mut Context<Self>) -> Button {
        let p = Palette::new(self.current.theme);
        let selected = self.current.theme.selected(theme.is_light()) == theme;
        Button::new(format!("theme-choice-{}", theme.id()))
            .debug_selector(move || format!("theme-choice-{}", theme.id()))
            .ghost()
            .flex_1()
            .min_w_0()
            .h(px(44.0))
            .p(px(4.0))
            .rounded(px(5.0))
            .border_0()
            .bg(if selected {
                p.selected
            } else {
                p.background.opacity(0.0)
            })
            .accessibility_label(format!(
                "{} · {}",
                self.t(if theme.is_light() {
                    "light_theme"
                } else {
                    "dark_theme"
                }),
                theme.label(self.current.language)
            ))
            .tooltip(theme.label(self.current.language))
            .toggled(selected)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(thumbnail(theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .overflow_hidden()
                            .max_h(px(36.0))
                            .line_height(relative(1.15))
                            .text_size(px(13.0))
                            .text_color(p.text)
                            .child(theme.label(self.current.language)),
                    )
                    .child(
                        div()
                            .size(px(12.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |slot| {
                                slot.child(icon(IconName::Check, p.accent).size(px(12.0)))
                            }),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.preferences.update(cx, |prefs, cx| {
                    prefs.theme.select(theme);
                    cx.notify();
                });
            }))
    }
}

fn stroke(width: f32, color: Hsla) -> Div {
    div().w(px(width)).h(px(2.0)).rounded(px(1.0)).bg(color)
}

fn thumbnail(theme: ColorTheme) -> Div {
    let p = Palette::new(theme);
    // Use the real palette and window gradient so previews follow future theme edits.
    div()
        .debug_selector(move || format!("theme-preview-{}", theme.id()))
        .w(px(56.0))
        .h(px(36.0))
        .flex_shrink_0()
        .rounded(px(3.0))
        .border_1()
        .border_color(p.chrome_border())
        .overflow_hidden()
        .bg(super::super::theme::workspace_background(theme))
        .flex()
        .child(
            div()
                .w(px(8.0))
                .h_full()
                .flex_shrink_0()
                .border_r_1()
                .border_color(p.chrome_border())
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .pt(px(5.0))
                .child(div().size(px(4.0)).rounded(px(2.0)).bg(p.accent))
                .child(
                    div()
                        .size(px(3.0))
                        .rounded(px(1.0))
                        .bg(p.secondary.opacity(0.5)),
                )
                .child(
                    div()
                        .size(px(3.0))
                        .rounded(px(1.0))
                        .bg(p.secondary.opacity(0.5)),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(6.0))
                        .px(px(4.0))
                        .flex()
                        .items_center()
                        .border_b_1()
                        .border_color(p.chrome_border())
                        .child(stroke(14.0, p.secondary.opacity(0.5))),
                )
                .child(
                    div()
                        .flex_1()
                        .px(px(4.0))
                        .py(px(3.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(stroke(
                            18.0,
                            super::super::theme::syntax_color(theme, "keyword"),
                        ))
                        .child(div().pl(px(3.0)).child(stroke(
                            26.0,
                            super::super::theme::syntax_color(theme, "string"),
                        )))
                        .child(div().pl(px(3.0)).child(stroke(
                            16.0,
                            super::super::theme::syntax_color(theme, "number"),
                        ))),
                )
                .child(
                    div()
                        .h(px(8.0))
                        .px(px(4.0))
                        .border_t_1()
                        .border_color(p.chrome_border())
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .child(icon(IconName::ChevronRight, p.green).size(px(5.0)))
                        .child(stroke(16.0, p.text.opacity(0.7))),
                ),
        )
}
