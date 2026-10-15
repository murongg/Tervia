use gpui::SharedString;

/// Detect the grammar name for editors and diffs from a file path.
/// Paths may come from another platform; dots in directories are not extensions.
pub fn language_for_path(path: &str) -> SharedString {
    let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    let language = match name.as_str() {
        "makefile" | "gnumakefile" => "make",
        "cmakelists.txt" => "cmake",
        "cargo.lock" => "toml",
        "gemfile" | "rakefile" => "ruby",
        ".bashrc" | ".bash_profile" | ".bash_login" | ".profile" => "bash",
        _ => match name.rsplit_once('.').map(|(_, extension)| extension) {
            Some("jsx" | "mjs" | "cjs") => "javascript",
            Some("mts" | "cts") => "typescript",
            Some("cc" | "cxx" | "hpp" | "hxx" | "hh") => "cpp",
            Some("h") => "c",
            Some("htm") => "html",
            Some("gql") => "graphql",
            Some("exs") => "elixir",
            Some("mk") => "make",
            Some("markdown") => "markdown",
            Some("rake") => "ruby",
            Some(
                extension @ ("astro" | "sh" | "bash" | "c" | "cmake" | "cs" | "cpp" | "css"
                | "scss" | "diff" | "ejs" | "ex" | "erb" | "go" | "graphql" | "html"
                | "java" | "js" | "json" | "jsonc" | "kt" | "kts" | "lua" | "md"
                | "mdx" | "php" | "php3" | "php4" | "php5" | "phtml" | "proto" | "py"
                | "pyi" | "rb" | "rs" | "scala" | "sql" | "svelte" | "swift" | "toml"
                | "tsx" | "ts" | "yaml" | "yml" | "zig"),
            ) => extension,
            _ => "text",
        },
    };
    language_name(language)
}

/// Built-in aliases for editing defaults when no exact grammar name is registered.
/// Availability of a parser does not change what a language name means.
pub(crate) fn language_name(name: &str) -> SharedString {
    let name = name.to_lowercase();
    match name.as_str() {
        "plain" => "text".into(),
        "plaintext" => "text".into(),
        "jsonc" => "json".into(),
        "sh" => "bash".into(),
        "c++" => "cpp".into(),
        "cs" => "csharp".into(),
        "scss" => "css".into(),
        "ex" => "elixir".into(),
        "js" => "javascript".into(),
        "kt" => "kotlin".into(),
        "kts" => "kotlin".into(),
        "ktm" => "kotlin".into(),
        "makefile" => "make".into(),
        "md" => "markdown".into(),
        "mdx" => "markdown".into(),
        "markdown-inline" => "markdown_inline".into(),
        "php3" => "php".into(),
        "php4" => "php".into(),
        "php5" => "php".into(),
        "phtml" => "php".into(),
        "protobuf" => "proto".into(),
        "py" => "python".into(),
        "rb" => "ruby".into(),
        "rs" => "rust".into(),
        "ts" => "typescript".into(),
        "yml" => "yaml".into(),
        "pyi" => "python".into(),
        _ => name.into(),
    }
}
