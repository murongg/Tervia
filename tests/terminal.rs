use alacritty_terminal::{
    index::{Column, Line, Point},
    term::TermMode,
};
use tervia::terminal::{TerminalBuffer, key_bytes, paste_bytes};

#[test]
fn terminal_search_finds_literal_unicode_and_wrapped_history_without_touching_output() {
    let mut terminal = TerminalBuffer::new(12, 3);
    terminal.feed("first mock[1]\r\n合成 mock[1]\r\nlast MOCK[1]\r\nend\r\n".as_bytes());
    let before = terminal.text();
    let matches = terminal.find("mock[1]");
    assert_eq!(matches.len(), 3);
    terminal.reveal_match(&matches[0]);
    assert_eq!(terminal.selected_text(), "mock[1]");
    assert!(terminal.term.grid().display_offset() > 0);
    assert_eq!(terminal.find("MOCK[1]").len(), 1);
    assert_eq!(terminal.find("合成").len(), 1);
    assert!(terminal.find("").is_empty());
    assert!(terminal.find("missing").is_empty());
    assert_eq!(terminal.text(), before);
}

#[test]
fn cursor_queries_produce_replies_across_split_pty_reads() {
    use alacritty_terminal::event::Event;
    let mut terminal = TerminalBuffer::new(40, 8);
    terminal.feed(b"\x1b[3;5H\x1b[6");
    assert!(
        !terminal
            .events()
            .iter()
            .any(|event| matches!(event, Event::PtyWrite(_)))
    );
    terminal.feed(b"n");
    let replies: Vec<_> = terminal
        .events()
        .into_iter()
        .filter_map(|event| match event {
            Event::PtyWrite(reply) => Some(reply),
            _ => None,
        })
        .collect();
    assert_eq!(replies, ["\x1b[3;5R"]);
}

#[test]
fn escape_streams_overwrite_cells_and_preserve_split_utf8() {
    let mut terminal = TerminalBuffer::new(40, 8);
    terminal.feed(b"hello\rHi");
    assert!(terminal.text().starts_with("Hillo"));
    terminal.feed(b"\x1b[2J\x1b[H");
    let bytes = "合成测试".as_bytes();
    terminal.feed(&bytes[..2]);
    terminal.feed(&bytes[2..]);
    assert_eq!(terminal.term.grid()[Point::new(Line(0), Column(0))].c, '合');
    assert_eq!(terminal.term.grid()[Point::new(Line(0), Column(2))].c, '成');
    assert!(terminal.text().contains("合成测试"));
    assert!(terminal.visible_text().starts_with("合成测试"));
}

#[test]
fn alternate_screen_restores_the_original_shell_and_resizes() {
    let mut terminal = TerminalBuffer::new(40, 8);
    terminal.feed(b"synthetic shell\x1b[?1049h\x1b[Hsynthetic tui");
    assert!(terminal.term.mode().contains(TermMode::ALT_SCREEN));
    assert!(terminal.text().contains("synthetic tui"));
    terminal.feed(b"\x1b[?1049l");
    assert!(terminal.text().contains("synthetic shell"));
    terminal.resize(72, 12);
    assert_eq!(terminal.size(), (72, 12));
}

#[test]
fn selection_copies_unicode_and_blank_lines_in_order() {
    let mut terminal = TerminalBuffer::new(40, 8);
    terminal.feed("first\r\n\r\n合成 last".as_bytes());
    terminal.select(
        Point::new(Line(0), Column(0)),
        Point::new(Line(2), Column(8)),
    );
    assert_eq!(terminal.selected_text(), "first\n\n合成 last");
}

#[test]
fn keyboard_and_paste_follow_terminal_modes() {
    assert_eq!(
        key_bytes("c", true, false, false, TermMode::empty()),
        Some(vec![3])
    );
    assert_eq!(
        key_bytes("escape", false, false, false, TermMode::empty()),
        Some(vec![27])
    );
    assert_eq!(
        key_bytes("up", false, false, false, TermMode::APP_CURSOR),
        Some(b"\x1bOA".to_vec())
    );
    assert_eq!(
        key_bytes("up", true, false, true, TermMode::empty()),
        Some(b"\x1b[1;6A".to_vec())
    );
    assert_eq!(paste_bytes("mock\ntext", TermMode::empty()), b"mock\rtext");
    assert_eq!(
        paste_bytes("合成\x1b[201~", TermMode::BRACKETED_PASTE),
        "\x1b[200~合成[201~\x1b[201~".as_bytes()
    );
}

#[test]
fn terminal_editing_keys_preserve_cross_platform_word_line_and_control_sequences() {
    for mode in [
        TermMode::empty(),
        TermMode::APP_CURSOR,
        TermMode::ALT_SCREEN,
    ] {
        assert_eq!(
            key_bytes("backspace", true, false, false, mode),
            Some(vec![0x17]),
            "Ctrl+Backspace should request backward word deletion"
        );
        assert_eq!(
            key_bytes("backspace", false, true, false, mode),
            Some(vec![0x1b, 0x7f])
        );
        assert_eq!(
            key_bytes("left", true, false, false, mode),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            key_bytes("right", true, false, false, mode),
            Some(b"\x1b[1;5C".to_vec())
        );
        assert_eq!(
            key_bytes("delete", true, false, false, mode),
            Some(b"\x1b[3;5~".to_vec())
        );
        assert_eq!(key_bytes("u", true, false, false, mode), Some(vec![0x15]));
        assert_eq!(key_bytes("a", true, false, false, mode), Some(vec![0x01]));
        assert_eq!(key_bytes("e", true, false, false, mode), Some(vec![0x05]));
        assert_eq!(key_bytes("c", true, false, false, mode), Some(vec![0x03]));
        assert_eq!(key_bytes("v", true, false, false, mode), Some(vec![0x16]));
    }
}

#[test]
fn shell_cursor_is_a_blinking_beam_and_programs_can_change_its_shape() {
    use alacritty_terminal::vte::ansi::CursorShape;
    let mut terminal = TerminalBuffer::new(40, 8);
    assert_eq!(terminal.term.cursor_style().shape, CursorShape::Beam);
    assert!(terminal.term.cursor_style().blinking);
    terminal.feed(b"\x1b[2 q");
    assert_eq!(terminal.term.cursor_style().shape, CursorShape::Block);
    assert!(!terminal.term.cursor_style().blinking);
    terminal.feed(b"\x1b[0 q");
    assert_eq!(terminal.term.cursor_style().shape, CursorShape::Beam);
}

#[test]
fn reusable_search_preserves_grid_matches_across_output_resize_and_screen_switches() {
    use alacritty_terminal::{grid::Dimensions, term::search::RegexSearch};
    use tervia::terminal::Search;
    fn reference(terminal: &TerminalBuffer, query: &str) -> Vec<std::ops::RangeInclusive<Point>> {
        let mut regex = RegexSearch::new(&regex::escape(query)).unwrap();
        let mut origin = Point::new(terminal.term.topmost_line(), Column(0));
        let end = Point::new(terminal.term.bottommost_line(), terminal.term.last_column());
        let mut matches = Vec::new();
        while let Some(found) = terminal.term.regex_search_right(&mut regex, origin, end) {
            let last = *found.end();
            matches.push(found);
            if last >= end || matches.len() == 5000 {
                break;
            }
            origin = if last.column.0 + 1 == terminal.term.columns() {
                Point::new(last.line + 1, Column(0))
            } else {
                Point::new(last.line, last.column + 1)
            };
        }
        matches
    }
    let mut terminal = TerminalBuffer::new(12, 3);
    let chunks: &[&[u8]] = &[
        b"mock[1] MOCK[1]\r\n",
        b"abcde abcde abcde\r\n",
        b"a\r\nb\r\n",
        "合成 mock[1] e\u{301}\r\n".as_bytes(),
        b"\x1b[?1049hMOCK[1]",
        b"\x1b[?1049l",
        b"\x1b[3J\x1b[2J\x1b[Hmock[1]",
    ];
    for query in [
        "mock[1]", "MOCK[1]", "abcde", "e a", "a\nb", "合成", " ", "e",
    ] {
        let mut search = Search::new(query);
        for chunk in chunks {
            terminal.feed(chunk);
            assert_eq!(
                search.find(&terminal),
                reference(&terminal, query),
                "query={query:?}, chunk={chunk:?}"
            );
        }
        terminal.resize(7, 5);
        assert_eq!(search.find(&terminal), reference(&terminal, query));
    }
}

#[test]
fn reusable_search_keeps_the_match_limit_and_rejects_empty_or_oversized_queries() {
    use tervia::terminal::Search;
    let mut terminal = TerminalBuffer::new(80, 4);
    terminal.feed("mock ".repeat(6000).as_bytes());
    assert_eq!(Search::new("mock").find(&terminal).len(), 5000);
    assert!(Search::new("").find(&terminal).is_empty());
    assert!(Search::new(&"x".repeat(4097)).find(&terminal).is_empty());
}
