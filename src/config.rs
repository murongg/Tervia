use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

use crate::{
    locale::Language,
    preferences::Preferences,
    updates::{MAX_INTERVAL_MINUTES, MIN_INTERVAL_MINUTES, Settings},
};

/// The user-owned document and the exact contents last loaded or saved by this instance.
pub struct Config {
    path: PathBuf,
    source: Option<String>,
}

impl Config {
    pub fn new(path: PathBuf) -> Self {
        Self { path, source: None }
    }

    pub fn discover() -> Result<Self, String> {
        if let Some(path) = std::env::var_os("TERVIA_CONFIG_FILE") {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err("TERVIA_CONFIG_FILE must be an absolute file path".into());
            }
            return Ok(Self::new(path));
        }
        dirs::config_dir()
            .map(|root| Self::new(root.join("tervia/config.toml")))
            .ok_or_else(|| "Cannot find the user configuration directory".into())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&mut self) -> Result<Preferences, String> {
        self.source = None;
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.create().map_err(|error| self.error(error))?;
                fs::read_to_string(&self.path).map_err(|error| self.error(error))?
            }
            Err(error) => return Err(self.error(error)),
        };
        let document = parse(&source).map_err(|error| self.error(error))?;
        let preferences = read(&document).map_err(|error| self.error(error))?;
        self.source = Some(source);
        Ok(preferences)
    }

    pub fn save(&mut self, preferences: &Preferences) -> Result<(), String> {
        let source = self.source.as_ref().ok_or_else(|| {
            self.error("Configuration was not loaded; fix the file and restart Tervia")
        })?;
        let mut document = parse(source).map_err(|error| self.error(error))?;
        write(&mut document, preferences);
        read(&document).map_err(|error| self.error(error))?;
        let updated = document.to_string();

        // Never replace edits made by a text editor or another Tervia instance.
        let current = fs::read_to_string(&self.path).map_err(|error| self.error(error))?;
        if &current != source {
            return Err(self.error(
                "Configuration changed on disk; restart Tervia to load it before saving settings",
            ));
        }
        if updated == *source {
            return Ok(());
        }
        // Resolve dotfile symlinks before replacing the target, keeping the link intact.
        let target = fs::canonicalize(&self.path).map_err(|error| self.error(error))?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent(&target)).map_err(|error| self.error(error))?;
        let permissions = fs::metadata(&target)
            .map_err(|error| self.error(error))?
            .permissions();
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| self.error(error))?;
        temporary
            .write_all(updated.as_bytes())
            .map_err(|error| self.error(error))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| self.error(error))?;
        // Recheck after preparing the replacement, so slow writes do not widen the conflict window.
        if fs::read_to_string(&self.path).map_err(|error| self.error(error))? != current {
            return Err(
                self.error("Configuration changed on disk; restart Tervia before saving settings")
            );
        }
        temporary
            .persist(&target)
            .map_err(|error| self.error(error))?;
        self.source = Some(updated);
        Ok(())
    }

    fn create(&self) -> io::Result<()> {
        let directory = parent(&self.path);
        fs::create_dir_all(directory)?;
        let mut document =
            parse(include_str!("../assets/config.toml")).expect("Valid bundled configuration");
        write(&mut document, &Preferences::default());
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(document.to_string().as_bytes())?;
        temporary.as_file().sync_all()?;
        // A second launch may have created the file since our read; never truncate it.
        match temporary.persist_noclobber(&self.path) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error.error),
        }
    }

    fn error(&self, error: impl std::fmt::Display) -> String {
        format!("{}: {error}", self.path.display())
    }
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn parse(source: &str) -> Result<DocumentMut, String> {
    source
        .parse()
        .map_err(|error| format!("Invalid TOML: {error}"))
}

fn read(document: &DocumentMut) -> Result<Preferences, String> {
    let defaults = Preferences::default();
    let empty = Table::new();
    let ui = section(document.get("ui"), "ui", &empty)?;
    let fonts = section(ui.get("fonts"), "ui.fonts", &empty)?;
    let updates = section(document.get("updates"), "updates", &empty)?;
    let theme = string(ui.get("theme"), "ui.theme", "dark")?;
    let light_theme = match theme {
        "dark" => false,
        "light" => true,
        _ => return Err("ui.theme must be \"dark\" or \"light\"".into()),
    };
    let code = string(ui.get("language"), "ui.language", "system")?;
    let language = Language::ALL
        .into_iter()
        .find(|language| language_code(*language) == code)
        .ok_or("ui.language must be system, zh-CN, zh-TW, en, ja, ko, fr, de, es, pt or ru")?;
    let font_family = string(
        fonts.get("code_family"),
        "ui.fonts.code_family",
        &defaults.font_family,
    )?;
    if font_family.trim().is_empty() || font_family.chars().any(char::is_control) {
        return Err(
            "ui.fonts.code_family must be a nonempty font name without control characters".into(),
        );
    }
    Ok(Preferences {
        font_family: font_family.into(),
        code_size: integer(
            fonts.get("code_size"),
            "ui.fonts.code_size",
            defaults.code_size,
            11..=24,
        )?,
        ui_size: integer(
            fonts.get("ui_size"),
            "ui.fonts.ui_size",
            defaults.ui_size,
            11..=16,
        )?,
        light_theme,
        language,
        updates: Settings {
            auto_check: boolean(
                updates.get("auto_check"),
                "updates.auto_check",
                defaults.updates.auto_check,
            )?,
            interval_minutes: integer(
                updates.get("interval_minutes"),
                "updates.interval_minutes",
                defaults.updates.interval_minutes,
                MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES,
            )?,
        },
    })
}

fn section<'a>(
    item: Option<&'a Item>,
    name: &str,
    empty: &'a Table,
) -> Result<&'a dyn TableLike, String> {
    match item {
        None => Ok(empty),
        Some(item) => item
            .as_table_like()
            .ok_or_else(|| format!("{name} must be a table")),
    }
}

fn string<'a>(item: Option<&'a Item>, key: &str, default: &'a str) -> Result<&'a str, String> {
    match item {
        None => Ok(default),
        Some(item) => item
            .as_str()
            .ok_or_else(|| format!("{key} must be a string")),
    }
}

fn boolean(item: Option<&Item>, key: &str, default: bool) -> Result<bool, String> {
    match item {
        None => Ok(default),
        Some(item) => item
            .as_bool()
            .ok_or_else(|| format!("{key} must be a boolean")),
    }
}

fn integer<T: TryFrom<i64> + Ord + Copy + std::fmt::Display>(
    item: Option<&Item>,
    key: &str,
    default: T,
    range: std::ops::RangeInclusive<T>,
) -> Result<T, String> {
    match item {
        None => Ok(default),
        Some(item) => item
            .as_integer()
            .and_then(|value| T::try_from(value).ok())
            .filter(|value| range.contains(value))
            .ok_or_else(|| {
                format!(
                    "{key} must be an integer from {} to {}",
                    range.start(),
                    range.end()
                )
            }),
    }
}

fn language_code(language: Language) -> &'static str {
    // `code()` resolves the system locale; persistence must retain the user's System choice.
    if language == Language::System {
        "system"
    } else {
        language.code()
    }
}

fn write(document: &mut DocumentMut, preferences: &Preferences) {
    set(
        &mut document["ui"]["theme"],
        Value::from(if preferences.light_theme {
            "light"
        } else {
            "dark"
        }),
    );
    set(
        &mut document["ui"]["language"],
        Value::from(language_code(preferences.language)),
    );
    for (key, value) in [
        ("code_family", Value::from(preferences.font_family.as_str())),
        ("code_size", Value::from(i64::from(preferences.code_size))),
        ("ui_size", Value::from(i64::from(preferences.ui_size))),
    ] {
        set(&mut document["ui"]["fonts"][key], value);
    }
    set(
        &mut document["updates"]["auto_check"],
        Value::from(preferences.updates.auto_check),
    );
    set(
        &mut document["updates"]["interval_minutes"],
        Value::from(i64::from(preferences.updates.interval_minutes)),
    );
}

fn set(item: &mut Item, mut value: Value) {
    if let Some(previous) = item.as_value() {
        if previous
            .as_str()
            .zip(value.as_str())
            .is_some_and(|(a, b)| a == b)
            || previous
                .as_bool()
                .zip(value.as_bool())
                .is_some_and(|(a, b)| a == b)
            || previous
                .as_integer()
                .zip(value.as_integer())
                .is_some_and(|(a, b)| a == b)
        {
            return;
        }
        *value.decor_mut() = previous.decor().clone();
    }
    *item = Item::Value(value);
}
