use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Icon, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use tervia::locale::Language;
use tervia::platform::Platform;

use super::theme::Palette;

pub(super) fn titlebar_mouse_down(event: &MouseDownEvent, window: &mut Window, _: &mut App) {
    if event.click_count == 2 {
        window.zoom_window();
    } else if event.click_count == 1 {
        // Native dragging can consume subsequent events, so handle the second click first.
        window.start_window_move();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Caption {
    System,
    Native,
    Client(gpui_kit::WindowControls),
}

impl Caption {
    fn new(
        platform: Platform,
        decorations: Decorations,
        supported: gpui_kit::WindowControls,
    ) -> Self {
        match (platform, decorations) {
            (Platform::Windows, _) => Self::Native,
            // Use the negotiated mode: Wayland may fall back to client decorations even
            // when server decorations were requested, while X11 may keep its system bar.
            (Platform::Linux, Decorations::Client { .. }) => Self::Client(supported),
            _ => Self::System,
        }
    }

    fn for_window(window: &Window) -> Self {
        Self::new(
            Platform::CURRENT,
            window.window_decorations(),
            window.window_controls(),
        )
    }
}

pub(super) fn has_window_controls(window: &Window) -> bool {
    !matches!(Caption::for_window(window), Caption::System)
}

impl super::Tervia {
    pub(super) fn window_controls(
        &self,
        window: &Window,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        window_controls(
            window,
            p,
            self.appearance.language,
            cx.listener(|this, _, window, cx| {
                if this.try_quit(cx) {
                    window.remove_window();
                }
            }),
        )
    }
}

pub(super) fn window_controls(
    window: &Window,
    p: Palette,
    language: Language,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    render_window_controls(
        Caption::for_window(window),
        window.is_maximized(),
        p,
        language,
        on_close,
    )
}

fn render_window_controls(
    mode: Caption,
    maximized: bool,
    p: Palette,
    language: Language,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let supported = match mode {
        Caption::System => return div(),
        Caption::Native => gpui_kit::WindowControls::default(),
        Caption::Client(supported) => supported,
    };
    let on_close = Rc::new(on_close);
    let mut controls = div().flex().h_full().flex_shrink_0().ml(px(12.0));
    for (id, glyph, area, key) in [
        (
            "window-minimize",
            IconName::WindowMinimize,
            WindowControlArea::Min,
            "window_minimize",
        ),
        (
            "window-maximize",
            if maximized {
                IconName::WindowRestore
            } else {
                IconName::WindowMaximize
            },
            WindowControlArea::Max,
            if maximized {
                "window_restore"
            } else {
                "window_maximize"
            },
        ),
        (
            "window-close",
            IconName::WindowClose,
            WindowControlArea::Close,
            "window_close",
        ),
    ] {
        if (matches!(area, WindowControlArea::Min) && !supported.minimize)
            || (matches!(area, WindowControlArea::Max) && !supported.maximize)
        {
            continue;
        }
        let close = matches!(area, WindowControlArea::Close);
        let label = language.text(key).to_owned();
        let on_close = on_close.clone();
        controls = controls.child(
            div()
                .id(id)
                .debug_selector(move || {
                    if maximized && matches!(area, WindowControlArea::Max) {
                        "window-restore"
                    } else {
                        id
                    }
                    .into()
                })
                .role(Role::Button)
                .aria_label(label.clone())
                .accessibility_id(id)
                .flex()
                .items_center()
                .justify_center()
                .w(px(46.0))
                .h_full()
                .flex_shrink_0()
                .text_color(p.text)
                .hover(move |style| {
                    if close {
                        style.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))
                    } else {
                        style.bg(p.text.opacity(0.12))
                    }
                })
                .active(move |style| {
                    style.bg(if close {
                        rgb(0xa52317).into()
                    } else {
                        p.text.opacity(0.20)
                    })
                })
                .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
                .when(matches!(mode, Caption::Native), |button| {
                    // Keep native caption clicks propagating so GPUI preserves Snap Layouts
                    // and the registered WM_CLOSE guard after dispatch.
                    button.window_control_area(area)
                })
                .when(matches!(mode, Caption::Client(_)), |button| {
                    button
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            match area {
                                WindowControlArea::Min => window.minimize_window(),
                                WindowControlArea::Max => window.zoom_window(),
                                // Removing a GPUI window bypasses the platform close request;
                                // the workspace callback must run its normal exit guard first.
                                WindowControlArea::Close => on_close(event, window, cx),
                                WindowControlArea::Drag => {
                                    unreachable!("Caption button cannot drag")
                                }
                            }
                        })
                })
                .child(Icon::new(glyph).size(px(15.0))),
        );
    }
    controls
}

pub fn icon(name: IconName, color: Hsla) -> Icon {
    Icon::new(name).size(px(15.0)).text_color(color)
}

pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    tooltip: impl Into<SharedString>,
    p: Palette,
    cx: &App,
) -> Button {
    Button::new(id)
        .custom(icon_style(p, cx))
        .small()
        .icon(icon(name, p.secondary))
        .tooltip(tervia::platform::Platform::CURRENT.hint(tooltip.into().as_ref()))
        .size(px(28.0))
        .rounded(px(5.0))
}

pub fn icon_style(p: Palette, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .foreground(p.secondary)
        .hover(p.text.opacity(0.12))
        .active(p.text.opacity(0.20))
}

pub fn label_button(id: impl Into<ElementId>, label: &str, p: Palette) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .label(label.to_owned())
        .text_color(p.secondary)
        .h(px(28.0))
        .rounded(px(5.0))
}

pub fn separator(p: Palette) -> Div {
    div()
        .w(px(1.0))
        .h(px(16.0))
        .bg(p.chrome_border())
        .mx(px(5.0))
}

pub fn badge(label: impl Into<SharedString>, color: Hsla, p: Palette) -> Div {
    div()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(4.0))
        .bg(p.selected)
        .text_color(color)
        .text_size(px(11.0))
        .child(label.into())
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_test::TestAppContext;
    #[cfg(target_os = "windows")]
    use gpui_test::VisualTestContext;
    use std::{cell::Cell, rc::Rc};
    #[cfg(target_os = "windows")]
    use tervia::{
        preferences::Preferences,
        state::{Screen, Tab},
    };

    #[test]
    fn caption_ownership_follows_the_actual_linux_decoration_mode() {
        let supported = gpui_kit::WindowControls::default();
        let client = Decorations::Client {
            tiling: Tiling::default(),
        };
        assert_eq!(
            Caption::new(tervia::platform::Platform::Linux, client, supported),
            Caption::Client(supported)
        );
        assert_eq!(
            Caption::new(
                tervia::platform::Platform::Linux,
                Decorations::Server,
                supported
            ),
            Caption::System
        );
        assert_eq!(
            Caption::new(tervia::platform::Platform::Mac, client, supported),
            Caption::System
        );
        assert_eq!(
            Caption::new(
                tervia::platform::Platform::Windows,
                Decorations::Server,
                supported
            ),
            Caption::Native
        );
    }

    struct ClientCaption {
        mode: Caption,
        maximized: bool,
        allow_close: Rc<Cell<bool>>,
        attempts: Rc<Cell<usize>>,
    }

    impl Render for ClientCaption {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let allow_close = self.allow_close.clone();
            let attempts = self.attempts.clone();
            div().size_full().child(
                div()
                    .h(px(48.0))
                    .flex()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {
                        panic!("Caption buttons must not drag their parent")
                    })
                    .child(render_window_controls(
                        self.mode,
                        self.maximized,
                        Palette::new(false),
                        Language::English,
                        move |_, window, _| {
                            attempts.set(attempts.get() + 1);
                            if allow_close.get() {
                                window.remove_window();
                            }
                        },
                    )),
            )
        }
    }

    #[gpui_test::test]
    fn client_caption_controls_respect_capabilities_and_maximization(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (view, cx) = cx.add_window_view(|_, _| ClientCaption {
            mode: Caption::System,
            maximized: false,
            allow_close: Rc::new(Cell::new(false)),
            attempts: Rc::new(Cell::new(0)),
        });
        for mode in [
            Caption::System,
            Caption::Client(gpui_kit::WindowControls::default()),
            Caption::Client(gpui_kit::WindowControls {
                minimize: false,
                maximize: false,
                ..Default::default()
            }),
            Caption::Client(gpui_kit::WindowControls {
                minimize: false,
                ..Default::default()
            }),
            Caption::Client(gpui_kit::WindowControls {
                maximize: false,
                ..Default::default()
            }),
        ] {
            view.update(cx, |view, cx| {
                view.mode = mode;
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("window-close").is_some(),
                !matches!(mode, Caption::System)
            );
            assert_eq!(
                cx.debug_bounds("window-minimize").is_some(),
                matches!(mode, Caption::Client(supported) if supported.minimize)
            );
            assert_eq!(
                cx.debug_bounds("window-maximize").is_some(),
                matches!(mode, Caption::Client(supported) if supported.maximize)
            );
        }
        view.update(cx, |view, cx| {
            view.mode = Caption::Client(gpui_kit::WindowControls::default());
            view.maximized = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("window-restore").is_some());
    }

    #[gpui_test::test]
    fn client_caption_close_uses_the_guard_and_does_not_drag(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let allow_close = Rc::new(Cell::new(false));
        let attempts = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view(|_, _| ClientCaption {
            mode: Caption::Client(gpui_kit::WindowControls::default()),
            maximized: false,
            allow_close: allow_close.clone(),
            attempts: attempts.clone(),
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let close = cx.debug_bounds("window-close").unwrap();
        cx.simulate_click(close.center(), Modifiers::default());
        assert_eq!(attempts.get(), 1);
        assert_eq!(cx.windows().len(), 1);
        allow_close.set(true);
        cx.simulate_click(close.center(), Modifiers::default());
        assert_eq!(attempts.get(), 2);
        assert!(cx.windows().is_empty());
    }

    #[cfg(target_os = "windows")]
    fn assert_caption_controls(cx: &mut VisualTestContext, width: f32) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let minimize = cx
            .debug_bounds("window-minimize")
            .expect("Minimize is visible");
        let maximize = cx
            .debug_bounds("window-maximize")
            .expect("Maximize is visible");
        let close = cx.debug_bounds("window-close").expect("Close is visible");
        for bounds in [minimize, maximize, close] {
            assert_eq!(bounds.top(), px(0.0));
            assert_eq!(bounds.size.height, px(48.0));
            assert!(bounds.size.width >= px(40.0));
        }
        assert_eq!(minimize.right(), maximize.left());
        assert_eq!(maximize.right(), close.left());
        assert_eq!(close.right(), px(width));
    }

    #[gpui_test::test]
    #[cfg(target_os = "windows")]
    fn windows_caption_controls_cover_welcome_and_workspace(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                let mut view = super::super::Tervia::new(window, cx);
                let project = view.state.open_directory("/synthetic/caption".into());
                view.init_project(project, window, cx);
                view.state.set_tab(Tab::Files);
                view.state.screen = Screen::Welcome;
                view
            });
            view = Some(entity.clone());
            gpui_kit::base::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        for screen in [Screen::Welcome, Screen::Project] {
            for light in [false, true] {
                view.update(cx, |view, cx| {
                    view.state.screen = screen;
                    view.preferences.update(cx, |preferences, cx| {
                        preferences.light_theme = light;
                        cx.notify();
                    });
                    cx.notify();
                });
                for width in [880.0, 1220.0] {
                    cx.simulate_resize(size(px(width), px(560.0)));
                    assert_caption_controls(cx, width);
                }
            }
        }
    }

    #[gpui_test::test]
    #[cfg(target_os = "windows")]
    fn windows_caption_controls_cover_settings(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let preferences = cx.new(|_| Preferences::default());
            let error = cx.new(|_| None);
            let updater = cx.new(|cx| super::super::updates::Updater::new(preferences.clone(), cx));
            let entity = cx.new(|cx| {
                super::super::preferences::PreferencesWindow::new(
                    preferences,
                    None,
                    error,
                    updater,
                    window,
                    cx,
                )
            });
            gpui_kit::base::Root::new(entity, window, cx)
        });
        for width in [700.0, 760.0] {
            cx.simulate_resize(size(px(width), px(640.0)));
            assert_caption_controls(cx, width);
        }
    }
}
