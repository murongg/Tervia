pub(crate) mod document;
use document::set;

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use toml_edit::{Array, DocumentMut, Item, Table, TableLike, Value};

use crate::{
    locale::Language,
    preferences::{Preferences, Theme},
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
        let preferences = read(&document, &crate::themes::Store::new(self.path.clone()))
            .map_err(|error| self.error(error))?;
        self.source = Some(source);
        Ok(preferences)
    }

    pub fn save(&mut self, preferences: &Preferences) -> Result<(), String> {
        let source = self.source.as_ref().ok_or_else(|| {
            self.error("Configuration was not loaded; fix the file and restart Tervia")
        })?;
        let mut document = parse(source).map_err(|error| self.error(error))?;
        write(&mut document, preferences);
        read(&document, &crate::themes::Store::new(self.path.clone()))
            .map_err(|error| self.error(error))?;
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

fn read(document: &DocumentMut, store: &crate::themes::Store) -> Result<Preferences, String> {
    let defaults = Preferences::default();
    let empty = Table::new();
    let ui = section(document.get("ui"), "ui", &empty)?;
    let fonts = section(ui.get("fonts"), "ui.fonts", &empty)?;
    let updates = section(document.get("updates"), "updates", &empty)?;
    let files = section(document.get("files"), "files", &empty)?;
    let terminal = section(document.get("terminal"), "terminal", &empty)?;
    let agents = section(document.get("agents"), "agents", &empty)?;
    let mode = string(ui.get("theme"), "ui.theme", "dark")?;
    let legacy_theme = if mode == "system" {
        Theme::Dark
    } else {
        Theme::from_id(mode).ok_or_else(|| {
            format!(
                "ui.theme must be system or one of: {}",
                Theme::ALL
                    .iter()
                    .map(Theme::id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?
    };
    let mut theme = crate::preferences::Themes::from(legacy_theme);
    if mode == "system" {
        theme.set_mode(crate::preferences::Mode::System);
    }
    for (key, light) in [("dark_theme", false), ("light_theme", true)] {
        let name = format!("ui.{key}");
        let fallback = theme.selected(light);
        let id = string(ui.get(key), &name, fallback.id())?;
        let selected = if crate::themes::valid_id(id) {
            // A removed or broken custom file must not prevent the rest of settings loading.
            Some(
                store
                    .load(id)
                    .map(|document| Theme::Custom(document.data))
                    .unwrap_or_else(|_| Theme::from(light)),
            )
        } else {
            Theme::from_id(id)
        }
        .filter(|theme| theme.is_light() == light)
        .ok_or_else(|| {
            format!(
                "{name} must be {}",
                Theme::ALL
                    .iter()
                    .filter(|theme| theme.is_light() == light)
                    .map(Theme::id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        theme.select(selected);
    }
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
        theme,
        language,
        file_nesting: boolean(files.get("nesting"), "files.nesting", defaults.file_nesting)?,
        command_suggestions: boolean(
            terminal.get("suggestions"),
            "terminal.suggestions",
            defaults.command_suggestions,
        )?,
        agent_notifications: boolean(
            agents.get("notifications"),
            "agents.notifications",
            defaults.agent_notifications,
        )?,
        shell: crate::shell::Profile::from_id(string(
            terminal.get("shell"),
            "terminal.shell",
            "default",
        )?)
        .ok_or("terminal.shell must be default, pwsh, powershell, cmd or an absolute executable path without NULs")?,
        shell_args: terminal
            .get("shell_args")
            .map(|item| {
                item.as_array()
                    .ok_or("terminal.shell_args must be an array of strings without NULs")?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|value| !value.contains('\0'))
                            .map(str::to_owned)
                            .ok_or("terminal.shell_args must be an array of strings without NULs")
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?,
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
        &mut document["agents"]["notifications"],
        Value::from(preferences.agent_notifications),
    );
    set(
        &mut document["terminal"]["shell"],
        Value::from(preferences.shell.id()),
    );
    if let Some(arguments) = &preferences.shell_args {
        set(
            &mut document["terminal"]["shell_args"],
            Value::Array(arguments.iter().map(String::as_str).collect::<Array>()),
        );
    } else if let Some(terminal) = document["terminal"].as_table_like_mut() {
        terminal.remove("shell_args");
    }
    set(
        &mut document["terminal"]["suggestions"],
        Value::from(preferences.command_suggestions),
    );
    set(
        &mut document["files"]["nesting"],
        Value::from(preferences.file_nesting),
    );
    set(
        &mut document["ui"]["theme"],
        Value::from(preferences.theme.mode().id()),
    );
    for (key, light) in [("dark_theme", false), ("light_theme", true)] {
        set(
            &mut document["ui"][key],
            Value::from(preferences.theme.selected(light).id()),
        );
    }
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
