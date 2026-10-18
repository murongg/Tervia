use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Icon, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    tooltip::Tooltip,
};
use gpui_kit::*;
use tervia::locale::Language;

use super::theme::Palette;

pub(super) fn titlebar_mouse_down(event: &MouseDownEvent, window: &mut Window, _: &mut App) {
    if event.click_count == 2 {
        window.zoom_window();
    } else if event.click_count == 1 {
        // Native dragging can consume subsequent events, so handle the second click first.
        window.start_window_move();
    }
}

pub(super) fn window_controls(window: &Window, p: Palette, language: Language) -> Div {
    if !cfg!(target_os = "windows") {
        return div();
    }
    let maximized = window.is_maximized();
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
        let close = matches!(area, WindowControlArea::Close);
        let label = language.text(key).to_owned();
        controls = controls.child(
            div()
                .id(id)
                .debug_selector(move || id.into())
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
                // Keep mouse events propagating: GPUI handles native caption clicks after dispatch.
                // Native hit testing preserves Snap Layouts and the registered WM_CLOSE guard.
                .window_control_area(area)
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

#[cfg(all(test, feature = "ui-tests", target_os = "windows"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_test::{TestAppContext, VisualTestContext};
    use tervia::{
        preferences::Preferences,
        state::{Screen, Tab},
    };

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
