use alacritty_terminal::term::cell::{Cell, Flags};
use gpui_kit::*;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Glyph {
    character: char,
    color: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
}

#[derive(Default)]
pub(super) struct Glyphs {
    font: Option<(Font, Pixels)>,
    width: Option<Pixels>,
    lines: HashMap<Glyph, Arc<ShapedLine>>,
}

impl Glyphs {
    pub fn configure(&mut self, font: &Font, size: Pixels) {
        if self
            .font
            .as_ref()
            .is_none_or(|previous| previous != &(font.clone(), size))
        {
            self.font = Some((font.clone(), size));
            self.width = None;
            self.lines.clear();
        }
    }

    pub fn cell_width(&mut self, window: &mut Window) -> Pixels {
        *self.width.get_or_insert_with(|| {
            let (font, size) = self.font.as_ref().unwrap();
            let run = TextRun {
                len: 1,
                font: font.clone(),
                ..Default::default()
            };
            window
                .text_system()
                .shape_line("M".into(), *size, &[run], None)
                .width
                .max(px(1.0))
        })
    }

    pub fn line(&mut self, cell: &Cell, color: Hsla, window: &mut Window) -> Arc<ShapedLine> {
        let key = Glyph {
            character: cell.c,
            color,
            bold: cell.flags.contains(Flags::BOLD),
            italic: cell.flags.contains(Flags::ITALIC),
            underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        };
        // Combining sequences keep the original per-cell shaping. The common
        // single-codepoint path avoids allocating text or copying large ShapedLines.
        let cacheable = cell.zerowidth().is_none();
        if cacheable && let Some(line) = self.lines.get(&key) {
            return line.clone();
        }
        let (font, size) = self.font.as_ref().unwrap();
        let mut text = cell.c.to_string();
        if let Some(combining) = cell.zerowidth() {
            text.extend(combining);
        }
        let run = TextRun {
            len: text.len(),
            font: Font {
                weight: if key.bold {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                },
                style: if key.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                ..font.clone()
            },
            color,
            background_color: None,
            underline: key.underline.then_some(UnderlineStyle {
                color: Some(color),
                thickness: px(1.0),
                wavy: false,
            }),
            strikethrough: None,
        };
        let line = Arc::new(
            window
                .text_system()
                .shape_line(text.into(), *size, &[run], None),
        );
        if cacheable {
            // True-color output and large alphabets must not create an unbounded cache.
            if self.lines.len() >= 4096 {
                self.lines.clear();
            }
            self.lines.insert(key, line.clone());
        }
        line
    }
}
