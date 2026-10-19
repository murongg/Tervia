#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub font_family: String,
    pub code_size: u8,
    pub ui_size: u8,
    pub light_theme: bool,
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
            light_theme: false,
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
        self.reset_workspace();
    }

    pub fn reset_general(&mut self) {
        let defaults = Self::default();
        self.light_theme = defaults.light_theme;
        self.language = defaults.language;
        self.ui_size = defaults.ui_size;
    }

    pub fn reset_workspace(&mut self) {
        // History recording is an explicit choice, even when the rest of this page is reset.
        let defaults = Self::default();
        self.font_family = defaults.font_family;
        self.code_size = defaults.code_size;
        self.file_nesting = defaults.file_nesting;
        self.shell = defaults.shell;
    }
}
use crate::{locale::Language, platform::Platform};
