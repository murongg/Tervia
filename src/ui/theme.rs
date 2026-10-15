use gpui_kit::component::Theme;
use gpui_kit::*;

pub fn workspace_background(light: bool) -> Background {
    if light {
        linear_gradient(
            135.0,
            linear_color_stop(rgb(0xf3f5f8), 0.0),
            linear_color_stop(rgb(0xced8e7), 1.0),
        )
    } else {
        linear_gradient(
            135.0,
            linear_color_stop(rgb(0x515764), 0.0),
            linear_color_stop(rgb(0x20212c), 1.0),
        )
    }
}

pub fn apply_palette(light: bool, cx: &mut App) {
    let p = Palette::new(light);
    Theme::update(cx, |theme| {
        theme.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Scrolling;
        theme.colors.scrollbar = p.background.opacity(0.0);
        theme.colors.scrollbar_thumb = p.text.opacity(0.35);
        theme.colors.scrollbar_thumb_hover = p.text.opacity(0.55);
        theme.colors.background = p.background;
        theme.colors.foreground = p.text;
        theme.colors.border = p.border;
        theme.colors.input = p.border;
        theme.colors.primary = p.accent;
        theme.colors.primary_foreground = p.on_accent;
        theme.colors.accent = p.raised;
        theme.colors.accent_foreground = p.text;
        theme.colors.secondary = p.raised;
        theme.colors.muted = p.raised;
        theme.colors.muted_foreground = p.secondary;
        theme.colors.popover = p.sidebar;
        theme.colors.popover_foreground = p.text;
        theme.colors.ring = p.accent;
        // Editor surfaces must reveal the same window gradient rather than paint a second canvas.
        let highlight = std::sync::Arc::make_mut(&mut theme.highlight_theme);
        highlight.style.editor_background = Some(p.background.opacity(0.0));
        highlight.style.editor_gutter_background = Some(p.sidebar.opacity(0.12));
        highlight.style.editor_active_line = Some(p.raised.opacity(0.25));
        highlight.style.editor_foreground = Some(p.text);
        highlight.style.editor_line_number = Some(p.secondary);
        highlight.style.editor_active_line_number = Some(p.text);
    });
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Hsla,
    pub on_accent: Hsla,
    pub sidebar: Hsla,
    pub toolbar: Hsla,
    pub raised: Hsla,
    pub selected: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub green: Hsla,
    pub red: Hsla,
}

impl Palette {
    pub fn workbench(light: bool) -> Self {
        let mut p = Self::new(light);
        // The workspace owns one window-sized gradient. Panes must reveal it;
        // button ink remains opaque even though their surrounding surfaces do not.
        p.background = p.background.opacity(0.0);
        p.sidebar = p.sidebar.opacity(0.0);
        p.toolbar = p.toolbar.opacity(0.0);
        p.raised = p.raised.opacity(0.35);
        p.border = p.chrome_border();
        if light {
            p.selected = rgb(0x2e67a3).opacity(0.10).into();
            p.accent = rgb(0x2e67a3).into();
        } else {
            p.muted = p.secondary;
            p.selected = rgb(0x98bee8).opacity(0.13).into();
            p.accent = rgb(0x98bee8).into();
        }
        p
    }

    pub fn chrome_border(self) -> Hsla {
        self.text.opacity(0.08)
    }

    pub fn new(light: bool) -> Self {
        if light {
            Self {
                background: rgb(0xfafaf9).into(),
                on_accent: rgb(0xfafaf9).into(),
                sidebar: rgb(0xf0f1ef).into(),
                toolbar: rgb(0xf5f5f3).into(),
                raised: rgb(0xe8ebe7).into(),
                selected: rgb(0x262a28).opacity(0.06).into(),
                border: rgb(0xd9ddd9).into(),
                text: rgb(0x262a28).into(),
                secondary: rgb(0x485662).into(),
                muted: rgb(0x495763).into(),
                accent: rgb(0x235e48).into(),
                green: rgb(0x205f37).into(),
                red: rgb(0x92343e).into(),
            }
        } else {
            Self {
                background: rgb(0x252630).into(),
                on_accent: rgb(0x252630).into(),
                sidebar: rgb(0x2b2d37).into(),
                toolbar: rgb(0x292b35).into(),
                raised: rgb(0x373b47).into(),
                selected: rgb(0xe6e8ec).opacity(0.08).into(),
                border: rgb(0x3b3e49).into(),
                text: rgb(0xe6e8ec).into(),
                secondary: rgb(0xc0c6d0).into(),
                muted: rgb(0xa4acba).into(),
                accent: rgb(0xa8d9c5).into(),
                green: rgb(0x99cfa6).into(),
                red: rgb(0xe6a0a5).into(),
            }
        }
    }
}
