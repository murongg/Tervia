use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

use gpui_kit::{component::Icon, *};

const ICON_SIZE: u32 = 16;

pub(super) fn file_icon(path: &Path, light: bool, muted: Option<Hsla>) -> Div {
    let name = resolve(path);
    let image = image(name, light);
    div()
        .debug_selector(move || format!("file-icon-{name}"))
        .size(px(ICON_SIZE as f32))
        .flex_shrink_0()
        .child(match muted {
            // A mask keeps ignored icons at the same readable neutral ink as folder icons.
            Some(color) => Icon::default()
                .data(&image.bytes)
                .size(px(ICON_SIZE as f32))
                .text_color(color)
                .into_any_element(),
            None => img(image).size_full().into_any_element(),
        })
}

fn resolve(path: &Path) -> &'static str {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return "document";
    };
    let name = name.to_ascii_lowercase();
    // Recognizable filenames must win over generic suffixes (package.json is not plain JSON).
    match name.as_str() {
        "package.json" | "package-lock.json" | "npm-shrinkwrap.json" => return "nodejs",
        "cargo.toml" | "cargo.lock" | "rust-toolchain" | "rust-toolchain.toml" => return "rust",
        "go.mod" | "go.sum" | "go.work" | "go.work.sum" => return "go-mod",
        "pyproject.toml" | "requirements.txt" | "pipfile" | ".python-version" => {
            return "python-misc";
        }
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" | ".gitkeep" => return "git",
        "pnpm-lock.yaml" | "pnpm-workspace.yaml" => return "pnpm",
        "yarn.lock" | ".yarnrc" | ".yarnrc.yml" => return "yarn",
        "makefile" | "gnumakefile" => return "makefile",
        "readme" | "readme.md" | "readme.markdown" | "readme.txt" | "readme.rst" => return "readme",
        "license" | "license.md" | "license.txt" | "licence" | "licence.md" | "licence.txt"
        | "copying" => return "license",
        ".env" => return "tune",
        ".bashrc" | ".bash_profile" | ".profile" | ".zshrc" | ".zprofile" | ".zshenv" => {
            return "console";
        }
        ".editorconfig" | ".npmrc" => return "settings",
        ".eslintrc" | ".eslintignore" => return "eslint",
        ".prettierrc" | ".prettierignore" => return "prettier",
        _ => {}
    }
    if name.starts_with(".env.") {
        return "tune";
    }
    if name == "dockerfile"
        || name == "containerfile"
        || name.starts_with("dockerfile.")
        || name.starts_with("containerfile.")
        || name.ends_with(".dockerfile")
        || ((name.starts_with("docker-compose.") || name.starts_with("compose."))
            && (name.ends_with(".yml") || name.ends_with(".yaml")))
    {
        return "docker";
    }
    if name.starts_with("tsconfig.") && name.ends_with(".json") {
        return "tsconfig";
    }
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("");
    if matches!(
        extension,
        "js" | "cjs" | "mjs" | "ts" | "cts" | "mts" | "json" | "yml" | "yaml" | "toml"
    ) {
        if name.starts_with(".eslintrc.") || name.starts_with("eslint.config.") {
            return "eslint";
        }
        if name.starts_with(".prettierrc.") || name.starts_with("prettier.config.") {
            return "prettier";
        }
    }
    // Check compound suffixes before the final extension so declarations remain distinct.
    if [".d.ts", ".d.mts", ".d.cts"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        return "typescript-def";
    }
    match extension {
        "rs" | "ron" => "rust",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "jsx" => "react",
        "tsx" => "react_ts",
        "py" | "pyi" | "pyw" => "python",
        "go" => "go",
        "c" => "c",
        "h" => "h",
        "cc" | "cpp" | "cxx" => "cpp",
        "hh" | "hpp" | "hxx" => "hpp",
        "cs" | "csx" => "csharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "php" | "phtml" => "php",
        "dart" => "dart",
        "vue" => "vue",
        "svelte" => "svelte",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "sass",
        "less" => "less",
        "json" | "jsonc" | "json5" | "jsonl" => "json",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "ini" | "cfg" | "conf" | "properties" => "settings",
        "md" | "markdown" | "mdx" | "rst" => "markdown",
        "sh" | "bash" | "zsh" | "fish" | "bat" | "cmd" => "console",
        "ps1" | "psm1" | "psd1" => "powershell",
        "sql" | "db" | "sqlite" | "sqlite3" => "database",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "ico" | "avif"
        | "heic" => "image",
        "svg" => "svg",
        "pdf" => "pdf",
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => "zip",
        "csv" | "tsv" | "xls" | "xlsx" | "ods" => "table",
        _ => "document",
    }
}

fn image(name: &str, light: bool) -> Arc<Image> {
    macro_rules! asset {
        ($name:literal) => {{
            // Share encoded images across rows and frames; GPUI caches decoded SVGs by image ID.
            static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
            IMAGE
                .get_or_init(|| {
                    // GPUI decodes intrinsic SVG dimensions before applying layout. Bound the
                    // viewport while preserving upstream coordinates (JSON's viewBox is 960x960).
                    let svg = include_str!(concat!("../../assets/material/", $name, ".svg"))
                        .replacen(
                            "<svg ",
                            &format!("<svg width=\"{ICON_SIZE}\" height=\"{ICON_SIZE}\" "),
                            1,
                        );
                    Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()))
                })
                .clone()
        }};
    }
    match (name, light) {
        ("toml", true) => asset!("toml_light"),
        ("pnpm", true) => asset!("pnpm_light"),
        ("rust", _) => asset!("rust"),
        ("javascript", _) => asset!("javascript"),
        ("typescript", _) => asset!("typescript"),
        ("react", _) => asset!("react"),
        ("react_ts", _) => asset!("react_ts"),
        ("typescript-def", _) => asset!("typescript-def"),
        ("python", _) => asset!("python"),
        ("go", _) => asset!("go"),
        ("c", _) => asset!("c"),
        ("h", _) => asset!("h"),
        ("cpp", _) => asset!("cpp"),
        ("hpp", _) => asset!("hpp"),
        ("csharp", _) => asset!("csharp"),
        ("java", _) => asset!("java"),
        ("kotlin", _) => asset!("kotlin"),
        ("swift", _) => asset!("swift"),
        ("ruby", _) => asset!("ruby"),
        ("php", _) => asset!("php"),
        ("dart", _) => asset!("dart"),
        ("vue", _) => asset!("vue"),
        ("svelte", _) => asset!("svelte"),
        ("html", _) => asset!("html"),
        ("css", _) => asset!("css"),
        ("sass", _) => asset!("sass"),
        ("less", _) => asset!("less"),
        ("json", _) => asset!("json"),
        ("yaml", _) => asset!("yaml"),
        ("toml", _) => asset!("toml"),
        ("settings", _) => asset!("settings"),
        ("markdown", _) => asset!("markdown"),
        ("console", _) => asset!("console"),
        ("powershell", _) => asset!("powershell"),
        ("database", _) => asset!("database"),
        ("image", _) => asset!("image"),
        ("svg", _) => asset!("svg"),
        ("pdf", _) => asset!("pdf"),
        ("zip", _) => asset!("zip"),
        ("table", _) => asset!("table"),
        ("nodejs", _) => asset!("nodejs"),
        ("go-mod", _) => asset!("go-mod"),
        ("python-misc", _) => asset!("python-misc"),
        ("git", _) => asset!("git"),
        ("tune", _) => asset!("tune"),
        ("docker", _) => asset!("docker"),
        ("makefile", _) => asset!("makefile"),
        ("readme", _) => asset!("readme"),
        ("license", _) => asset!("license"),
        ("tsconfig", _) => asset!("tsconfig"),
        ("eslint", _) => asset!("eslint"),
        ("prettier", _) => asset!("prettier"),
        ("pnpm", _) => asset!("pnpm"),
        ("yarn", _) => asset!("yarn"),
        _ => asset!("document"),
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::path::Path;

    #[test]
    fn common_file_types_have_material_icons() {
        for (file, icon) in [
            ("mock.rs", "rust"),
            ("mock.js", "javascript"),
            ("mock.mjs", "javascript"),
            ("mock.ts", "typescript"),
            ("mock.jsx", "react"),
            ("mock.tsx", "react_ts"),
            ("mock.py", "python"),
            ("mock.go", "go"),
            ("mock.c", "c"),
            ("mock.h", "h"),
            ("mock.cpp", "cpp"),
            ("mock.hpp", "hpp"),
            ("mock.cs", "csharp"),
            ("mock.java", "java"),
            ("mock.kt", "kotlin"),
            ("mock.swift", "swift"),
            ("mock.rb", "ruby"),
            ("mock.php", "php"),
            ("mock.dart", "dart"),
            ("mock.vue", "vue"),
            ("mock.svelte", "svelte"),
            ("mock.html", "html"),
            ("mock.css", "css"),
            ("mock.scss", "sass"),
            ("mock.less", "less"),
            ("mock.jsonc", "json"),
            ("mock.yaml", "yaml"),
            ("mock.toml", "toml"),
            ("mock.ini", "settings"),
            ("mock.md", "markdown"),
            ("mock.txt", "document"),
            ("mock.sh", "console"),
            ("mock.ps1", "powershell"),
            ("mock.sql", "database"),
            ("mock.png", "image"),
            ("mock.svg", "svg"),
            ("mock.pdf", "pdf"),
            ("mock.zip", "zip"),
            ("mock.csv", "table"),
        ] {
            assert_eq!(resolve(Path::new(file)), icon, "{file}");
        }
    }

    #[test]
    fn filenames_take_precedence_over_generic_extensions() {
        for (file, icon) in [
            ("mock/package.json", "nodejs"),
            ("mock/package-lock.json", "nodejs"),
            ("mock/Cargo.toml", "rust"),
            ("mock/Cargo.lock", "rust"),
            ("mock/go.mod", "go-mod"),
            ("mock/go.sum", "go-mod"),
            ("mock/pyproject.toml", "python-misc"),
            ("mock/requirements.txt", "python-misc"),
            ("mock/.gitignore", "git"),
            ("mock/.gitattributes", "git"),
            ("mock/.env", "tune"),
            ("mock/.env.local", "tune"),
            ("mock/Dockerfile", "docker"),
            ("mock/Dockerfile.dev", "docker"),
            ("mock/docker-compose.yml", "docker"),
            ("mock/compose.prod.yaml", "docker"),
            ("mock/Makefile", "makefile"),
            ("mock/README.md", "readme"),
            ("mock/LICENSE", "license"),
            ("mock/tsconfig.app.json", "tsconfig"),
            ("mock/.eslintrc.json", "eslint"),
            ("mock/eslint.config.mjs", "eslint"),
            ("mock/.prettierrc", "prettier"),
            ("mock/prettier.config.js", "prettier"),
            ("mock/pnpm-lock.yaml", "pnpm"),
            ("mock/yarn.lock", "yarn"),
            ("mock/.zshrc", "console"),
        ] {
            assert_eq!(resolve(Path::new(file)), icon, "{file}");
        }
    }

    #[test]
    fn compound_extensions_and_case_are_consistent() {
        for (file, icon) in [
            ("mock.d.ts", "typescript-def"),
            ("mock.d.mts", "typescript-def"),
            ("mock.d.cts", "typescript-def"),
            ("MOCK.TSX", "react_ts"),
            ("PACKAGE.JSON", "nodejs"),
            ("README.MD", "readme"),
            ("PHOTO.JPEG", "image"),
        ] {
            assert_eq!(resolve(Path::new(file)), icon, "{file}");
        }
    }

    #[test]
    fn unknown_files_and_near_matches_keep_the_default_icon() {
        for file in [
            "mock.unknown",
            "mock",
            ".environment",
            "Dockerfileish",
            "",
            "mock/package.json.unknown",
        ] {
            assert_eq!(resolve(Path::new(file)), "document", "{file}");
        }
        assert_eq!(resolve(Path::new("mock/README.rs")), "rust");
    }

    #[test]
    fn upstream_light_variants_are_used_without_changing_file_associations() {
        for name in ["toml", "pnpm"] {
            assert_ne!(image(name, false).bytes, image(name, true).bytes, "{name}");
        }
        assert_eq!(image("react_ts", false), image("react_ts", true));
    }

    #[test]
    fn file_icons_rasterize_at_display_size_instead_of_source_viewbox() {
        let renderer = gpui_test::SvgRenderer::new(Arc::new(()));
        for name in ["json", "svelte", "rust"] {
            for light in [false, true] {
                let decoded = image(name, light).to_image_data(renderer.clone()).unwrap();
                let size = decoded.size(0);
                // GPUI smooths the 16-point icon at 2x, independent of the source coordinates.
                assert_eq!(
                    (size.width.0, size.height.0),
                    (32, 32),
                    "{name}, light={light}"
                );
                let (pixels, _) = decoded.as_bytes(0).unwrap().as_chunks::<4>();
                assert!(
                    pixels.iter().any(|pixel| pixel[3] > 0),
                    "{name} must retain visible artwork"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_filenames_fall_back_without_panicking() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        assert_eq!(
            resolve(Path::new(OsStr::from_bytes(b"mock-\xff.rs"))),
            "document"
        );
    }
}
