use gpui::SharedString;
use std::borrow::Cow;

use crate::highlighter::GrammarConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_iterator::Sequence)]
pub enum Language {
    Json,
    Plain,
    #[cfg(feature = "tree-sitter-dart")]
    Dart,
    #[cfg(feature = "tree-sitter-dockerfile")]
    Dockerfile,
    #[cfg(feature = "tree-sitter-fish")]
    Fish,
    #[cfg(feature = "tree-sitter-hcl")]
    Hcl,
    #[cfg(feature = "tree-sitter-ini")]
    Ini,
    #[cfg(feature = "tree-sitter-json5")]
    Json5,
    #[cfg(feature = "tree-sitter-less")]
    Less,
    #[cfg(feature = "tree-sitter-nix")]
    Nix,
    #[cfg(feature = "tree-sitter-powershell")]
    PowerShell,
    #[cfg(feature = "tree-sitter-scss")]
    Scss,
    #[cfg(feature = "tree-sitter-vue")]
    Vue,
    #[cfg(feature = "tree-sitter-xml")]
    Xml,

    #[cfg(feature = "tree-sitter-astro")]
    Astro,
    #[cfg(feature = "tree-sitter-bash")]
    Bash,
    #[cfg(feature = "tree-sitter-c")]
    C,
    #[cfg(feature = "tree-sitter-cmake")]
    CMake,
    #[cfg(feature = "tree-sitter-csharp")]
    CSharp,
    #[cfg(feature = "tree-sitter-cpp")]
    Cpp,
    #[cfg(feature = "tree-sitter-css")]
    Css,
    #[cfg(feature = "tree-sitter-diff")]
    Diff,
    #[cfg(feature = "tree-sitter-ejs")]
    Ejs,
    #[cfg(feature = "tree-sitter-elixir")]
    Elixir,
    #[cfg(feature = "tree-sitter-erb")]
    Erb,
    #[cfg(feature = "tree-sitter-go")]
    Go,
    #[cfg(feature = "tree-sitter-graphql")]
    GraphQL,
    #[cfg(feature = "tree-sitter-html")]
    Html,
    #[cfg(feature = "tree-sitter-java")]
    Java,
    #[cfg(feature = "tree-sitter-javascript")]
    JavaScript,
    #[cfg(feature = "tree-sitter-jsdoc")]
    JsDoc,
    #[cfg(feature = "tree-sitter-kotlin")]
    Kotlin,
    #[cfg(feature = "tree-sitter-lua")]
    Lua,
    #[cfg(feature = "tree-sitter-make")]
    Make,
    #[cfg(feature = "tree-sitter-markdown")]
    Markdown,
    #[cfg(feature = "tree-sitter-markdown")]
    MarkdownInline,
    #[cfg(feature = "tree-sitter-php")]
    Php,
    #[cfg(feature = "tree-sitter-proto")]
    Proto,
    #[cfg(feature = "tree-sitter-python")]
    Python,
    #[cfg(feature = "tree-sitter-ruby")]
    Ruby,
    #[cfg(feature = "tree-sitter-rust")]
    Rust,
    #[cfg(feature = "tree-sitter-scala")]
    Scala,
    #[cfg(feature = "tree-sitter-sql")]
    Sql,
    #[cfg(feature = "tree-sitter-svelte")]
    Svelte,
    #[cfg(feature = "tree-sitter-swift")]
    Swift,
    #[cfg(feature = "tree-sitter-toml")]
    Toml,
    #[cfg(feature = "tree-sitter-tsx")]
    Tsx,
    #[cfg(feature = "tree-sitter-typescript")]
    TypeScript,
    #[cfg(feature = "tree-sitter-yaml")]
    Yaml,
    #[cfg(feature = "tree-sitter-zig")]
    Zig,
}

impl From<Language> for SharedString {
    fn from(language: Language) -> Self {
        language.name().into()
    }
}

impl Language {
    pub fn all() -> impl Iterator<Item = Self> {
        enum_iterator::all::<Language>()
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Plain => "text",
            #[cfg(feature = "tree-sitter-dart")]
            Self::Dart => "dart",
            #[cfg(feature = "tree-sitter-dockerfile")]
            Self::Dockerfile => "dockerfile",
            #[cfg(feature = "tree-sitter-fish")]
            Self::Fish => "fish",
            #[cfg(feature = "tree-sitter-hcl")]
            Self::Hcl => "hcl",
            #[cfg(feature = "tree-sitter-ini")]
            Self::Ini => "ini",
            #[cfg(feature = "tree-sitter-json5")]
            Self::Json5 => "json5",
            #[cfg(feature = "tree-sitter-less")]
            Self::Less => "less",
            #[cfg(feature = "tree-sitter-nix")]
            Self::Nix => "nix",
            #[cfg(feature = "tree-sitter-powershell")]
            Self::PowerShell => "powershell",
            #[cfg(feature = "tree-sitter-scss")]
            Self::Scss => "scss",
            #[cfg(feature = "tree-sitter-vue")]
            Self::Vue => "vue",
            #[cfg(feature = "tree-sitter-xml")]
            Self::Xml => "xml",

            #[cfg(feature = "tree-sitter-astro")]
            Self::Astro => "astro",
            #[cfg(feature = "tree-sitter-bash")]
            Self::Bash => "bash",
            #[cfg(feature = "tree-sitter-c")]
            Self::C => "c",
            #[cfg(feature = "tree-sitter-cmake")]
            Self::CMake => "cmake",
            #[cfg(feature = "tree-sitter-csharp")]
            Self::CSharp => "csharp",
            #[cfg(feature = "tree-sitter-cpp")]
            Self::Cpp => "cpp",
            #[cfg(feature = "tree-sitter-css")]
            Self::Css => "css",
            #[cfg(feature = "tree-sitter-diff")]
            Self::Diff => "diff",
            #[cfg(feature = "tree-sitter-ejs")]
            Self::Ejs => "ejs",
            #[cfg(feature = "tree-sitter-elixir")]
            Self::Elixir => "elixir",
            #[cfg(feature = "tree-sitter-erb")]
            Self::Erb => "erb",
            #[cfg(feature = "tree-sitter-go")]
            Self::Go => "go",
            #[cfg(feature = "tree-sitter-graphql")]
            Self::GraphQL => "graphql",
            #[cfg(feature = "tree-sitter-html")]
            Self::Html => "html",
            #[cfg(feature = "tree-sitter-java")]
            Self::Java => "java",
            #[cfg(feature = "tree-sitter-javascript")]
            Self::JavaScript => "javascript",
            #[cfg(feature = "tree-sitter-jsdoc")]
            Self::JsDoc => "jsdoc",
            #[cfg(feature = "tree-sitter-kotlin")]
            Self::Kotlin => "kotlin",
            #[cfg(feature = "tree-sitter-lua")]
            Self::Lua => "lua",
            #[cfg(feature = "tree-sitter-make")]
            Self::Make => "make",
            #[cfg(feature = "tree-sitter-markdown")]
            Self::Markdown => "markdown",
            #[cfg(feature = "tree-sitter-markdown")]
            Self::MarkdownInline => "markdown_inline",
            #[cfg(feature = "tree-sitter-php")]
            Self::Php => "php",
            #[cfg(feature = "tree-sitter-proto")]
            Self::Proto => "proto",
            #[cfg(feature = "tree-sitter-python")]
            Self::Python => "python",
            #[cfg(feature = "tree-sitter-ruby")]
            Self::Ruby => "ruby",
            #[cfg(feature = "tree-sitter-rust")]
            Self::Rust => "rust",
            #[cfg(feature = "tree-sitter-scala")]
            Self::Scala => "scala",
            #[cfg(feature = "tree-sitter-sql")]
            Self::Sql => "sql",
            #[cfg(feature = "tree-sitter-svelte")]
            Self::Svelte => "svelte",
            #[cfg(feature = "tree-sitter-swift")]
            Self::Swift => "swift",
            #[cfg(feature = "tree-sitter-toml")]
            Self::Toml => "toml",
            #[cfg(feature = "tree-sitter-tsx")]
            Self::Tsx => "tsx",
            #[cfg(feature = "tree-sitter-typescript")]
            Self::TypeScript => "typescript",
            #[cfg(feature = "tree-sitter-yaml")]
            Self::Yaml => "yaml",
            #[cfg(feature = "tree-sitter-zig")]
            Self::Zig => "zig",
        }
    }

    #[allow(unused)]
    pub fn from_str(s: &str) -> Self {
        Self::from_name(s).unwrap_or(Self::Plain)
    }

    pub(crate) fn from_name(s: &str) -> Option<Self> {
        match s {
            "text" | "plain" | "plaintext" => Some(Self::Plain),
            "json" | "jsonc" => Some(Self::Json),
            #[cfg(feature = "tree-sitter-dart")]
            "dart" => Some(Self::Dart),
            #[cfg(feature = "tree-sitter-dockerfile")]
            "dockerfile" => Some(Self::Dockerfile),
            #[cfg(feature = "tree-sitter-fish")]
            "fish" => Some(Self::Fish),
            #[cfg(feature = "tree-sitter-hcl")]
            "hcl" => Some(Self::Hcl),
            #[cfg(feature = "tree-sitter-ini")]
            "ini" => Some(Self::Ini),
            #[cfg(feature = "tree-sitter-json5")]
            "json5" => Some(Self::Json5),
            #[cfg(feature = "tree-sitter-less")]
            "less" => Some(Self::Less),
            #[cfg(feature = "tree-sitter-nix")]
            "nix" => Some(Self::Nix),
            #[cfg(feature = "tree-sitter-powershell")]
            "powershell" => Some(Self::PowerShell),
            #[cfg(feature = "tree-sitter-scss")]
            "scss" => Some(Self::Scss),
            #[cfg(feature = "tree-sitter-vue")]
            "vue" => Some(Self::Vue),
            #[cfg(feature = "tree-sitter-xml")]
            "xml" => Some(Self::Xml),

            #[cfg(feature = "tree-sitter-astro")]
            "astro" => Some(Self::Astro),
            #[cfg(feature = "tree-sitter-bash")]
            "bash" | "sh" => Some(Self::Bash),
            #[cfg(feature = "tree-sitter-c")]
            "c" => Some(Self::C),
            #[cfg(feature = "tree-sitter-cmake")]
            "cmake" => Some(Self::CMake),
            #[cfg(feature = "tree-sitter-cpp")]
            "cpp" | "c++" => Some(Self::Cpp),
            #[cfg(feature = "tree-sitter-csharp")]
            "csharp" | "cs" => Some(Self::CSharp),
            #[cfg(feature = "tree-sitter-css")]
            "css" => Some(Self::Css),
            #[cfg(feature = "tree-sitter-diff")]
            "diff" => Some(Self::Diff),
            #[cfg(feature = "tree-sitter-ejs")]
            "ejs" => Some(Self::Ejs),
            #[cfg(feature = "tree-sitter-elixir")]
            "elixir" | "ex" => Some(Self::Elixir),
            #[cfg(feature = "tree-sitter-erb")]
            "erb" => Some(Self::Erb),
            #[cfg(feature = "tree-sitter-go")]
            "go" => Some(Self::Go),
            #[cfg(feature = "tree-sitter-graphql")]
            "graphql" => Some(Self::GraphQL),
            #[cfg(feature = "tree-sitter-html")]
            "html" => Some(Self::Html),
            #[cfg(feature = "tree-sitter-java")]
            "java" => Some(Self::Java),
            #[cfg(feature = "tree-sitter-javascript")]
            "javascript" | "js" | "jsx" => Some(Self::JavaScript),
            #[cfg(feature = "tree-sitter-jsdoc")]
            "jsdoc" => Some(Self::JsDoc),
            #[cfg(feature = "tree-sitter-kotlin")]
            "kt" | "kts" | "ktm" | "kotlin" => Some(Self::Kotlin),
            #[cfg(feature = "tree-sitter-lua")]
            "lua" => Some(Self::Lua),
            #[cfg(feature = "tree-sitter-make")]
            "make" | "makefile" => Some(Self::Make),
            #[cfg(feature = "tree-sitter-markdown")]
            "markdown" | "md" | "mdx" => Some(Self::Markdown),
            #[cfg(feature = "tree-sitter-markdown")]
            "markdown_inline" | "markdown-inline" => Some(Self::MarkdownInline),
            #[cfg(feature = "tree-sitter-php")]
            "php" | "php3" | "php4" | "php5" | "phtml" => Some(Self::Php),
            #[cfg(feature = "tree-sitter-proto")]
            "proto" | "protobuf" => Some(Self::Proto),
            #[cfg(feature = "tree-sitter-python")]
            "python" | "py" => Some(Self::Python),
            #[cfg(feature = "tree-sitter-ruby")]
            "ruby" | "rb" => Some(Self::Ruby),
            #[cfg(feature = "tree-sitter-rust")]
            "rust" | "rs" => Some(Self::Rust),
            #[cfg(feature = "tree-sitter-scala")]
            "scala" => Some(Self::Scala),
            #[cfg(feature = "tree-sitter-sql")]
            "sql" => Some(Self::Sql),
            #[cfg(feature = "tree-sitter-svelte")]
            "svelte" => Some(Self::Svelte),
            #[cfg(feature = "tree-sitter-swift")]
            "swift" => Some(Self::Swift),
            #[cfg(feature = "tree-sitter-toml")]
            "toml" => Some(Self::Toml),
            #[cfg(feature = "tree-sitter-tsx")]
            "tsx" => Some(Self::Tsx),
            #[cfg(feature = "tree-sitter-typescript")]
            "typescript" | "ts" => Some(Self::TypeScript),
            #[cfg(feature = "tree-sitter-yaml")]
            "yaml" | "yml" => Some(Self::Yaml),
            #[cfg(feature = "tree-sitter-zig")]
            "zig" => Some(Self::Zig),
            _ => None,
        }
    }

    #[allow(unused)]
    pub(super) fn injection_languages(&self) -> Vec<SharedString> {
        let mut languages: Vec<&'static str> = Vec::new();

        match self {
            #[cfg(feature = "tree-sitter-markdown")]
            Self::Markdown => {
                languages.push("markdown_inline");
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-toml")]
                languages.push("toml");
                #[cfg(feature = "tree-sitter-yaml")]
                languages.push("yaml");
            }
            #[cfg(feature = "tree-sitter-vue")]
            Self::Vue => {
                #[cfg(feature = "tree-sitter-javascript")]
                languages.push("javascript");
                #[cfg(feature = "tree-sitter-typescript")]
                languages.push("typescript");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
                #[cfg(feature = "tree-sitter-scss")]
                languages.push("scss");
                #[cfg(feature = "tree-sitter-less")]
                languages.push("less");
                #[cfg(feature = "tree-sitter-tsx")]
                languages.push("tsx");
            }
            #[cfg(feature = "tree-sitter-html")]
            Self::Html => {
                #[cfg(feature = "tree-sitter-javascript")]
                languages.push("javascript");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
            }
            #[cfg(feature = "tree-sitter-rust")]
            Self::Rust => {
                languages.push("rust");
            }
            #[cfg(feature = "tree-sitter-javascript")]
            Self::JavaScript => {
                #[cfg(feature = "tree-sitter-jsdoc")]
                languages.push("jsdoc");
                languages.push("json");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-sql")]
                languages.push("sql");
                #[cfg(feature = "tree-sitter-typescript")]
                languages.push("typescript");
                languages.push("javascript");
                #[cfg(feature = "tree-sitter-tsx")]
                languages.push("tsx");
                #[cfg(feature = "tree-sitter-yaml")]
                languages.push("yaml");
                #[cfg(feature = "tree-sitter-graphql")]
                languages.push("graphql");
            }
            #[cfg(feature = "tree-sitter-typescript")]
            Self::TypeScript => {
                #[cfg(feature = "tree-sitter-jsdoc")]
                languages.push("jsdoc");
                languages.push("json");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-sql")]
                languages.push("sql");
                languages.push("typescript");
                #[cfg(feature = "tree-sitter-javascript")]
                languages.push("javascript");
                #[cfg(feature = "tree-sitter-tsx")]
                languages.push("tsx");
                #[cfg(feature = "tree-sitter-yaml")]
                languages.push("yaml");
                #[cfg(feature = "tree-sitter-graphql")]
                languages.push("graphql");
            }
            #[cfg(feature = "tree-sitter-astro")]
            Self::Astro => {
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
                #[cfg(feature = "tree-sitter-javascript")]
                languages.push("javascript");
                #[cfg(feature = "tree-sitter-typescript")]
                languages.push("typescript");
            }
            #[cfg(feature = "tree-sitter-php")]
            Self::Php => {
                languages.push("php");
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
                #[cfg(feature = "tree-sitter-javascript")]
                languages.push("javascript");
                languages.push("json");
                #[cfg(feature = "tree-sitter-jsdoc")]
                languages.push("jsdoc");
                #[cfg(feature = "tree-sitter-graphql")]
                languages.push("graphql");
            }
            #[cfg(feature = "tree-sitter-svelte")]
            Self::Svelte => {
                languages.push("svelte");
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-css")]
                languages.push("css");
                #[cfg(feature = "tree-sitter-typescript")]
                languages.push("typescript");
            }
            _ => {}
        }

        languages.into_iter().map(SharedString::from).collect()
    }

    /// Return the language info for the language.
    ///
    /// (language, query, injection, locals)
    pub(super) fn config(&self) -> GrammarConfig {
        let (language, query, injection, locals) = match self {
            Self::Plain => return GrammarConfig::plain(self.name()),
            #[cfg(feature = "tree-sitter-dart")]
            Self::Dart => (
                tree_sitter_dart_orchard::LANGUAGE.into(),
                tree_sitter_dart_orchard::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-dockerfile")]
            Self::Dockerfile => (
                tree_sitter_containerfile::LANGUAGE.into(),
                tree_sitter_containerfile::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-fish")]
            Self::Fish => (
                tree_sitter_fish::language(),
                tree_sitter_fish::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-hcl")]
            Self::Hcl => (
                tree_sitter_hcl::LANGUAGE.into(),
                include_str!("languages/hcl/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-ini")]
            Self::Ini => (
                tree_sitter_ini::LANGUAGE.into(),
                tree_sitter_ini::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-json5")]
            Self::Json5 => (
                tree_sitter_json5_orchard::LANGUAGE.into(),
                tree_sitter_json5_orchard::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-less")]
            Self::Less => (
                tree_sitter_less::language(),
                tree_sitter_less::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-nix")]
            Self::Nix => (
                tree_sitter_nix::LANGUAGE.into(),
                tree_sitter_nix::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-powershell")]
            Self::PowerShell => (
                tree_sitter_powershell::LANGUAGE.into(),
                tree_sitter_powershell::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-scss")]
            Self::Scss => (
                tree_sitter_scss::language(),
                tree_sitter_scss::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-vue")]
            Self::Vue => (
                tree_sitter_vue_next::LANGUAGE.into(),
                include_str!("languages/vue/highlights.scm"),
                include_str!("languages/vue/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-xml")]
            Self::Xml => (
                tree_sitter_xml::LANGUAGE_XML.into(),
                tree_sitter_xml::XML_HIGHLIGHT_QUERY,
                "",
                "",
            ),

            Self::Json => (
                tree_sitter_json::LANGUAGE.into(),
                include_str!("languages/json/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-markdown")]
            Self::Markdown => (
                tree_sitter_md::LANGUAGE.into(),
                include_str!("languages/markdown/highlights.scm"),
                include_str!("languages/markdown/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-markdown")]
            Self::MarkdownInline => (
                tree_sitter_md::INLINE_LANGUAGE.into(),
                include_str!("languages/markdown_inline/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-toml")]
            Self::Toml => (
                tree_sitter_toml_ng::LANGUAGE.into(),
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-yaml")]
            Self::Yaml => (
                tree_sitter_yaml::LANGUAGE.into(),
                tree_sitter_yaml::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-rust")]
            Self::Rust => (
                tree_sitter_rust::LANGUAGE.into(),
                include_str!("languages/rust/highlights.scm"),
                include_str!("languages/rust/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-go")]
            Self::Go => (
                tree_sitter_go::LANGUAGE.into(),
                include_str!("languages/go/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-c")]
            Self::C => (
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::HIGHLIGHT_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-cpp")]
            Self::Cpp => (
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::HIGHLIGHT_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-javascript")]
            Self::JavaScript => (
                tree_sitter_javascript::LANGUAGE.into(),
                include_str!("languages/javascript/highlights.scm"),
                include_str!("languages/javascript/injections.scm"),
                tree_sitter_javascript::LOCALS_QUERY,
            ),
            #[cfg(feature = "tree-sitter-jsdoc")]
            Self::JsDoc => (
                tree_sitter_jsdoc::LANGUAGE.into(),
                tree_sitter_jsdoc::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-zig")]
            Self::Zig => (
                tree_sitter_zig::LANGUAGE.into(),
                include_str!("languages/zig/highlights.scm"),
                include_str!("languages/zig/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-java")]
            Self::Java => (
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-python")]
            Self::Python => (
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-ruby")]
            Self::Ruby => (
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_ruby::LOCALS_QUERY,
            ),
            #[cfg(feature = "tree-sitter-bash")]
            Self::Bash => (
                tree_sitter_bash::LANGUAGE.into(),
                tree_sitter_bash::HIGHLIGHT_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-html")]
            Self::Html => (
                tree_sitter_html::LANGUAGE.into(),
                include_str!("languages/html/highlights.scm"),
                include_str!("languages/html/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-css")]
            Self::Css => (
                tree_sitter_css::LANGUAGE.into(),
                tree_sitter_css::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-swift")]
            Self::Swift => (
                tree_sitter_swift::LANGUAGE.into(),
                tree_sitter_swift::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-scala")]
            Self::Scala => (
                tree_sitter_scala::LANGUAGE.into(),
                tree_sitter_scala::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_scala::LOCALS_QUERY,
            ),
            #[cfg(feature = "tree-sitter-sql")]
            Self::Sql => (
                tree_sitter_sequel::LANGUAGE.into(),
                tree_sitter_sequel::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-csharp")]
            Self::CSharp => (
                tree_sitter_c_sharp::LANGUAGE.into(),
                tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-graphql")]
            Self::GraphQL => (
                tree_sitter_graphql::LANGUAGE.into(),
                include_str!("languages/graphql/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-proto")]
            Self::Proto => (
                tree_sitter_proto::LANGUAGE.into(),
                include_str!("languages/proto/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-make")]
            Self::Make => (
                tree_sitter_make::LANGUAGE.into(),
                tree_sitter_make::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-cmake")]
            Self::CMake => (
                tree_sitter_cmake::LANGUAGE.into(),
                tree_sitter_cmake::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-typescript")]
            Self::TypeScript => (
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                include_str!("languages/typescript/highlights.scm"),
                include_str!("languages/javascript/injections.scm"),
                tree_sitter_typescript::LOCALS_QUERY,
            ),
            #[cfg(feature = "tree-sitter-tsx")]
            Self::Tsx => (
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                include_str!("languages/typescript/highlights.scm"),
                include_str!("languages/javascript/injections.scm"),
                tree_sitter_typescript::LOCALS_QUERY,
            ),
            #[cfg(feature = "tree-sitter-diff")]
            Self::Diff => (
                tree_sitter_diff::LANGUAGE.into(),
                tree_sitter_diff::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-elixir")]
            Self::Elixir => (
                tree_sitter_elixir::LANGUAGE.into(),
                tree_sitter_elixir::HIGHLIGHTS_QUERY,
                tree_sitter_elixir::INJECTIONS_QUERY,
                "",
            ),
            #[cfg(feature = "tree-sitter-erb")]
            Self::Erb => (
                tree_sitter_embedded_template::LANGUAGE.into(),
                tree_sitter_embedded_template::HIGHLIGHTS_QUERY,
                tree_sitter_embedded_template::INJECTIONS_ERB_QUERY,
                "",
            ),
            #[cfg(feature = "tree-sitter-ejs")]
            Self::Ejs => (
                tree_sitter_embedded_template::LANGUAGE.into(),
                tree_sitter_embedded_template::HIGHLIGHTS_QUERY,
                tree_sitter_embedded_template::INJECTIONS_EJS_QUERY,
                "",
            ),
            #[cfg(feature = "tree-sitter-php")]
            Self::Php => (
                tree_sitter_php::LANGUAGE_PHP.into(),
                tree_sitter_php::HIGHLIGHTS_QUERY,
                include_str!("languages/php/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-astro")]
            Self::Astro => (
                tree_sitter_astro_next::LANGUAGE.into(),
                tree_sitter_astro_next::HIGHLIGHTS_QUERY,
                tree_sitter_astro_next::INJECTIONS_QUERY,
                "",
            ),
            #[cfg(feature = "tree-sitter-kotlin")]
            Self::Kotlin => (
                tree_sitter_kotlin_sg::LANGUAGE.into(),
                include_str!("languages/kotlin/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-lua")]
            Self::Lua => (
                tree_sitter_lua::LANGUAGE.into(),
                include_str!("languages/lua/highlights.scm"),
                tree_sitter_lua::INJECTIONS_QUERY,
                tree_sitter_lua::LOCALS_QUERY,
            ),
            #[cfg(feature = "tree-sitter-svelte")]
            Self::Svelte => (
                tree_sitter_svelte_next::LANGUAGE.into(),
                tree_sitter_svelte_next::HIGHLIGHTS_QUERY,
                tree_sitter_svelte_next::INJECTIONS_QUERY,
                tree_sitter_svelte_next::LOCALS_QUERY,
            ),
        };

        // Extension queries also need their base language's rules; Tree-sitter
        // does not expand editor-specific query inheritance directives.
        // Registering a parser with only the extension query silently leaves base tokens plain.
        let query: Cow<'_, str> = match self {
            #[cfg(feature = "tree-sitter-cpp")]
            Self::Cpp => format!("{}\n{query}", tree_sitter_c::HIGHLIGHT_QUERY).into(),
            #[cfg(feature = "tree-sitter-scss")]
            Self::Scss => format!("{}\n{query}", tree_sitter_css::HIGHLIGHTS_QUERY).into(),
            #[cfg(feature = "tree-sitter-less")]
            Self::Less => format!("{}\n{query}", tree_sitter_css::HIGHLIGHTS_QUERY).into(),
            #[cfg(feature = "tree-sitter-javascript")]
            Self::JavaScript => {
                format!("{query}\n{}", tree_sitter_javascript::JSX_HIGHLIGHT_QUERY).into()
            }
            #[cfg(feature = "tree-sitter-tsx")]
            Self::Tsx => format!("{query}\n{}", tree_sitter_javascript::JSX_HIGHLIGHT_QUERY).into(),
            _ => query.into(),
        };

        GrammarConfig::new(
            self.name(),
            language,
            self.injection_languages(),
            &query,
            injection,
            locals,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_name() {
        assert_eq!(Language::Plain.name(), "text");
        assert_eq!(Language::Json.name(), "json");

        #[cfg(feature = "tree-sitter-markdown")]
        {
            assert_eq!(Language::MarkdownInline.name(), "markdown_inline");
            assert_eq!(Language::Markdown.name(), "markdown");
        }

        #[cfg(feature = "tree-sitter-yaml")]
        assert_eq!(Language::Yaml.name(), "yaml");
        #[cfg(feature = "tree-sitter-rust")]
        assert_eq!(Language::Rust.name(), "rust");
        #[cfg(feature = "tree-sitter-go")]
        assert_eq!(Language::Go.name(), "go");
        #[cfg(feature = "tree-sitter-c")]
        assert_eq!(Language::C.name(), "c");
        #[cfg(feature = "tree-sitter-cpp")]
        assert_eq!(Language::Cpp.name(), "cpp");
        #[cfg(feature = "tree-sitter-sql")]
        assert_eq!(Language::Sql.name(), "sql");
        #[cfg(feature = "tree-sitter-javascript")]
        assert_eq!(Language::JavaScript.name(), "javascript");
        #[cfg(feature = "tree-sitter-zig")]
        assert_eq!(Language::Zig.name(), "zig");
        #[cfg(feature = "tree-sitter-csharp")]
        assert_eq!(Language::CSharp.name(), "csharp");
        #[cfg(feature = "tree-sitter-typescript")]
        assert_eq!(Language::TypeScript.name(), "typescript");
        #[cfg(feature = "tree-sitter-tsx")]
        assert_eq!(Language::Tsx.name(), "tsx");
        #[cfg(feature = "tree-sitter-diff")]
        assert_eq!(Language::Diff.name(), "diff");
        #[cfg(feature = "tree-sitter-elixir")]
        assert_eq!(Language::Elixir.name(), "elixir");
        #[cfg(feature = "tree-sitter-erb")]
        assert_eq!(Language::Erb.name(), "erb");
        #[cfg(feature = "tree-sitter-ejs")]
        assert_eq!(Language::Ejs.name(), "ejs");
    }

    #[test]
    fn test_language_aliases_only_resolve_enabled_features() {
        assert_eq!(Language::from_name("text"), Some(Language::Plain));
        assert_eq!(Language::from_name("jsonc"), Some(Language::Json));
        assert_eq!(Language::from_name("unknown"), None);

        #[cfg(feature = "tree-sitter-rust")]
        assert_eq!(Language::from_name("rs"), Some(Language::Rust));
        #[cfg(not(feature = "tree-sitter-rust"))]
        assert_eq!(Language::from_name("rs"), None);

        #[cfg(feature = "tree-sitter-markdown")]
        assert_eq!(Language::from_name("md"), Some(Language::Markdown));
        #[cfg(not(feature = "tree-sitter-markdown"))]
        assert_eq!(Language::from_name("md"), None);

        #[cfg(feature = "tree-sitter-typescript")]
        assert_eq!(Language::from_name("ts"), Some(Language::TypeScript));
        #[cfg(not(feature = "tree-sitter-typescript"))]
        assert_eq!(Language::from_name("ts"), None);

        assert_eq!(Language::from_str("unknown"), Language::Plain);
    }

    #[test]
    fn test_every_language_query_compiles() {
        let failures: Vec<String> = Language::all()
            .filter_map(|language| {
                let config = language.config();
                let grammar = config.language?;
                let source = format!(
                    "{}{}{}",
                    config.injections, config.locals, config.highlights
                );
                let err = tree_sitter::Query::new(&grammar, &source).err()?;
                Some(format!("{}: {err}", language.name()))
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
