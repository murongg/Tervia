use alacritty_terminal::{
    index::{Column, Line, Point},
    term::TermMode,
};
use tervia::terminal::{TerminalBuffer, key_bytes, paste_bytes};

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
