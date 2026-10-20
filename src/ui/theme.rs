use gpui_kit::component::Theme;
use gpui_kit::*;
use tervia::preferences::Theme as ColorTheme;

mod presets;

fn workspace_colors(theme: ColorTheme) -> [u32; 2] {
    if let Some(preset) = presets::get(theme) {
        return preset.workspace;
    }
    match theme {
        ColorTheme::Dark => [0x515764, 0x20212c],
        ColorTheme::Light => [0xf3f5f8, 0xced8e7],
        ColorTheme::Midnight => [0x303e58, 0x171e30],
        ColorTheme::Forest => [0x354a41, 0x182a24],
        ColorTheme::Paper => [0xf6f0e4, 0xe4d7bd],
        ColorTheme::Rose => [0xf8edf0, 0xe8d4dc],
        _ => unreachable!("Classic presets supply their own workspace colors"),
    }
}

pub fn workspace_background(theme: impl Into<ColorTheme>) -> Background {
    let [start, end] = workspace_colors(theme.into());
    linear_gradient(
        135.0,
        linear_color_stop(rgb(start), 0.0),
        linear_color_stop(rgb(end), 1.0),
    )
}

pub fn apply_palette(theme: impl Into<ColorTheme>, cx: &mut App) {
    let theme = theme.into();
    let light = theme.is_light();
    let preset = presets::get(theme);
    let p = Palette::new(theme);
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
        // Always start from the base: same-mode switches must not retain a previous preset's syntax.
        theme.highlight_theme = if light {
            gpui_kit::component::highlighter::HighlightTheme::default_light()
        } else {
            gpui_kit::component::highlighter::HighlightTheme::default_dark()
        };
        let highlight = std::sync::Arc::make_mut(&mut theme.highlight_theme);
        if let Some(preset) = preset {
            highlight.name = preset.name.into();
            highlight.style.syntax = preset.syntax.clone();
        }
        highlight.style.editor_background = Some(p.background.opacity(0.0));
        highlight.style.editor_gutter_background = Some(p.sidebar.opacity(0.12));
        highlight.style.editor_active_line = Some(p.raised.opacity(0.25));
        highlight.style.editor_foreground = Some(p.text);
        highlight.style.editor_line_number = Some(p.secondary);
        highlight.style.editor_active_line_number = Some(p.text);
    });
}

pub fn terminal_ansi(theme: impl Into<ColorTheme>) -> &'static [u32; 16] {
    let theme = theme.into();
    if let Some(preset) = presets::get(theme) {
        return &preset.ansi;
    }
    if theme.is_light() {
        &LIGHT_ANSI
    } else {
        &DARK_ANSI
    }
}

pub fn syntax_color(theme: ColorTheme, capture: &str) -> Hsla {
    let syntax = if let Some(preset) = presets::get(theme) {
        preset.syntax.style(capture)
    } else {
        let base = if theme.is_light() {
            gpui_kit::component::highlighter::HighlightTheme::default_light()
        } else {
            gpui_kit::component::highlighter::HighlightTheme::default_dark()
        };
        base.style.syntax.style(capture)
    };
    syntax
        .and_then(|style| style.color)
        .unwrap_or_else(|| Palette::new(theme).text)
}

const LIGHT_ANSI: [u32; 16] = [
    0x262a28, 0x92343e, 0x205f37, 0x805600, 0x31578d, 0x783d87, 0x1f626d, 0x485662, 0x495763,
    0x9a3747, 0x235e48, 0x875b00, 0x365b9d, 0x7a368c, 0x1a6977, 0x262a28,
];
const DARK_ANSI: [u32; 16] = [
    0x303341, 0xe69b9f, 0x99cfa6, 0xe9cd97, 0x9fbfe5, 0xcbb0e8, 0x96ccd1, 0xdde1e8, 0x929cad,
    0xf1a9ad, 0xa8d9b5, 0xf1d7a9, 0xb4d0f1, 0xd7bff0, 0xabe0e2, 0xf4f5f7,
];

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
    // Resting Git-ignore decorations recede; selected filenames use primary text instead.
    pub ignored: Hsla,
    pub accent: Hsla,
    pub green: Hsla,
    pub red: Hsla,
}

impl Palette {
    pub fn workbench(theme: impl Into<ColorTheme>) -> Self {
        let theme = theme.into();
        let mut p = Self::new(theme);
        // The workspace owns one window-sized gradient. Panes must reveal it;
        // button ink remains opaque even though their surrounding surfaces do not.
        p.background = p.background.opacity(0.0);
        p.sidebar = p.sidebar.opacity(0.0);
        p.toolbar = p.toolbar.opacity(0.0);
        p.raised = p.raised.opacity(0.35);
        p.border = p.chrome_border();
        if !theme.is_light() {
            p.muted = p.secondary;
        }
        p.accent = match theme {
            ColorTheme::Light => rgb(0x2e67a3).into(),
            ColorTheme::Dark => rgb(0x98bee8).into(),
            _ => p.accent,
        };
        p.selected = p.accent.opacity(if theme.is_light() { 0.10 } else { 0.13 });
        p
    }

    pub fn chrome_border(self) -> Hsla {
        self.text.opacity(0.08)
    }

    pub fn new(theme: impl Into<ColorTheme>) -> Self {
        let theme = theme.into();
        if let Some(preset) = presets::get(theme) {
            return preset.palette;
        }
        match theme {
            ColorTheme::Midnight => Self {
                background: rgb(0x1d273b).into(),
                on_accent: rgb(0x1d273b).into(),
                sidebar: rgb(0x222e44).into(),
                toolbar: rgb(0x202b40).into(),
                raised: rgb(0x33435e).into(),
                selected: rgb(0xa6c9ff).opacity(0.12).into(),
                border: rgb(0x40516b).into(),
                text: rgb(0xe5edf8).into(),
                secondary: rgb(0xb6c8e2).into(),
                muted: rgb(0xa5b8d2).into(),
                ignored: rgb(0x8797ae).into(),
                accent: rgb(0xa6c9ff).into(),
                green: rgb(0xa4d5b1).into(),
                red: rgb(0xf0acb4).into(),
            },
            ColorTheme::Forest => Self {
                background: rgb(0x1d3028).into(),
                on_accent: rgb(0x1d3028).into(),
                sidebar: rgb(0x24392f).into(),
                toolbar: rgb(0x21352c).into(),
                raised: rgb(0x354d40).into(),
                selected: rgb(0xb2dbad).opacity(0.12).into(),
                border: rgb(0x466050).into(),
                text: rgb(0xe8f1e9).into(),
                secondary: rgb(0xc0d3c4).into(),
                muted: rgb(0xa7beae).into(),
                ignored: rgb(0x8caa96).into(),
                accent: rgb(0xb2dbad).into(),
                green: rgb(0xa9dcab).into(),
                red: rgb(0xf0b2b0).into(),
            },
            ColorTheme::Paper => Self {
                background: rgb(0xf8f3e8).into(),
                on_accent: rgb(0xfffbf2).into(),
                sidebar: rgb(0xefe6d5).into(),
                toolbar: rgb(0xf3ecdf).into(),
                raised: rgb(0xe6dac5).into(),
                selected: rgb(0x76552c).opacity(0.10).into(),
                border: rgb(0xd8c9ae).into(),
                text: rgb(0x352e25).into(),
                secondary: rgb(0x5d4d38).into(),
                muted: rgb(0x65533e).into(),
                ignored: rgb(0x938774).into(),
                accent: rgb(0x76552c).into(),
                green: rgb(0x365c37).into(),
                red: rgb(0x943a3b).into(),
            },
            ColorTheme::Rose => Self {
                background: rgb(0xfcf4f6).into(),
                on_accent: rgb(0xfff8fa).into(),
                sidebar: rgb(0xf2e4ea).into(),
                toolbar: rgb(0xf7ebef).into(),
                raised: rgb(0xead8e0).into(),
                selected: rgb(0x943e64).opacity(0.10).into(),
                border: rgb(0xdbc1cd).into(),
                text: rgb(0x382b33).into(),
                secondary: rgb(0x654452).into(),
                muted: rgb(0x704b5b).into(),
                ignored: rgb(0x9b8390).into(),
                accent: rgb(0x943e64).into(),
                green: rgb(0x2d5e47).into(),
                red: rgb(0x9e3347).into(),
            },
            ColorTheme::Light => Self {
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
                ignored: rgb(0x8e8e90).into(),
                accent: rgb(0x235e48).into(),
                green: rgb(0x205f37).into(),
                red: rgb(0x92343e).into(),
            },
            ColorTheme::Dark => Self {
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
                ignored: rgb(0x8c8c8c).into(),
                accent: rgb(0xa8d9c5).into(),
                green: rgb(0x99cfa6).into(),
                red: rgb(0xe6a0a5).into(),
            },
            _ => unreachable!("Classic presets supply their own palette"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[cfg(feature = "ui-tests")]
    #[gpui_test::test]
    fn classic_syntax_colors_apply_to_source_and_reset_when_leaving_a_preset(
        cx: &mut gpui_test::TestAppContext,
    ) {
        use gpui_kit::component::{
            ActiveTheme,
            highlighter::{HighlightTheme, SyntaxHighlighter},
        };
        use tervia::preferences::Preferences;
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            for (id, keyword, string) in [
                ("darcula", 0xe09961, 0xa1b68a),
                ("monokai", 0xff5c95, 0xe6db74),
                ("solarized-dark", 0xa3b852, 0x63c4bb),
                ("solarized-light", 0x586e00, 0x136f67),
                ("quiet-light", 0x405bb0, 0x366f20),
            ] {
                let preset = ColorTheme::from_id(id).expect("Classic preset is available");
                super::super::preferences::apply(
                    &Preferences {
                        theme: preset.into(),
                        ..Preferences::default()
                    },
                    cx,
                );
                let highlight = &cx.theme().highlight_theme;
                assert_eq!(
                    highlight.style.syntax.style("keyword").unwrap().color,
                    Some(rgb(keyword).into()),
                    "{id}"
                );
                assert_eq!(
                    highlight.style.syntax.style("string").unwrap().color,
                    Some(rgb(string).into()),
                    "{id}"
                );
                let source = "fn mock() { let value = \"mock\"; }";
                let mut parser = SyntaxHighlighter::new("rust");
                parser.update(None, &source.into(), None);
                let spans = parser.styles(&(0..source.len()), highlight.as_ref());
                assert!(
                    spans
                        .iter()
                        .any(|(_, style)| style.color == Some(rgb(keyword).into())),
                    "{id}: keyword reaches the editor"
                );
                assert!(
                    spans
                        .iter()
                        .any(|(_, style)| style.color == Some(rgb(string).into())),
                    "{id}: string reaches the editor"
                );
            }
            for theme in [ColorTheme::Dark, ColorTheme::Light] {
                super::super::preferences::apply(
                    &Preferences {
                        theme: theme.into(),
                        ..Preferences::default()
                    },
                    cx,
                );
                let base = if theme.is_light() {
                    HighlightTheme::default_light()
                } else {
                    HighlightTheme::default_dark()
                };
                assert_eq!(cx.theme().highlight_theme.style.syntax, base.style.syntax);
            }
        });
    }

    fn contrast(foreground: Hsla, background: Hsla) -> f32 {
        let luminance = |color: Hsla| {
            let color = color.to_rgb();
            let linear = |value: f32| {
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        };
        let foreground = luminance(foreground);
        let background = luminance(background);
        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
    }

    #[test]
    fn new_themes_keep_text_readable_on_solid_surfaces_and_both_gradient_endpoints() {
        for theme in [
            ColorTheme::Midnight,
            ColorTheme::Forest,
            ColorTheme::Paper,
            ColorTheme::Rose,
            ColorTheme::Darcula,
            ColorTheme::Monokai,
            ColorTheme::SolarizedDark,
            ColorTheme::SolarizedLight,
            ColorTheme::QuietLight,
        ] {
            let p = Palette::new(theme);
            let [start, end] = workspace_colors(theme);
            for background in [
                p.background,
                p.sidebar,
                p.toolbar,
                p.raised,
                rgb(start).into(),
                rgb(end).into(),
            ] {
                for foreground in [p.text, p.secondary, p.muted, p.accent, p.green, p.red] {
                    assert!(
                        contrast(foreground, background) >= 4.5,
                        "{theme:?}: {foreground:?} on {background:?}"
                    );
                }
            }
            assert!(contrast(p.on_accent, p.accent) >= 4.5, "{theme:?}");
            let workbench = Palette::workbench(theme);
            assert_eq!(workbench.background.a, 0.0);
            assert_eq!(workbench.sidebar.a, 0.0);
            assert_eq!(workbench.toolbar.a, 0.0);
            assert_eq!(workbench.accent, p.accent);
        }
    }
}
