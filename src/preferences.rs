#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub font_family: String,
    pub code_size: u8,
    pub ui_size: u8,
    pub theme: Themes,
    pub language: Language,
    pub file_nesting: bool,
    pub command_suggestions: bool,
    pub agent_notifications: bool,
    pub shell: crate::shell::Profile,
    pub updates: crate::updates::Settings,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            font_family: Platform::CURRENT.mono_font().into(),
            code_size: 14,
            ui_size: 13,
            theme: Themes::default(),
            language: Language::System,
            file_nesting: false,
            command_suggestions: true,
            agent_notifications: false,
            shell: crate::shell::Profile::Default,
            updates: crate::updates::Settings::default(),
        }
    }
}

impl Preferences {
    pub fn set_code_size(&mut self, size: u8) {
        self.code_size = size.clamp(11, 24);
    }
    pub fn set_ui_size(&mut self, size: u8) {
        self.ui_size = size.clamp(11, 16);
    }
    pub fn reset(&mut self) {
        // Resetting appearance must not resume network checks or command-history recording.
        self.reset_general();
        self.reset_appearance();
        self.reset_workspace();
    }

    pub fn reset_general(&mut self) {
        self.language = Self::default().language;
    }

    pub fn reset_appearance(&mut self) {
        let defaults = Self::default();
        self.theme = defaults.theme;
        self.ui_size = defaults.ui_size;
        self.font_family = defaults.font_family;
        self.code_size = defaults.code_size;
    }

    pub fn reset_workspace(&mut self) {
        // History recording is an explicit choice, even when the rest of this page is reset.
        let defaults = Self::default();
        self.file_nesting = defaults.file_nesting;
        self.shell = defaults.shell;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Midnight,
    Forest,
    Paper,
    Rose,
    Darcula,
    Monokai,
    SolarizedDark,
    SolarizedLight,
    QuietLight,
    Custom(std::sync::Arc<crate::themes::Definition>),
}

impl Theme {
    pub const ALL: [Self; 11] = [
        Self::Dark,
        Self::Light,
        Self::Midnight,
        Self::Forest,
        Self::Paper,
        Self::Rose,
        Self::Darcula,
        Self::Monokai,
        Self::SolarizedDark,
        Self::SolarizedLight,
        Self::QuietLight,
    ];

    pub fn id(&self) -> &str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Midnight => "midnight",
            Self::Forest => "forest",
            Self::Paper => "paper",
            Self::Rose => "rose",
            Self::Darcula => "darcula",
            Self::Monokai => "monokai",
            Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light",
            Self::QuietLight => "quiet-light",
            Self::Custom(data) => data.id(),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.id() == id)
    }

    pub fn custom(&self) -> Option<&crate::themes::Definition> {
        match self {
            Self::Custom(data) => Some(data),
            _ => None,
        }
    }

    pub fn is_light(&self) -> bool {
        if let Self::Custom(data) = self {
            return data.is_light();
        }
        matches!(
            self,
            Self::Light | Self::Paper | Self::Rose | Self::SolarizedLight | Self::QuietLight
        )
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Midnight => "theme_midnight",
            Self::Forest => "theme_forest",
            Self::Paper => "theme_paper",
            Self::Rose => "theme_rose",
            Self::Darcula => "theme_darcula",
            Self::Monokai => "theme_monokai",
            Self::SolarizedDark => "theme_solarized_dark",
            Self::SolarizedLight => "theme_solarized_light",
            Self::QuietLight => "theme_quiet_light",
            Self::Custom(_) => "theme_custom",
        }
    }

    pub fn label(&self, language: Language) -> &str {
        if let Self::Custom(data) = self {
            return data.name();
        }
        language.text(self.key())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
    System,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::System];

    pub fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Themes {
    dark: Theme,
    light: Theme,
    light_mode: bool,
    mode: Mode,
}

impl Default for Themes {
    fn default() -> Self {
        Self {
            dark: Theme::Dark,
            light: Theme::Light,
            light_mode: false,
            mode: Mode::Dark,
        }
    }
}

impl Themes {
    pub fn current(&self) -> Theme {
        self.selected(self.light_mode)
    }

    pub fn selected(&self, light: bool) -> Theme {
        if light {
            self.light.clone()
        } else {
            self.dark.clone()
        }
    }

    pub fn is_light(&self) -> bool {
        self.light_mode
    }

    pub fn select(&mut self, theme: Theme) {
        // Configuring the inactive mode must not change the current appearance.
        if theme.is_light() {
            self.light = theme;
        } else {
            self.dark = theme;
        }
    }

    pub fn remove_custom(&mut self, id: &str) -> bool {
        let mut removed = false;
        for light in [false, true] {
            if self
                .selected(light)
                .custom()
                .is_some_and(|data| data.id() == id)
            {
                self.select(Theme::from(light));
                removed = true;
            }
        }
        removed
    }

    pub fn set_light(&mut self, light: bool) {
        self.light_mode = light;
        self.mode = if light { Mode::Light } else { Mode::Dark };
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        match mode {
            Mode::Dark => self.set_light(false),
            Mode::Light => self.set_light(true),
            Mode::System => self.mode = Mode::System,
        }
    }

    pub fn sync_system(&mut self, light: bool) -> bool {
        if self.mode != Mode::System || self.light_mode == light {
            return false;
        }
        self.light_mode = light;
        true
    }

    pub fn toggle(&mut self) {
        self.set_light(!self.light_mode);
    }
}

impl From<Theme> for Themes {
    fn from(theme: Theme) -> Self {
        let mut themes = Self::default();
        themes.set_light(theme.is_light());
        themes.select(theme);
        themes
    }
}

impl From<Themes> for Theme {
    fn from(themes: Themes) -> Self {
        themes.current()
    }
}

impl From<bool> for Themes {
    fn from(light: bool) -> Self {
        Theme::from(light).into()
    }
}

impl From<bool> for Theme {
    fn from(light: bool) -> Self {
        if light { Self::Light } else { Self::Dark }
    }
}
use crate::{locale::Language, platform::Platform};

impl From<&Themes> for Theme {
    fn from(themes: &Themes) -> Self {
        themes.current()
    }
}

impl From<&Theme> for Theme {
    fn from(theme: &Theme) -> Self {
        theme.clone()
    }
}

impl From<&Theme> for Themes {
    fn from(theme: &Theme) -> Self {
        theme.clone().into()
    }
}
