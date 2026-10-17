use std::sync::{Arc, Mutex};
mod links;
pub mod shell;
pub use links::{FileLink, file_link};

use alacritty_terminal::{
    event::{Event, EventListener},
    grid::Dimensions,
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::{Config, Term, TermMode},
    vte::ansi::{CursorShape, CursorStyle, Processor},
};

#[derive(Clone, Default)]
pub struct Events(Arc<Mutex<Vec<Event>>>);

impl EventListener for Events {
    fn send_event(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

struct Size(usize, usize);

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.1
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn columns(&self) -> usize {
        self.0
    }
}

pub struct TerminalBuffer {
    pub term: Term<Events>,
    parser: Processor,
    events: Events,
}

impl TerminalBuffer {
    pub fn new(columns: usize, rows: usize) -> Self {
        Self::with_history(columns, rows, 10000)
    }

    pub(crate) fn with_history(columns: usize, rows: usize, history: usize) -> Self {
        let events = Events::default();
        Self {
            term: Term::new(
                Config {
                    scrolling_history: history,
                    default_cursor_style: CursorStyle {
                        shape: CursorShape::Beam,
                        blinking: true,
                    },
                    ..Config::default()
                },
                &Size(columns.max(2), rows.max(1)),
                events.clone(),
            ),
            parser: Processor::new(),
            events,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        // Keep the parser between chunks: UTF-8 and escape sequences can span PTY reads.
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.term.resize(Size(columns.max(2), rows.max(1)));
    }

    pub fn size(&self) -> (usize, usize) {
        (self.term.columns(), self.term.screen_lines())
    }

    pub fn text(&self) -> String {
        self.term
            .bounds_to_string(
                Point::new(self.term.topmost_line(), Column(0)),
                Point::new(self.term.bottommost_line(), self.term.last_column()),
            )
            .trim_end_matches('\n')
            .to_owned()
    }

    pub fn visible_text(&self) -> String {
        let offset = self.term.grid().display_offset() as i32;
        self.term
            .bounds_to_string(
                Point::new(Line(-offset), Column(0)),
                Point::new(
                    Line(self.term.screen_lines() as i32 - 1 - offset),
                    self.term.last_column(),
                ),
            )
            .trim_end_matches('\n')
            .to_owned()
    }

    pub fn select(&mut self, start: Point, end: Point) {
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        self.term.selection = Some(selection);
    }

    pub fn selected_text(&self) -> String {
        self.term.selection_to_string().unwrap_or_default()
    }

    pub fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut *self.events.0.lock().unwrap())
    }

    pub fn find(&self, query: &str) -> Vec<alacritty_terminal::term::search::Match> {
        use alacritty_terminal::term::search::RegexSearch;
        if query.is_empty() || query.len() > 4096 {
            return Vec::new();
        }
        let Ok(mut regex) = RegexSearch::new(&regex::escape(query)) else {
            return Vec::new();
        };
        let mut origin = Point::new(self.term.topmost_line(), Column(0));
        let end = Point::new(self.term.bottommost_line(), self.term.last_column());
        let mut matches = Vec::new();
        while let Some(found) = self.term.regex_search_right(&mut regex, origin, end) {
            let last = *found.end();
            matches.push(found);
            if last >= end || matches.len() == 5000 {
                break;
            }
            origin = if last.column.0 + 1 == self.term.columns() {
                Point::new(last.line + 1, Column(0))
            } else {
                Point::new(last.line, last.column + 1)
            };
        }
        matches
    }

    pub fn reveal_match(&mut self, found: &alacritty_terminal::term::search::Match) {
        use alacritty_terminal::grid::Scroll;
        let target = (-found.start().line.0 + self.term.screen_lines() as i32 / 2).max(0);
        let current = self.term.grid().display_offset() as i32;
        self.term.scroll_display(Scroll::Delta(target - current));
        self.select(*found.start(), *found.end());
    }

    pub fn link_at(&self, point: Point) -> Option<FileLink> {
        use alacritty_terminal::term::cell::Flags;
        if point.line < self.term.topmost_line() || point.line > self.term.bottommost_line() {
            return None;
        }
        let grid = self.term.grid();
        // Soft wraps belong to one reference; mouse cells must map back to UTF-8 byte offsets.
        let mut first = point.line;
        while first > self.term.topmost_line()
            && grid[Point::new(first - 1, self.term.last_column())]
                .flags
                .contains(Flags::WRAPLINE)
        {
            first -= 1;
            if point.line.0 - first.0 > 256 {
                return None;
            }
        }
        let mut line = first;
        let mut text = String::new();
        let mut clicked = None;
        loop {
            for column in 0..self.term.columns() {
                let cell = &grid[Point::new(line, Column(column))];
                if line == point.line && column == point.column.0 {
                    clicked = Some(if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        text.char_indices().last().map_or(0, |(ix, _)| ix)
                    } else {
                        text.len()
                    });
                }
                if !cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    text.push(cell.c);
                    if let Some(extra) = cell.zerowidth() {
                        text.extend(extra);
                    }
                }
            }
            if line == self.term.bottommost_line()
                || !grid[Point::new(line, self.term.last_column())]
                    .flags
                    .contains(Flags::WRAPLINE)
            {
                break;
            }
            line += 1;
            if line.0 - first.0 > 256 {
                return None;
            }
        }
        file_link(&text, clicked?)
    }
}

pub fn paste_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        // Embedded ESC must not terminate bracketed paste and execute the rest as keystrokes.
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
    } else {
        text.replace("\r\n", "\n").replace('\n', "\r").into_bytes()
    }
}

pub fn key_bytes(
    key: &str,
    control: bool,
    alt: bool,
    shift: bool,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
    let final_byte = match key {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(final_byte) = final_byte {
        let sequence = if modifier > 1 {
            format!("\x1b[1;{modifier}{final_byte}")
        } else if mode.contains(TermMode::APP_CURSOR) {
            format!("\x1bO{final_byte}")
        } else {
            format!("\x1b[{final_byte}")
        };
        return Some(sequence.into_bytes());
    }
    let number = match key {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    };
    if let Some(number) = number {
        return Some(
            if modifier == 1 {
                format!("\x1b[{number}~")
            } else {
                format!("\x1b[{number};{modifier}~")
            }
            .into_bytes(),
        );
    }
    if let Some(final_byte) = match key {
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    } {
        return Some(
            if modifier == 1 {
                format!("\x1bO{final_byte}")
            } else {
                format!("\x1b[1;{modifier}{final_byte}")
            }
            .into_bytes(),
        );
    }
    let mut bytes = match key {
        "enter" => vec![b'\r'],
        "escape" => vec![27],
        // Ctrl+W is understood by Readline, ZLE and PSReadLine (including Windows mode).
        "backspace" if control => vec![0x17],
        "backspace" => vec![127],
        "tab" if shift => b"\x1b[Z".to_vec(),
        "tab" => vec![b'\t'],
        "space" if control => vec![0],
        _ if control && key.len() == 1 => {
            let byte = key.as_bytes()[0].to_ascii_uppercase();
            if (b'@'..=b'_').contains(&byte) {
                vec![byte & 0x1f]
            } else {
                return None;
            }
        }
        _ if alt && key.chars().count() == 1 => key.as_bytes().to_vec(),
        _ => return None,
    };
    if alt && key != "escape" {
        bytes.insert(0, 27);
    }
    Some(bytes)
}

pub fn mouse_bytes(
    button: u8,
    column: usize,
    row: usize,
    released: bool,
    mode: TermMode,
) -> Vec<u8> {
    if mode.contains(TermMode::SGR_MOUSE) {
        format!(
            "\x1b[<{button};{};{}{}",
            column + 1,
            row + 1,
            if released { 'm' } else { 'M' }
        )
        .into_bytes()
    } else if column < 223 && row < 223 {
        vec![
            27,
            b'[',
            b'M',
            32 + if released { 3 } else { button },
            (column + 33) as u8,
            (row + 33) as u8,
        ]
    } else {
        Vec::new()
    }
}
