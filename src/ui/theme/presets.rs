use std::sync::LazyLock;

use gpui_kit::component::highlighter::{SyntaxColors, ThemeStyle};

use super::*;

pub(super) struct Preset {
    pub name: &'static str,
    pub palette: Palette,
    pub workspace: [u32; 2],
    pub syntax: SyntaxColors,
    pub ansi: [u32; 16],
}

pub(super) fn get(theme: ColorTheme) -> Option<&'static Preset> {
    let index = match theme {
        ColorTheme::Darcula => 0,
        ColorTheme::Monokai => 1,
        ColorTheme::SolarizedDark => 2,
        ColorTheme::SolarizedLight => 3,
        ColorTheme::QuietLight => 4,
        _ => return None,
    };
    Some(&PRESETS[index])
}

struct Syntax {
    foreground: u32,
    keyword: u32,
    string: u32,
    comment: u32,
    number: u32,
    function: u32,
    type_: u32,
}

impl Syntax {
    fn colors(self, light: bool) -> SyntaxColors {
        let base = if light {
            gpui_kit::component::highlighter::HighlightTheme::default_light()
        } else {
            gpui_kit::component::highlighter::HighlightTheme::default_dark()
        };
        let style = |color: u32| -> Option<ThemeStyle> {
            Some(
                serde_json::from_value(serde_json::json!({ "color": format!("#{color:06x}") }))
                    .expect("Bundled syntax colors are valid"),
            )
        };
        SyntaxColors {
            keyword: style(self.keyword),
            operator: style(self.keyword),
            preproc: style(self.keyword),
            string: style(self.string),
            string_escape: style(self.number),
            string_regex: style(self.string),
            comment: style(self.comment),
            comment_doc: style(self.comment),
            number: style(self.number),
            boolean: style(self.number),
            constant: style(self.number),
            function: style(self.function),
            constructor: style(self.type_),
            type_: style(self.type_),
            enum_: style(self.type_),
            attribute: style(self.type_),
            property: style(self.foreground),
            variable: style(self.foreground),
            variable_special: style(self.number),
            primary: style(self.foreground),
            punctuation: style(self.foreground),
            tag: style(self.keyword),
            title: style(self.function),
            link_text: style(self.function),
            link_uri: style(self.string),
            ..base.style.syntax.clone()
        }
    }
}

// Adapted from JetBrains Darcula and VS Code's built-in palettes. Text inks are
// adjusted for contrast on Tervia's transparent panes; terminal ANSI colors retain their roles.
static PRESETS: LazyLock<[Preset; 5]> = LazyLock::new(|| {
    [
        Preset {
            name: "Darcula",
            palette: Palette {
                background: rgb(0x2b2b2b).into(),
                on_accent: rgb(0x25282b).into(),
                sidebar: rgb(0x3c3f41).into(),
                toolbar: rgb(0x333638).into(),
                raised: rgb(0x3c3f41).into(),
                selected: rgb(0x8cb6e5).opacity(0.13).into(),
                border: rgb(0x515658).into(),
                text: rgb(0xc8cbd0).into(),
                secondary: rgb(0xb8bec5).into(),
                muted: rgb(0xb1b4b7).into(),
                ignored: rgb(0x888b8d).into(),
                accent: rgb(0x8cb6e5).into(),
                green: rgb(0xa1c28a).into(),
                red: rgb(0xe89b9b).into(),
            },
            workspace: [0x333638, 0x262829],
            syntax: Syntax {
                foreground: 0xa9b7c6,
                keyword: 0xe09961,
                string: 0xa1b68a,
                comment: 0xb1b4b7,
                number: 0x88b4d8,
                function: 0xffc66d,
                type_: 0xa9b7c6,
            }
            .colors(false),
            ansi: [
                0x000000, 0xf0524f, 0x5c962c, 0xa68a0d, 0x3993d4, 0xa771bf, 0x00a3a3, 0xffffff,
                0x808080, 0xff4050, 0x4fc414, 0xe5bf00, 0x1fb0ff, 0xed7eed, 0x00e5e5, 0xffffff,
            ],
        },
        Preset {
            name: "Monokai",
            palette: Palette {
                background: rgb(0x272822).into(),
                on_accent: rgb(0x272822).into(),
                sidebar: rgb(0x1e1f1c).into(),
                toolbar: rgb(0x2b2c25).into(),
                raised: rgb(0x34352f).into(),
                selected: rgb(0xa6e22e).opacity(0.13).into(),
                border: rgb(0x494b40).into(),
                text: rgb(0xf8f8f2).into(),
                secondary: rgb(0xc5c5b8).into(),
                muted: rgb(0xaaa58d).into(),
                ignored: rgb(0x8d8b7c).into(),
                accent: rgb(0xa6e22e).into(),
                green: rgb(0xa6e22e).into(),
                red: rgb(0xff82ae).into(),
            },
            workspace: [0x2b2c25, 0x20211c],
            syntax: Syntax {
                foreground: 0xf8f8f2,
                keyword: 0xff5c95,
                string: 0xe6db74,
                comment: 0xaaa58d,
                number: 0xae81ff,
                function: 0xa6e22e,
                type_: 0x66d9ef,
            }
            .colors(false),
            ansi: [
                0x333333, 0xc4265e, 0x86b42b, 0xb3b42b, 0x6a7ec8, 0x8c6bc8, 0x56adbc, 0xe3e3dd,
                0x666666, 0xf92672, 0xa6e22e, 0xe2e22e, 0x819aff, 0xae81ff, 0x66d9ef, 0xf8f8f2,
            ],
        },
        Preset {
            name: "Solarized Dark",
            palette: Palette {
                background: rgb(0x002b36).into(),
                on_accent: rgb(0x002b36).into(),
                sidebar: rgb(0x00212b).into(),
                toolbar: rgb(0x073642).into(),
                raised: rgb(0x123f4a).into(),
                selected: rgb(0x63c4bb).opacity(0.13).into(),
                border: rgb(0x28515a).into(),
                text: rgb(0xc4d0cf).into(),
                secondary: rgb(0xabbfbe).into(),
                muted: rgb(0xa0b4b3).into(),
                ignored: rgb(0x718c90).into(),
                accent: rgb(0x63c4bb).into(),
                green: rgb(0xa3b852).into(),
                red: rgb(0xf0867b).into(),
            },
            workspace: [0x073642, 0x00232c],
            syntax: Syntax {
                foreground: 0xc4d0cf,
                keyword: 0xa3b852,
                string: 0x63c4bb,
                comment: 0xa0b4b3,
                number: 0xe18ab4,
                function: 0x69b5e5,
                type_: 0xe1a06a,
            }
            .colors(false),
            ansi: solarized_ansi(),
        },
        Preset {
            name: "Solarized Light",
            palette: Palette {
                background: rgb(0xfdf6e3).into(),
                on_accent: rgb(0xfdf6e3).into(),
                sidebar: rgb(0xeee8d5).into(),
                toolbar: rgb(0xf7f0df).into(),
                raised: rgb(0xeee8d5).into(),
                selected: rgb(0x176891).opacity(0.10).into(),
                border: rgb(0xd8d0b8).into(),
                text: rgb(0x455b63).into(),
                secondary: rgb(0x52676f).into(),
                muted: rgb(0x556a72).into(),
                ignored: rgb(0x87928b).into(),
                accent: rgb(0x176891).into(),
                green: rgb(0x526d00).into(),
                red: rgb(0xae302b).into(),
            },
            workspace: [0xfdf6e3, 0xeee8d5],
            syntax: Syntax {
                foreground: 0x455b63,
                keyword: 0x586e00,
                string: 0x136f67,
                comment: 0x556a72,
                number: 0xac2766,
                function: 0x1b6392,
                type_: 0xa44712,
            }
            .colors(true),
            ansi: solarized_ansi(),
        },
        Preset {
            name: "Quiet Light",
            palette: Palette {
                background: rgb(0xf5f5f5).into(),
                on_accent: rgb(0xfaf8fc).into(),
                sidebar: rgb(0xf2f2f2).into(),
                toolbar: rgb(0xececec).into(),
                raised: rgb(0xe4e4e4).into(),
                selected: rgb(0x7047a3).opacity(0.10).into(),
                border: rgb(0xd3d6ce).into(),
                text: rgb(0x333333).into(),
                secondary: rgb(0x5e6159).into(),
                muted: rgb(0x61645b).into(),
                ignored: rgb(0x8c9185).into(),
                accent: rgb(0x7047a3).into(),
                green: rgb(0x366f20).into(),
                red: rgb(0xa13c4b).into(),
            },
            workspace: [0xf5f5f5, 0xe9ece5],
            syntax: Syntax {
                foreground: 0x333333,
                keyword: 0x405bb0,
                string: 0x366f20,
                comment: 0x62665b,
                number: 0x8b4b1b,
                function: 0x7a3e9d,
                type_: 0x7a3e9d,
            }
            .colors(true),
            ansi: [
                0x333333, 0xa13c4b, 0x366f20, 0x806000, 0x405bb0, 0x7047a3, 0x176e75, 0x6d705b,
                0x61645b, 0xa13c4b, 0x366f20, 0x806000, 0x405bb0, 0x7047a3, 0x176e75, 0x333333,
            ],
        },
    ]
});

const fn solarized_ansi() -> [u32; 16] {
    [
        0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x002b36,
        0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
    ]
}
