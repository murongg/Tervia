//! Portable, complete color snapshots. Rendering and filesystem IO live at the edges.
use crate::config::document::set;
use std::collections::BTreeMap;
use toml_edit::{DocumentMut, Item, Table, Value};

mod draft;
mod store;
pub use draft::Draft;
pub use store::{Catalog, Document, Store};

pub const COLOR_KEYS: &[&str] = &[
    "ui.background",
    "ui.on_accent",
    "ui.sidebar",
    "ui.toolbar",
    "ui.raised",
    "ui.selected",
    "ui.border",
    "ui.text",
    "ui.secondary",
    "ui.muted",
    "ui.ignored",
    "ui.accent",
    "ui.green",
    "ui.red",
    "ui.workspace_start",
    "ui.workspace_end",
    "ui.workspace_accent",
    "ui.workspace_selected",
    "ui.workspace_border",
    "terminal.background",
    "terminal.foreground",
    "terminal.cursor",
    "terminal.ansi0",
    "terminal.ansi1",
    "terminal.ansi2",
    "terminal.ansi3",
    "terminal.ansi4",
    "terminal.ansi5",
    "terminal.ansi6",
    "terminal.ansi7",
    "terminal.ansi8",
    "terminal.ansi9",
    "terminal.ansi10",
    "terminal.ansi11",
    "terminal.ansi12",
    "terminal.ansi13",
    "terminal.ansi14",
    "terminal.ansi15",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(u32);

impl Color {
    pub fn parse(source: &str) -> Result<Self, String> {
        let digits = source.strip_prefix('#').unwrap_or("");
        if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Use #RRGGBB or #RRGGBBAA".into());
        }
        let value = u32::from_str_radix(digits, 16).map_err(|error| error.to_string())?;
        Ok(Self(if digits.len() == 6 {
            (value << 8) | 255
        } else {
            value
        }))
    }

    pub fn hex(self) -> String {
        if self.0 & 255 == 255 {
            format!("#{:06x}", self.0 >> 8)
        } else {
            format!("#{:08x}", self.0)
        }
    }

    pub fn rgb(self) -> u32 {
        self.0 >> 8
    }
    pub fn alpha(self) -> f32 {
        (self.0 & 255) as f32 / 255.0
    }

    pub fn over(self, background: Self) -> Self {
        let a = self.alpha();
        let b = background.alpha() * (1.0 - a);
        let alpha = a + b;
        if alpha == 0.0 {
            return Self(0);
        }
        let mut rgb = 0;
        for shift in [16, 8, 0] {
            let front = ((self.rgb() >> shift) & 255) as f32;
            let back = ((background.rgb() >> shift) & 255) as f32;
            rgb |= (((front * a + back * b) / alpha).round() as u32) << shift;
        }
        Self((rgb << 8) | (alpha * 255.0).round() as u32)
    }

    pub fn contrast(self, background: Self) -> f32 {
        let luminance = |color: Self| {
            let rgb = color.rgb();
            [16, 8, 0]
                .into_iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(shift, weight)| {
                    let value = ((rgb >> shift) & 255) as f32 / 255.0;
                    weight
                        * if value <= 0.04045 {
                            value / 12.92
                        } else {
                            ((value + 0.055) / 1.055).powf(2.4)
                        }
                })
                .sum::<f32>()
        };
        let a = luminance(self.over(background));
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    id: String,
    name: String,
    light: bool,
    colors: BTreeMap<String, Color>,
    syntax: serde_json::Value,
}

pub fn valid_id(id: &str) -> bool {
    id.strip_prefix("user-").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 80
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

impl Definition {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        light: bool,
        colors: BTreeMap<String, Color>,
        mut syntax: serde_json::Value,
    ) -> Result<Self, String> {
        if let Some(styles) = syntax.as_object_mut() {
            styles.retain(|_, style| !style.is_null());
            for style in styles.values_mut() {
                if let Some(style) = style.as_object_mut() {
                    style.retain(|_, value| !value.is_null());
                }
            }
        }
        let mut data = Self {
            id: id.into(),
            name: String::new(),
            light,
            colors,
            syntax,
        };
        if !valid_id(&data.id) {
            return Err("Invalid custom theme identifier".into());
        }
        data.set_name(name)?;
        for key in COLOR_KEYS {
            if !data.colors.contains_key(*key) {
                return Err(format!("Missing theme color: {key}"));
            }
        }
        let styles = data.syntax.as_object().ok_or("syntax must be a table")?;
        for (key, style) in styles {
            if style.is_null() {
                continue;
            }
            let style = style
                .as_object()
                .ok_or_else(|| format!("Invalid syntax style: {key}"))?;
            if let Some(color) = style.get("color").filter(|value| !value.is_null()) {
                Color::parse(color.as_str().ok_or("Syntax color must be a HEX string")?)?;
            }
            if let Some(font) = style.get("font_style").filter(|value| !value.is_null())
                && !matches!(font.as_str(), Some("normal" | "italic" | "underline"))
            {
                return Err(format!("Invalid syntax font style: {key}"));
            }
            if let Some(weight) = style.get("font_weight").filter(|value| !value.is_null())
                && !matches!(
                    weight.as_u64(),
                    Some(100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900)
                )
            {
                return Err(format!("Invalid syntax font weight: {key}"));
            }
        }
        Ok(data)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn is_light(&self) -> bool {
        self.light
    }
    pub fn syntax(&self) -> &serde_json::Value {
        &self.syntax
    }
    pub fn set_name(&mut self, name: impl Into<String>) -> Result<(), String> {
        let name = name.into();
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err(
                "Theme name must contain 1–80 characters without control characters".into(),
            );
        }
        self.name = name.into();
        Ok(())
    }

    pub fn color(&self, key: &str) -> Result<Color, String> {
        if let Some(capture) = key.strip_prefix("syntax.") {
            return self
                .syntax
                .get(capture)
                .and_then(|style| style.get("color"))
                .and_then(serde_json::Value::as_str)
                .map(Color::parse)
                .unwrap_or_else(|| self.color("ui.text"));
        }
        self.colors
            .get(key)
            .copied()
            .ok_or_else(|| format!("Unknown theme color: {key}"))
    }

    pub fn set_color(&mut self, key: &str, color: Color) -> Result<(), String> {
        if let Some(capture) = key.strip_prefix("syntax.") {
            let syntax = self.syntax.as_object_mut().expect("Validated syntax map");
            let style = syntax
                .entry(capture)
                .or_insert_with(|| serde_json::json!({}));
            if style.is_null() {
                *style = serde_json::json!({});
            }
            style["color"] = color.hex().into();
        } else {
            *self
                .colors
                .get_mut(key)
                .ok_or_else(|| format!("Unknown theme color: {key}"))? = color;
        }
        Ok(())
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let document = source
            .parse::<DocumentMut>()
            .map_err(|error| error.to_string())?;
        if document.get("version").and_then(Item::as_integer) != Some(1) {
            return Err("Unsupported theme version".into());
        }
        let string = |key: &str| {
            document
                .get(key)
                .and_then(Item::as_str)
                .ok_or_else(|| format!("Missing theme field: {key}"))
        };
        let light = match string("appearance")? {
            "dark" => false,
            "light" => true,
            _ => return Err("appearance must be dark or light".into()),
        };
        let mut colors = BTreeMap::new();
        for key in COLOR_KEYS {
            let (section, name) = key.split_once('.').expect("Color keys have sections");
            let source = document
                .get(section)
                .and_then(|table| table.get(name))
                .and_then(Item::as_str)
                .ok_or_else(|| format!("Missing theme color: {key}"))?;
            colors.insert((*key).into(), Color::parse(source)?);
        }
        let mut syntax = serde_json::Map::new();
        let styles = document
            .get("syntax")
            .and_then(Item::as_table_like)
            .ok_or("Missing syntax table")?;
        for (key, item) in styles.iter() {
            let table = item
                .as_table_like()
                .ok_or_else(|| format!("Invalid syntax style: {key}"))?;
            let mut style = serde_json::Map::new();
            for (field, item) in table.iter() {
                let value = if let Some(value) = item.as_str() {
                    serde_json::Value::from(value)
                } else if let Some(value) = item.as_integer() {
                    serde_json::Value::from(value)
                } else {
                    return Err(format!("Invalid syntax property: {key}.{field}"));
                };
                style.insert(field.into(), value);
            }
            syntax.insert(key.into(), style.into());
        }
        Self::new(string("id")?, string("name")?, light, colors, syntax.into())
    }

    pub fn to_toml(&self) -> Result<String, String> {
        self.update_toml("")
    }

    fn update_toml(&self, source: &str) -> Result<String, String> {
        let mut document = source
            .parse::<DocumentMut>()
            .map_err(|error| error.to_string())?;
        set(&mut document["version"], Value::from(1));
        set(&mut document["id"], Value::from(self.id.as_str()));
        set(&mut document["name"], Value::from(self.name.as_str()));
        set(
            &mut document["appearance"],
            Value::from(if self.light { "light" } else { "dark" }),
        );
        for (key, color) in &self.colors {
            let (section, name) = key.split_once('.').expect("Validated color keys");
            if document.get(section).is_none() {
                document[section] = Item::Table(Table::new());
            }
            set(&mut document[section][name], Value::from(color.hex()));
        }
        if document.get("syntax").is_none() {
            document["syntax"] = Item::Table(Table::new());
        }
        for (capture, style) in self.syntax.as_object().expect("Validated syntax map") {
            let Some(style) = style.as_object() else {
                continue;
            };
            if document["syntax"].get(capture).is_none() {
                document["syntax"][capture] = Item::Table(Table::new());
            }
            for (key, value) in style {
                if let Some(value) = value.as_str() {
                    if document["syntax"][capture].get(key).and_then(Item::as_str) != Some(value) {
                        set(&mut document["syntax"][capture][key], Value::from(value));
                    }
                } else if let Some(value) = value.as_i64() {
                    set(&mut document["syntax"][capture][key], Value::from(value));
                }
            }
        }
        Ok(document.to_string())
    }
}
