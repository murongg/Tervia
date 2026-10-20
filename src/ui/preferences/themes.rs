use super::*;
use gpui_kit::component::{
    Disableable as _,
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
};

impl PreferencesWindow {
    pub(super) fn reload_themes(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.config_path.clone() else {
            return;
        };
        self.theme_loading = true;
        self.theme_error = None;
        self.theme_task = Some(cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { tervia::themes::Store::new(path).list() })
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.theme_loading = false;
                match result {
                    Ok(catalog) => {
                        this.custom_themes = catalog.themes;
                        this.theme_error =
                            (!catalog.errors.is_empty()).then(|| catalog.errors.join("\n"));
                    }
                    Err(error) => this.theme_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open_theme_editor(&mut self, theme: ColorTheme, duplicate: bool, cx: &mut Context<Self>) {
        let Some(path) = self.config_path.clone() else {
            return;
        };
        if let Err(error) = super::editor::Editor::open(
            theme,
            duplicate,
            tervia::themes::Store::new(path),
            self.preferences.clone(),
            cx.entity().downgrade(),
            cx,
        ) {
            self.theme_error = Some(error);
            cx.notify();
        }
    }

    pub(super) fn delete_theme(
        &mut self,
        theme: ColorTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.theme_deleting {
            return;
        }
        let Some(path) = self.config_path.clone() else {
            return;
        };
        let Some(document) = self
            .custom_themes
            .iter()
            .find(|document| document.data.id() == theme.id())
            .cloned()
        else {
            return;
        };
        self.theme_deleting = true;
        let detail = self
            .current
            .language
            .format("theme_delete_hint", &[("name", document.data.name())]);
        let answer = window.prompt(
            PromptLevel::Warning,
            self.t("theme_delete"),
            Some(&detail),
            &[self.t("git_cancel"), self.t("theme_delete")],
            cx,
        );
        cx.spawn_in(window, async move |weak, cx| {
            let accepted = answer.await == Ok(1);
            let id = document.data.id().to_owned();
            let result = if accepted {
                cx.background_executor()
                    .spawn(async move { tervia::themes::Store::new(path).delete(&document) })
                    .await
            } else {
                Ok(())
            };
            let _ = weak.update_in(cx, |this, _, cx| {
                this.theme_deleting = false;
                match result {
                    Ok(()) if accepted => {
                        this.preferences.update(cx, |preferences, cx| {
                            if preferences.theme.remove_custom(&id) {
                                cx.notify();
                            }
                        });
                        this.reload_themes(cx);
                    }
                    Err(error) => this.theme_error = Some(error),
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn theme_menu(
        &self,
        theme: &ColorTheme,
        menu: PopupMenu,
        owner: WeakEntity<Self>,
    ) -> PopupMenu {
        if self.config_path.is_none() {
            return menu;
        }
        let copying = owner.clone();
        let copy = theme.clone();
        let mut menu = menu.item(PopupMenuItem::new(self.t("theme_duplicate")).on_click(
            move |_, _, cx| {
                let _ = copying.update(cx, |this, cx| {
                    this.open_theme_editor(copy.clone(), true, cx)
                });
            },
        ));
        if theme.custom().is_some() {
            let editing = owner.clone();
            let edit = theme.clone();
            let delete = theme.clone();
            menu = menu
                .item(
                    PopupMenuItem::new(self.t("theme_edit")).on_click(move |_, _, cx| {
                        let _ = editing.update(cx, |this, cx| {
                            this.open_theme_editor(edit.clone(), false, cx)
                        });
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(self.t("theme_delete")).on_click(move |_, window, cx| {
                        let _ = owner
                            .update(cx, |this, cx| this.delete_theme(delete.clone(), window, cx));
                    }),
                );
        }
        menu
    }
}

impl PreferencesWindow {
    pub(super) fn theme_control(&self, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(&self.current.theme);
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
        let p = Palette::new(&self.current.theme);
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .pt(px(6.0))
            .pb(px(6.0))
            .child(
                div().flex().justify_end().child(
                    label_button("theme-create", self.t("theme_create"), p)
                        .debug_selector(|| "theme-create".into())
                        .disabled(self.config_path.is_none() || self.theme_deleting)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_theme_editor(this.current.theme.current(), true, cx)
                        })),
                ),
            )
            .when_some(self.theme_error.clone(), |body, error| {
                body.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.red)
                        .child(error)
                        .child(
                            label_button("theme-reload", self.t("autostart_retry"), p)
                                .on_click(cx.listener(|this, _, _, cx| this.reload_themes(cx))),
                        ),
                )
            })
            .children(
                [false, true]
                    .into_iter()
                    .map(|light| self.theme_group(light, cx)),
            )
    }

    fn theme_group(&self, light: bool, cx: &mut Context<Self>) -> Div {
        let p = Palette::new(&self.current.theme);
        let choices: Vec<_> = ColorTheme::ALL
            .into_iter()
            .chain(
                self.custom_themes
                    .iter()
                    .map(|document| ColorTheme::Custom(document.data.clone())),
            )
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
                    .children(row.iter().map(|theme| self.theme_choice(theme.clone(), cx)))
                    // Keep the last row's previews the same width as every other row.
                    .children((row.len()..3).map(|_| div().flex_1().min_w_0()))
            }))
    }

    fn theme_choice(&self, theme: ColorTheme, cx: &mut Context<Self>) -> AnyElement {
        let p = Palette::new(&self.current.theme);
        let selected = self.current.theme.selected(theme.is_light()) == theme;
        let choosing = theme.clone();
        let owner = cx.entity().downgrade();
        let button = Button::new(format!("theme-choice-{}", theme.id()))
            .w_full()
            .debug_selector({
                let id = theme.id().to_owned();
                move || format!("theme-choice-{id}")
            })
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
            .tooltip(theme.label(self.current.language).to_owned())
            .toggled(selected)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(thumbnail(&theme))
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
                            .child(theme.label(self.current.language).to_owned()),
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
                    prefs.theme.select(choosing.clone());
                    cx.notify();
                });
            }));
        div()
            .id(format!("theme-option-{}", theme.id()))
            .flex_1()
            .min_w_0()
            .child(button)
            .context_menu(move |menu, _, cx| {
                if let Some(owner) = owner.upgrade() {
                    owner.read(cx).theme_menu(&theme, menu, owner.downgrade())
                } else {
                    menu
                }
            })
            .into_any_element()
    }
}

fn stroke(width: f32, color: Hsla) -> Div {
    div().w(px(width)).h(px(2.0)).rounded(px(1.0)).bg(color)
}

pub(super) fn thumbnail(theme: &ColorTheme) -> Div {
    let p = Palette::new(theme);
    // Use the real palette and window gradient so previews follow future theme edits.
    div()
        .debug_selector({
            let id = theme.id().to_owned();
            move || format!("theme-preview-{id}")
        })
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
