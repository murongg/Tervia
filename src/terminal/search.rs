use super::TerminalBuffer;
use alacritty_terminal::{
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{
        cell::Flags,
        search::{Match, RegexSearch},
    },
};

/// Reuses the query's matcher and scratch space across output batches.
pub struct Search {
    query: String,
    literal: Option<regex::Regex>,
    grid: Option<RegexSearch>,
    bytes: Vec<u8>,
    rows: Vec<usize>,
    fallback: Option<Point>,
}

impl Search {
    pub fn new(query: &str) -> Self {
        let valid = !query.is_empty() && query.len() <= 4096;
        let literal = (valid && query.is_ascii() && !query.contains('\n'))
            .then(|| {
                regex::RegexBuilder::new(&regex::escape(query))
                    .case_insensitive(!query.chars().any(char::is_uppercase))
                    .build()
                    .ok()
            })
            .flatten();
        Self {
            query: if valid { query.into() } else { String::new() },
            literal,
            grid: None,
            bytes: Vec::new(),
            rows: Vec::new(),
            fallback: None,
        }
    }

    pub fn find(&mut self, buffer: &TerminalBuffer) -> Vec<Match> {
        if self.query.is_empty() {
            return Vec::new();
        }
        if self.literal.is_some() && self.ascii(buffer) {
            let text = std::str::from_utf8(&self.bytes).expect("ASCII terminal search");
            let top = buffer.term.topmost_line().0;
            let point = |offset: usize| {
                let row = self.rows.partition_point(|start| *start <= offset) - 1;
                Point::new(Line(top + row as i32), Column(offset - self.rows[row]))
            };
            return self
                .literal
                .as_ref()
                .unwrap()
                .find_iter(text)
                .take(5000)
                .map(|found| point(found.start())..=point(found.end() - 1))
                .collect();
        }
        // Wide characters, Unicode case folding and unusual grid states retain
        // Alacritty's exact cell mapping rather than approximating byte positions.
        if self.grid.is_none() {
            self.grid = RegexSearch::new(&regex::escape(&self.query)).ok();
        }
        let Some(regex) = &mut self.grid else {
            return Vec::new();
        };
        let term = &buffer.term;
        let mut origin = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        let mut matches = Vec::new();
        while let Some(found) = term.regex_search_right(regex, origin, end) {
            let last = *found.end();
            matches.push(found);
            if last >= end || matches.len() == 5000 {
                break;
            }
            origin = if last.column.0 + 1 == term.columns() {
                Point::new(last.line + 1, Column(0))
            } else {
                Point::new(last.line, last.column + 1)
            };
        }
        matches
    }

    fn ascii(&mut self, buffer: &TerminalBuffer) -> bool {
        self.bytes.clear();
        self.rows.clear();
        let term = &buffer.term;
        let unsupported = |cell: &alacritty_terminal::term::cell::Cell| {
            !cell.c.is_ascii()
                || cell.flags.intersects(
                    Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
                )
        };
        // Remember only a reason to use the original matcher, never cached results.
        if let Some(point) = self.fallback
            && point.line >= term.topmost_line()
            && point.line <= term.bottommost_line()
            && point.column < Column(term.columns())
            && unsupported(&term.grid()[point])
        {
            return false;
        }
        for row in term.topmost_line().0..=term.bottommost_line().0 {
            self.rows.push(self.bytes.len());
            let line = &term.grid()[Line(row)];
            for column in 0..term.columns() {
                let cell = &line[Column(column)];
                if unsupported(cell) {
                    self.fallback = Some(Point::new(Line(row), Column(column)));
                    return false;
                }
                self.bytes.push(cell.c as u8);
            }
            // Hard line breaks reset Alacritty's matcher; only soft wraps join cells.
            if !line[term.last_column()].flags.contains(Flags::WRAPLINE) {
                self.bytes.push(b'\n');
            }
        }
        self.fallback = None;
        true
    }
}
