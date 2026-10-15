use std::{fs, path::Path};

use alacritty_terminal::index::{Column, Line, Point};
use tervia::terminal::{FileLink, TerminalBuffer, file_link};

#[test]
fn links_parse_locations_and_only_match_the_clicked_reference() {
    for (text, needle, path, line, column) in [
        ("error at src/mock.rs:42:7:", "mock", "src/mock.rs", 42, 7),
        ("see `src/mock.rs:8` please", "mock", "src/mock.rs", 8, 1),
        (
            "  --> /synthetic/src/mock.rs:2:3",
            "mock",
            "/synthetic/src/mock.rs",
            2,
            3,
        ),
        (
            "open \"src/mock file.rs\":9:2",
            "file",
            "src/mock file.rs",
            9,
            2,
        ),
        ("open README.md", "README", "README.md", 1, 1),
        (
            "C:\\mock\\src\\file.rs:12:4",
            "file",
            "C:\\mock\\src\\file.rs",
            12,
            4,
        ),
        ("查看 合成/mock.rs:3", "mock", "合成/mock.rs", 3, 1),
    ] {
        let link = file_link(text, text.find(needle).unwrap()).unwrap();
        assert_eq!(link.path.to_string_lossy(), path, "{text}");
        assert_eq!((link.line, link.column), (line, column), "{text}");
        assert!(file_link(text, text.len()).is_none());
    }
    for text in [
        "https://example.test/mock.rs:4",
        "127.0.0.1:8000",
        "mock.rs:0",
        "count=42",
        "mock.rs:999999999999999999999999999999",
    ] {
        assert!(file_link(text, 1).is_none(), "{text}");
    }
    assert!(file_link("src/mock.rs:2 ordinary text", 18).is_none());
}

#[test]
fn links_resolve_inside_the_project_and_reject_missing_or_external_files() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/mock.rs"), "synthetic\n").unwrap();
    fs::write(outside.path().join("external.rs"), "synthetic\n").unwrap();
    let link = |path| FileLink {
        path,
        line: 2,
        column: 3,
    };
    assert_eq!(
        link("src/mock.rs".into()).resolve(root.path()).unwrap(),
        Path::new("src/mock.rs")
    );
    assert_eq!(
        link(root.path().join("src/mock.rs"))
            .resolve(root.path())
            .unwrap(),
        Path::new("src/mock.rs")
    );
    assert!(link("missing.rs".into()).resolve(root.path()).is_err());
    assert!(link("src".into()).resolve(root.path()).is_err());
    assert!(
        link(outside.path().join("external.rs"))
            .resolve(root.path())
            .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            outside.path().join("external.rs"),
            root.path().join("escape.rs"),
        )
        .unwrap();
        assert!(link("escape.rs".into()).resolve(root.path()).is_err());
    }
}

#[test]
fn terminal_links_follow_wrapping_unicode_and_scrollback_without_changing_selection() {
    let mut terminal = TerminalBuffer::new(18, 3);
    terminal.feed("合成 src/mock.rs:42:7\r\nother\r\nlast\r\n".as_bytes());
    terminal.select(
        Point::new(Line(0), Column(0)),
        Point::new(Line(0), Column(2)),
    );
    let selection = terminal.selected_text();
    let link = terminal.link_at(Point::new(Line(-1), Column(1))).unwrap();
    assert_eq!(link.path.to_string_lossy(), "src/mock.rs");
    assert_eq!((link.line, link.column), (42, 7));
    assert_eq!(terminal.selected_text(), selection);
}
