use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Icon, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use gpui_kit::*;

use super::theme::Palette;

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
