use super::*;

impl PreferencesWindow {
    pub(super) fn general_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(self.row("theme", "theme-control", self.theme_control(cx)))
                    .child(
                        self.row(
                            "language",
                            "language-control",
                            Select::new(&self.language_select)
                                .w_full()
                                .accessibility_label(self.t("language")),
                        ),
                    )
                    .child(self.row("ui_size", "ui-size-control", self.size_control(false, cx))),
            )
            .child(div().h(px(1.0)).bg(p.border))
            .child(self.startup_settings(cx))
    }

    pub(super) fn workspace_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .child(self.font_settings(cx))
            .child(div().h(px(1.0)).bg(p.border))
            .child(self.file_settings(cx))
            .child(div().h(px(1.0)).bg(p.border))
            .child(self.terminal_settings(cx))
    }

    fn font_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .flex()
            .flex_col()
            .child(self.heading("settings_fonts"))
            .child(
                self.row(
                    "code_font",
                    "font-control",
                    Select::new(&self.font_select)
                        .w_full()
                        .accessibility_label(self.t("code_font")),
                ),
            )
            .child(self.row(
                "code_size",
                "code-size-control",
                self.size_control(true, cx),
            ))
            .child(
                div()
                    .mt(px(12.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(p.raised.opacity(0.5))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.secondary)
                            .child(self.t("preview")),
                    )
                    .child(
                        div()
                            .overflow_x_hidden()
                            .font_family(self.current.font_family.clone())
                            .text_size(px(self.current.code_size as f32))
                            .line_height(relative(1.4))
                            .child("Aa Bb 012345  {}"),
                    ),
            )
    }

    fn file_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .flex()
            .flex_col()
            .child(self.heading("settings_files"))
            .child(
                self.row(
                    "file_nesting",
                    "file-nesting-control",
                    div()
                        .debug_selector(|| "file-nesting-switch".into())
                        .flex()
                        .items_center()
                        .h_full()
                        .child(
                            Switch::new("file-nesting")
                                .checked(self.current.file_nesting)
                                .accessibility_label(self.t("file_nesting"))
                                .on_change(cx.listener(|this, enabled, _, cx| {
                                    this.preferences.update(cx, |prefs, cx| {
                                        prefs.file_nesting = *enabled;
                                        cx.notify();
                                    });
                                })),
                        ),
                ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.t("file_nesting_hint")),
            )
    }

    fn terminal_settings(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(self.current.light_theme);
        div()
            .flex()
            .flex_col()
            .child(self.heading("terminal"))
            .when(cfg!(windows), |settings| {
                settings
                    .child(
                        self.row(
                            "default_shell",
                            "shell-control",
                            Select::new(&self.shell_select)
                                .w_full()
                                .accessibility_label(self.t("default_shell")),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.secondary)
                            .child(self.t("default_shell_hint")),
                    )
            })
            .child(
                self.row(
                    "command_suggestions",
                    "command-suggestions-control",
                    div()
                        .debug_selector(|| "command-suggestions-switch".into())
                        .flex()
                        .items_center()
                        .h_full()
                        .child(
                            Switch::new("command-suggestions")
                                .checked(self.current.command_suggestions)
                                .accessibility_label(self.t("command_suggestions"))
                                .on_change(cx.listener(|this, enabled, _, cx| {
                                    this.preferences.update(cx, |prefs, cx| {
                                        prefs.command_suggestions = *enabled;
                                        cx.notify();
                                    });
                                })),
                        ),
                ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.secondary)
                    .child(self.t("command_suggestions_hint")),
            )
    }
}
