#![cfg(feature = "native")]

use gpui_kit::component::{
    Rope,
    diff::DiffFile,
    highlighter::{HighlightTheme, SyntaxHighlighter},
};

#[test]
fn bundled_languages_parse_and_color_source_in_both_themes() {
    let mut failures = Vec::new();
    for (language, source) in [
        ("astro", "---\nconst count = 42;\n---\n<h1>Mock</h1>\n"),
        ("bash", "echo \"mock\"\n"),
        ("c", "int count = 42;\n"),
        ("cmake", "set(MOCK_VALUE \"mock\")\n"),
        ("cpp", "const int count = 42;\n"),
        ("csharp", "class Mock { int Count = 42; }\n"),
        ("css", "p { color: red; }\n"),
        ("diff", "--- a/mock\n+++ b/mock\n@@ -1 +1 @@\n-old\n+new\n"),
        ("ejs", "<p><%= \"mock\" %></p>\n"),
        ("elixir", "defmodule Mock do\n  def count, do: 42\nend\n"),
        ("erb", "<p><%= \"mock\" %></p>\n"),
        ("go", "package mock\nconst Count = 42\n"),
        ("graphql", "query Mock { mock(id: 1) { name } }\n"),
        ("html", "<p class=\"mock\">Mock</p>\n"),
        ("java", "class Mock { int count = 42; }\n"),
        ("javascript", "const mock = <p>Mock</p>;\n"),
        ("jsdoc", "/**\n * @param {string} mock\n */"),
        ("json", "{\"mock\": 42}\n"),
        ("kotlin", "val count = 42\n"),
        ("lua", "local count = 42\n"),
        ("make", "MOCK = value\nall:\n\techo $(MOCK)\n"),
        ("markdown", "# Mock\n\n**mock**\n"),
        ("markdown_inline", "[mock](https://example.test)"),
        ("php", "<?php $count = 42; echo \"mock\"; ?>\n"),
        (
            "proto",
            "syntax = \"proto3\"; message Mock { string name = 1; }\n",
        ),
        ("python", "def mock():\n    return 42\n"),
        ("ruby", "def mock\n  42\nend\n"),
        ("rust", "fn mock() -> u32 { 42 }\n"),
        ("scala", "object Mock { val count = 42 }\n"),
        ("sql", "SELECT 'mock' AS name;\n"),
        (
            "svelte",
            "<script>let count = 42;</script>\n<p>{count}</p>\n",
        ),
        ("swift", "let count = 42\n"),
        ("toml", "mock = 42\n"),
        ("tsx", "const mock = <p>Mock</p>;\n"),
        ("typescript", "const count: number = 42;\n"),
        ("yaml", "mock: 42\n"),
        ("zig", "const count: u32 = 42;\n"),
    ] {
        let mut highlighter = SyntaxHighlighter::new(language);
        if highlighter.language().as_str() != language {
            failures.push(format!("{language}: fell back to plain text"));
            continue;
        }
        highlighter.update(None, &Rope::from(source), None);
        let tree = highlighter.tree().expect("An enabled grammar must parse");
        if tree.root_node().has_error() {
            failures.push(format!(
                "{language}: invalid parse {}",
                tree.root_node().to_sexp()
            ));
        }
        for theme in [
            HighlightTheme::default_dark(),
            HighlightTheme::default_light(),
        ] {
            if !highlighter
                .styles(&(0..source.len()), theme.as_ref())
                .iter()
                .any(|(_, style)| style.color.is_some())
            {
                failures.push(format!("{language}: no colored tokens"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn diff_detects_language_aliases_and_keeps_unknown_files_plain() {
    for (path, expected) in [
        ("src/mock.PY", "python"),
        ("mock.jsx", "javascript"),
        ("mock.mjs", "javascript"),
        ("mock.cjs", "javascript"),
        ("mock.tsx", "tsx"),
        ("mock.mts", "typescript"),
        ("mock.cts", "typescript"),
        ("mock.jsonc", "json"),
        ("mock.yml", "yaml"),
        ("mock.markdown", "markdown"),
        ("C:\\mock\\header.hpp", "cpp"),
        ("mock.hh", "cpp"),
        ("mock.cs", "csharp"),
        ("mock.gql", "graphql"),
        ("mock.exs", "elixir"),
        ("mock.pyi", "python"),
        ("mock.mk", "make"),
        ("build/Makefile", "make"),
        ("build/GNUmakefile", "make"),
        ("build/CMakeLists.txt", "cmake"),
        ("Cargo.lock", "toml"),
        ("Gemfile", "ruby"),
        (".bashrc", "bash"),
        ("directory.rs/README", "text"),
        ("mock.unknown", "text"),
        (".gitignore", "text"),
    ] {
        assert_eq!(
            DiffFile::unchanged(path, "").language().as_str(),
            expected,
            "{path}"
        );
    }
}

#[test]
fn jsx_tags_and_embedded_languages_keep_their_token_styles() {
    for (language, source, token, capture) in [
        (
            "javascript",
            "const mock = <section title=\"mock\" />;",
            "section",
            "tag",
        ),
        (
            "tsx",
            "const mock = <section title=\"mock\" />;",
            "section",
            "tag",
        ),
        ("tsx", "const count: number = 42;", "const", "keyword"),
        ("tsx", "const count: number = 42;", "42", "number"),
        (
            "markdown",
            "```python\ndef mock():\n    return 42\n```\n",
            "return",
            "keyword",
        ),
        (
            "html",
            "<script>const count = 42;</script>",
            "const",
            "keyword",
        ),
        ("ejs", "<% const count = 42; %>", "const", "keyword"),
        (
            "erb",
            "<% unless false %><p>Mock</p><% end %>",
            "unless",
            "keyword",
        ),
    ] {
        let mut highlighter = SyntaxHighlighter::new(language);
        highlighter.update(None, &Rope::from(source), None);
        let offset = source.find(token).unwrap();
        for theme in [
            HighlightTheme::default_dark(),
            HighlightTheme::default_light(),
        ] {
            let expected = theme.style.syntax.style(capture).unwrap().color;
            assert!(expected.is_some());
            assert!(
                highlighter
                    .styles(&(0..source.len()), theme.as_ref())
                    .iter()
                    .any(|(range, style)| {
                        range.start <= offset
                            && range.end >= offset + token.len()
                            && style.color == expected
                    }),
                "{language}: {token} must use the {capture} style"
            );
        }
    }
}
