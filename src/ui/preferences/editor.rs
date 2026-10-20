use super::*;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::{
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState},
};
use std::{collections::BTreeMap, sync::Arc};
use tervia::themes::{Definition, Document, Draft, Store};

mod fields;
mod preview;

#[cfg(all(test, feature = "ui-tests"))]
mod tests;

gpui_kit::actions!(tervia, [SaveTheme]);

struct Control {
    input: Entity<InputState>,
    picker: Entity<ColorPickerState>,
}

// Quit may arrive while GPUI has leased the active editor's window, so keep weak
// editor handles rather than reading window roots during that event.
#[derive(Default)]
struct OpenEditors(Vec<(AnyWindowHandle, WeakEntity<Editor>)>);
impl Global for OpenEditors {}

pub(super) fn has_pending_edits(cx: &App) -> bool {
    let windows = cx.windows();
    cx.try_global::<OpenEditors>().is_some_and(|editors| {
        editors.0.iter().any(|(handle, weak)| {
            windows.contains(handle)
                && weak.upgrade().is_some_and(|editor| {
                    let editor = editor.read(cx);
                    editor.saving || editor.closing || editor.draft.is_dirty()
                })
        })
    })
}

pub(super) fn guard_quit(cx: &mut App) -> bool {
    let windows = cx.windows();
    let editors = cx
        .try_global::<OpenEditors>()
        .map(|editors| editors.0.clone())
        .unwrap_or_default();
    for (handle, weak) in editors {
        if !windows.contains(&handle) {
            continue;
        }
        let Some(editor) = weak.upgrade() else {
            continue;
        };
        let editor = editor.read(cx);
        if !editor.saving && !editor.closing && !editor.draft.is_dirty() {
            continue;
        }
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, cx| {
                window.activate_window();
                let _ = weak.update(cx, |editor, cx| editor.confirm_close(true, window, cx));
            });
        });
        return false;
    }
    true
}

fn copy_name(name: &str, language: Language) -> String {
    let suffix = language.format("theme_copy_name", &[("name", "")]);
    let name: String = name
        .chars()
        .take(80_usize.saturating_sub(suffix.chars().count()))
        .collect();
    language.format("theme_copy_name", &[("name", &name)])
}

pub(super) struct Editor {
    draft: Draft,
    original: Option<Document>,
    store: Store,
    preferences: Entity<Preferences>,
    owner: Option<WeakEntity<PreferencesWindow>>,
    language: Language,
    chrome: Palette,
    focus: FocusHandle,
    name: Entity<InputState>,
    controls: BTreeMap<String, Control>,
    tab: usize,
    advanced: bool,
    scroll: ScrollHandle,
    saving: bool,
    closing: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl Editor {
    fn t(&self, key: &'static str) -> &'static str {
        self.language.text(key)
    }

    pub(super) fn open(
        theme: ColorTheme,
        duplicate: bool,
        store: Store,
        preferences: Entity<Preferences>,
        owner: WeakEntity<PreferencesWindow>,
        cx: &mut App,
    ) -> Result<(), String> {
        let language = preferences.read(cx).language;
        let original = if !duplicate && theme.custom().is_some() {
            Some(store.load(theme.id())?)
        } else {
            None
        };
        let mut data = original
            .as_ref()
            .map(|document| document.data.as_ref().clone())
            .unwrap_or_else(|| super::super::theme::snapshot(&theme));
        if duplicate {
            data.set_name(copy_name(theme.label(language), language))?;
        }
        cx.bind_keys([
            KeyBinding::new("escape", Cancel, Some("ThemeEditor")),
            KeyBinding::new("secondary-s", SaveTheme, Some("ThemeEditor")),
        ]);
        let bounds = Bounds::centered(None, size(px(960.0), px(680.0)), cx);
        let (handle, editor) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(880.0), px(600.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(super::super::window_title(language.text("theme_editor"))),
                    appears_transparent: true,
                    traffic_light_position: (Platform::CURRENT == Platform::Mac)
                        .then(|| point(px(16.0), px(18.0))),
                }),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| Self::new(data, original, store, preferences, Some(owner), window, cx))
            },
        )
        .map_err(|error| error.to_string())?;
        let weak = editor.downgrade();
        handle
            .update(cx, |_, window, cx| {
                window.on_window_should_close(cx, move |window, cx| {
                    weak.update(cx, |editor, cx| {
                        if editor.saving || editor.closing {
                            return false;
                        }
                        if !editor.draft.is_dirty() {
                            return true;
                        }
                        editor.request_close(window, cx);
                        false
                    })
                    .unwrap_or(true)
                });
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn new(
        data: Definition,
        original: Option<Document>,
        store: Store,
        preferences: Entity<Preferences>,
        owner: Option<WeakEntity<PreferencesWindow>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let current = preferences.read(cx);
        let language = current.language;
        let chrome = Palette::new(&current.theme);
        let name = cx.new(|cx| InputState::new(window, cx).default_value(data.name().to_owned()));
        let name_subscription =
            cx.subscribe_in(&name, window, |this: &mut Self, name, event, _, cx| {
                if matches!(event, InputEvent::Change) && !this.saving {
                    this.draft.set_name(name.read(cx).value().to_string());
                    cx.notify();
                }
            });
        let mut controls = BTreeMap::new();
        let mut subscriptions = vec![name_subscription];
        for key in tervia::themes::COLOR_KEYS
            .iter()
            .copied()
            .chain(fields::SYNTAX_KEYS.iter().copied())
        {
            let value = data
                .color(key)
                .expect("Editor fields use validated colors")
                .hex();
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            let picker = cx.new(|cx| {
                ColorPickerState::new(window, cx)
                    .default_value(super::super::theme::color(&data, key))
            });
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, input, event, window, cx| {
                    if !matches!(event, InputEvent::Change) || this.saving {
                        return;
                    }
                    let value = input.read(cx).value();
                    if this.draft.set_color(key, value.as_str()).is_ok() {
                        let color = super::super::theme::color(this.draft.data(), key);
                        this.controls[key]
                            .picker
                            .update(cx, |picker, cx| picker.set_value(color, window, cx));
                    }
                    cx.notify();
                },
            ));
            subscriptions.push(cx.subscribe_in(
                &picker,
                window,
                move |this: &mut Self, _, event, window, cx| {
                    if this.saving {
                        return;
                    }
                    if let ColorPickerEvent::Change(Some(color)) = event {
                        let value = super::super::theme::hex(*color);
                        let _ = this.draft.set_color(key, &value);
                        this.sync_input(key, value, window, cx);
                        cx.notify();
                    }
                },
            ));
            controls.insert(key.into(), Control { input, picker });
        }
        let focus = cx.focus_handle();
        name.focus_handle(cx).focus(window, cx);
        if !cx.has_global::<OpenEditors>() {
            cx.set_global(OpenEditors::default());
        }
        let windows = cx.windows();
        let handle = window.window_handle();
        let weak = cx.entity().downgrade();
        cx.update_global::<OpenEditors, _>(|editors, _| {
            editors
                .0
                .retain(|(handle, editor)| windows.contains(handle) && editor.upgrade().is_some());
            editors.0.push((handle, weak));
        });
        Self {
            draft: Draft::new(data),
            original,
            store,
            preferences,
            owner,
            language,
            chrome,
            focus,
            name,
            controls,
            tab: 0,
            advanced: false,
            scroll: ScrollHandle::new(),
            saving: false,
            closing: false,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    fn sync_input(
        &mut self,
        key: &str,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.controls[key].input.clone();
        if input.read(cx).value().as_str() != value {
            // set_value does not emit Change, so picker/reset synchronization cannot loop.
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    fn reset_color(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if self.draft.reset_color(key).is_ok() {
            let value = self.draft.data().color(key).expect("Editor color").hex();
            let color = super::super::theme::color(self.draft.data(), key);
            self.sync_input(key, value, window, cx);
            self.controls[key]
                .picker
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
            cx.notify();
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || !self.draft.can_save() {
            return;
        }
        self.saving = true;
        for control in self.controls.values() {
            control
                .picker
                .update(cx, |picker, cx| picker.set_open(false, cx));
        }
        self.error = None;
        let data = self.draft.data().clone();
        let original = self.original.clone();
        let store = self.store.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match original {
                        Some(document) => store.save(&document, &data),
                        None => store.create(&data),
                    }
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(document) => {
                        // Selecting the edited snapshot preserves System mode and the opposite choice.
                        this.preferences.update(cx, |preferences, cx| {
                            preferences.theme.select(ColorTheme::Custom(document.data));
                            cx.notify();
                        });
                        if let Some(owner) = &this.owner {
                            let _ = owner.update(cx, |owner, cx| owner.reload_themes(cx));
                        }
                        window.remove_window();
                    }
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_close(false, window, cx);
    }

    fn confirm_close(&mut self, quitting: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.closing {
            return;
        }
        if !self.draft.is_dirty() {
            window.remove_window();
            return;
        }
        self.closing = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            self.t("theme_discard_title"),
            Some(self.t("theme_discard_hint")),
            &[self.t("git_cancel"), self.t("theme_discard")],
            cx,
        );
        cx.spawn_in(window, async move |weak, cx| {
            let discard = answer.await == Ok(1);
            let _ = weak.update_in(cx, |this, window, cx| {
                this.closing = false;
                if discard {
                    window.remove_window();
                    if quitting {
                        cx.defer(|cx| cx.dispatch_action(&super::super::Quit));
                    }
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn contrast_warning(&self) -> bool {
        let data = self.draft.data();
        let behind =
            tervia::themes::Color::parse(&super::super::theme::hex(self.chrome.background))
                .expect("Rendered editor background");
        for key in ["ui.background", "ui.workspace_start", "ui.workspace_end"] {
            let background = data.color(key).expect("Validated background").over(behind);
            if ["ui.text", "ui.secondary"]
                .into_iter()
                .chain(fields::SYNTAX_KEYS.iter().copied())
                .any(|key| {
                    data.color(key)
                        .expect("Validated text")
                        .contrast(background)
                        < 4.5
                })
            {
                return true;
            }
            let terminal = data
                .color("terminal.background")
                .expect("Validated terminal background")
                .over(background);
            if data
                .color("terminal.foreground")
                .expect("Validated terminal text")
                .contrast(terminal)
                < 4.5
            {
                return true;
            }
        }
        false
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.chrome;
        let close = cx.entity().downgrade();
        let status = if let Some(error) = &self.error {
            error.clone()
        } else if !self.draft.can_save() {
            self.t("theme_invalid_hint").into()
        } else if self.contrast_warning() {
            self.t("theme_contrast_hint").into()
        } else {
            self.language.format(
                "theme_assignment_hint",
                &[(
                    "appearance",
                    self.t(if self.draft.data().is_light() {
                        "light_theme"
                    } else {
                        "dark_theme"
                    }),
                )],
            )
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.background)
            .text_color(p.text)
            .text_size(px(13.0))
            .key_context("ThemeEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.request_close(window, cx)))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.request_close(window, cx)))
            .on_action(cx.listener(|this, _: &SaveTheme, window, cx| this.save(window, cx)))
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .pl(px(80.0))
                    .when(!has_window_controls(window), titlebar_drag_region)
                    .child(div().flex_1().child(self.t("theme_editor")))
                    .child(window_controls(
                        window,
                        p,
                        self.language,
                        move |_, window, cx| {
                            let _ = close.update(cx, |this, cx| this.request_close(window, cx));
                        },
                    )),
            )
            .child(
                div()
                    .h(px(58.0))
                    .flex_shrink_0()
                    .px(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(64.0))
                            .text_color(p.secondary)
                            .child(self.t("theme_name")),
                    )
                    .child(
                        Input::new(&self.name)
                            .aria_label(self.t("theme_name"))
                            .disabled(self.saving)
                            .w(px(300.0)),
                    )
                    .child(div().text_color(p.secondary).child(self.t(
                        if self.draft.data().is_light() {
                            "light_theme"
                        } else {
                            "dark_theme"
                        },
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.fields(cx))
                    .child(self.preview()),
            )
            .child(
                div()
                    .min_h(px(62.0))
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(p.border)
                    .px(px(24.0))
                    .py(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .text_color(if self.error.is_some() || !self.draft.can_save() {
                                p.red
                            } else {
                                p.secondary
                            })
                            .child(status),
                    )
                    .child(
                        label_button("theme-editor-cancel", self.t("git_cancel"), p)
                            .disabled(self.saving)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.request_close(window, cx)),
                            ),
                    )
                    .child(
                        label_button(
                            "theme-editor-save",
                            self.t(if self.saving {
                                "theme_saving"
                            } else {
                                "theme_save_use"
                            }),
                            p,
                        )
                        .debug_selector(|| "theme-editor-save".into())
                        .disabled(self.saving || !self.draft.can_save())
                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
