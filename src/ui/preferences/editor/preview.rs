use super::*;

impl Editor {
    pub(super) fn preview(&self) -> Div {
        let theme = ColorTheme::Custom(Arc::new(self.draft.data().clone()));
        let p = Palette::workbench(&theme);
        let ink = |key: &str| super::super::super::theme::color(self.draft.data(), key);
        let syntax = |capture: &str| super::super::super::theme::syntax_color(&theme, capture);
        let code_line = |parts: Vec<(&'static str, Hsla)>| {
            div().flex().h(px(24.0)).children(
                parts
                    .into_iter()
                    .map(|(text, color)| div().text_color(color).child(text)),
            )
        };
        let terminal = div()
            .h(px(130.0))
            .flex_shrink_0()
            .border_t_1()
            .border_color(p.border)
            .p(px(14.0))
            .bg(ink("terminal.background"))
            .text_color(ink("terminal.foreground"))
            .font_family(Platform::CURRENT.mono_font())
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(div().child("$ cargo test"))
            .child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_color(ink("terminal.ansi2"))
                            .child("12 tests passed"),
                    )
                    .child(div().w(px(7.0)).h(px(14.0)).bg(ink("terminal.cursor"))),
            )
            .children([0, 8].into_iter().map(|start| {
                div()
                    .flex()
                    .gap(px(5.0))
                    .children((start..start + 8).map(|index| {
                        div()
                            .flex_1()
                            .h(px(8.0))
                            .rounded(px(1.0))
                            .bg(ink(&format!("terminal.ansi{index}")))
                    }))
            }));
        let code = div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .p(px(14.0))
            .font_family(Platform::CURRENT.mono_font())
            .child(code_line(vec![("// Color preview", syntax("comment"))]))
            .child(code_line(vec![
                ("fn ", syntax("keyword")),
                ("main", syntax("function")),
                ("() {", p.text),
            ]))
            .child(code_line(vec![
                ("  let ", syntax("keyword")),
                ("name = ", p.text),
                ("\"Tervia\"", syntax("string")),
                (";", p.text),
            ]))
            .child(code_line(vec![
                ("  let ", syntax("keyword")),
                ("count: ", p.text),
                ("usize", syntax("type")),
                (" = ", p.text),
                ("42", syntax("number")),
                (";", p.text),
            ]))
            .child(code_line(vec![("}", p.text)]));
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(self.chrome.secondary)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.t("theme_preview"))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .child(self.t("theme_preview_hint")),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "theme-editor-preview".into())
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .rounded(px(6.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(p.border)
                    .bg(super::super::super::theme::workspace_background(&theme))
                    .flex()
                    .flex_col()
                    .text_color(p.text)
                    .text_size(px(12.0))
                    .child(
                        div()
                            .h(px(34.0))
                            .flex_shrink_0()
                            .px(px(12.0))
                            .border_b_1()
                            .border_color(p.border)
                            .flex()
                            .items_center()
                            .text_color(p.secondary)
                            .child("demo-project"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .w(px(100.0))
                                    .flex_shrink_0()
                                    .border_r_1()
                                    .border_color(p.border)
                                    .py(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(div().px(px(10.0)).text_color(p.secondary).child("src"))
                                    .child(
                                        div()
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .bg(p.selected)
                                            .child("main.rs"),
                                    )
                                    .child(
                                        div()
                                            .px(px(10.0))
                                            .text_color(p.secondary)
                                            .child("Cargo.toml"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .h(px(34.0))
                                            .flex_shrink_0()
                                            .px(px(12.0))
                                            .flex()
                                            .items_center()
                                            .border_b_1()
                                            .border_color(p.border)
                                            .child("main.rs"),
                                    )
                                    .child(code)
                                    .child(terminal),
                            ),
                    ),
            )
    }
}
