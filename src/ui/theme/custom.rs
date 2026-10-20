use super::*;
use gpui_kit::component::highlighter::SyntaxColors;
use tervia::themes::{Color, Definition};

pub(in crate::ui) fn hex(color: Hsla) -> String {
    let color = color.to_rgb();
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u32;
    let rgb = (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b);
    if color.a < 1.0 {
        format!("#{rgb:06x}{:02x}", channel(color.a))
    } else {
        format!("#{rgb:06x}")
    }
}

pub(in crate::ui) fn color(data: &Definition, key: &str) -> Hsla {
    let color = data
        .color(key)
        .expect("Theme documents validate required colors");
    rgb(color.rgb()).opacity(color.alpha()).into()
}

pub(super) fn syntax(theme: &ColorTheme) -> SyntaxColors {
    if let Some(data) = theme.custom() {
        return serde_json::from_value(data.syntax().clone()).expect("Validated syntax styles");
    }
    if let Some(preset) = presets::get(theme) {
        return preset.syntax.clone();
    }
    let base = if theme.is_light() {
        gpui_kit::component::highlighter::HighlightTheme::default_light()
    } else {
        gpui_kit::component::highlighter::HighlightTheme::default_dark()
    };
    base.style.syntax.clone()
}

pub(in crate::ui) fn palette(data: &Definition) -> Palette {
    Palette {
        background: color(data, "ui.background"),
        on_accent: color(data, "ui.on_accent"),
        sidebar: color(data, "ui.sidebar"),
        toolbar: color(data, "ui.toolbar"),
        raised: color(data, "ui.raised"),
        selected: color(data, "ui.selected"),
        border: color(data, "ui.border"),
        text: color(data, "ui.text"),
        secondary: color(data, "ui.secondary"),
        muted: color(data, "ui.muted"),
        ignored: color(data, "ui.ignored"),
        accent: color(data, "ui.accent"),
        green: color(data, "ui.green"),
        red: color(data, "ui.red"),
    }
}

pub(in crate::ui) fn terminal_color(theme: &ColorTheme, key: &str) -> Hsla {
    if let Some(data) = theme.custom() {
        return color(data, key);
    }
    let p = Palette::new(theme);
    match key {
        "terminal.background" => p.background.opacity(0.0),
        _ => p.text,
    }
}

pub(in crate::ui) fn terminal_entry(theme: &ColorTheme, index: usize) -> Option<Color> {
    let data = theme.custom()?;
    let key = match index {
        0..=15 => format!("terminal.ansi{index}"),
        256 => "terminal.foreground".into(),
        257 => "terminal.background".into(),
        258 => "terminal.cursor".into(),
        _ => return None,
    };
    data.color(&key).ok()
}

pub(in crate::ui) fn snapshot(theme: &ColorTheme) -> Definition {
    if let Some(data) = theme.custom() {
        return data.clone();
    }
    let p = Palette::new(theme);
    let workbench = Palette::workbench(theme);
    let workspace = workspace_colors(theme);
    let ui = [
        ("background", p.background),
        ("on_accent", p.on_accent),
        ("sidebar", p.sidebar),
        ("toolbar", p.toolbar),
        ("raised", p.raised),
        ("selected", p.selected),
        ("border", p.border),
        ("text", p.text),
        ("secondary", p.secondary),
        ("muted", p.muted),
        ("ignored", p.ignored),
        ("accent", p.accent),
        ("green", p.green),
        ("red", p.red),
        ("workspace_start", rgb(workspace[0]).into()),
        ("workspace_end", rgb(workspace[1]).into()),
        ("workspace_accent", workbench.accent),
        ("workspace_selected", workbench.selected),
        ("workspace_border", workbench.border),
    ];
    let mut colors = std::collections::BTreeMap::new();
    for (key, value) in ui {
        colors.insert(
            format!("ui.{key}"),
            Color::parse(&hex(value)).expect("Rendered color"),
        );
    }
    for key in ["background", "foreground", "cursor"] {
        colors.insert(
            format!("terminal.{key}"),
            Color::parse(&hex(terminal_color(theme, &format!("terminal.{key}"))))
                .expect("Rendered color"),
        );
    }
    for (index, value) in terminal_ansi(theme).into_iter().enumerate() {
        colors.insert(
            format!("terminal.ansi{index}"),
            Color::parse(&format!("#{value:06x}")).expect("Bundled ANSI color"),
        );
    }
    let original_syntax = syntax(theme);
    let mut syntax = serde_json::to_value(&original_syntax).expect("Serializable syntax");
    // Preserve every style, including emphasis and weight, rather than reconstructing six colors.
    for (capture, style) in syntax.as_object_mut().expect("Syntax object") {
        if style.get("color").is_none_or(serde_json::Value::is_null) {
            continue;
        }
        let capture = if capture == "comment_doc" {
            "comment.doc"
        } else {
            capture
        };
        if let Some(color) = original_syntax.style(capture).and_then(|style| style.color) {
            style["color"] = hex(color).into();
        }
    }
    Definition::new(
        "user-draft",
        theme.label(tervia::locale::Language::English),
        theme.is_light(),
        colors,
        syntax,
    )
    .expect("Complete bundled theme snapshot")
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn copying_every_preset_preserves_actual_palette_syntax_and_ansi() {
        for base in ColorTheme::ALL {
            let snapshot = snapshot(&base);
            let snapshot = Definition::parse(&snapshot.to_toml().unwrap()).unwrap();
            let copied = ColorTheme::Custom(std::sync::Arc::new(snapshot));
            let before = Palette::new(&base);
            let after = Palette::new(&copied);
            assert_eq!(hex(before.text), hex(after.text), "{}", base.id());
            assert_eq!(hex(before.accent), hex(after.accent), "{}", base.id());
            let before = Palette::workbench(&base);
            let after = Palette::workbench(&copied);
            assert_eq!(hex(before.accent), hex(after.accent), "{}", base.id());
            assert_eq!(hex(before.selected), hex(after.selected), "{}", base.id());
            assert_eq!(workspace_colors(&base), workspace_colors(&copied));
            assert_eq!(terminal_ansi(&base), terminal_ansi(&copied));
            assert_eq!(syntax(&base), syntax(&copied), "{}", base.id());
        }
    }

    #[gpui_test::test]
    fn custom_edits_reach_all_rendering_surfaces_without_global_registration(
        cx: &mut gpui_test::TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let mut data = snapshot(&ColorTheme::Dark);
        for (key, value) in [
            ("ui.text", "#fafafa"),
            ("ui.workspace_start", "#12345680"),
            ("syntax.keyword", "#abcdef"),
            ("terminal.ansi1", "#987654"),
        ] {
            data.set_color(key, tervia::themes::Color::parse(value).unwrap())
                .unwrap();
        }
        let custom = ColorTheme::Custom(std::sync::Arc::new(data));
        assert_eq!(hex(Palette::new(&custom).text), "#fafafa");
        assert_eq!(workspace_colors(&custom)[0], 0x123456);
        assert_eq!(
            workspace_background(&custom),
            linear_gradient(
                135.0,
                linear_color_stop(rgb(0x123456).opacity(128.0 / 255.0), 0.0),
                linear_color_stop(rgb(0x20212c), 1.0)
            )
        );
        assert_eq!(terminal_ansi(&custom)[1], 0x987654);
        cx.update(|cx| {
            apply_palette(&custom, cx);
            use gpui_kit::component::ActiveTheme;
            assert_eq!(
                hex(cx
                    .theme()
                    .highlight_theme
                    .style
                    .syntax
                    .style("keyword")
                    .unwrap()
                    .color
                    .unwrap()),
                "#abcdef"
            );
        });
    }
}
