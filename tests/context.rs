use std::path::Path;
use tervia::files::{code_context, selection_context};

#[test]
fn selected_context_uses_draft_bytes_and_exclusive_end_line() {
    let draft = "first\r\n合成 second\r\nlast\r\n";
    let start = draft.find("合成").unwrap();
    let end = draft.find("last").unwrap();
    assert_eq!(
        selection_context(Path::new("src/mock.rs"), draft, start..end),
        Some("src/mock.rs:2\n```\n合成 second\r\n```".into())
    );
    assert_eq!(
        selection_context(Path::new("src/mock.rs"), draft, start..draft.len()),
        Some("src/mock.rs:2-3\n```\n合成 second\r\nlast\r\n```".into())
    );
    assert!(selection_context(Path::new("mock.rs"), draft, start..start).is_none());
    assert!(selection_context(Path::new("mock.rs"), draft, start + 1..end).is_none());
    assert!(selection_context(Path::new("mock.rs"), draft, 0..usize::MAX).is_none());
}

#[test]
fn context_preserves_partial_lines_and_escapes_embedded_code_fences() {
    assert_eq!(
        selection_context(
            Path::new("src/mock.rs"),
            "first\nlet mock = 42;\nlast",
            10..14
        ),
        Some("src/mock.rs:2\n```\nmock\n```".into())
    );
    assert_eq!(
        code_context(
            Path::new("mock.md"),
            9,
            11,
            Some("original"),
            "```rust\nsynthetic\n```"
        ),
        "mock.md:9-11 (original)\n````\n```rust\nsynthetic\n```\n````"
    );
}
