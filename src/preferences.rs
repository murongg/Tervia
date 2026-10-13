#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub font_family: String,
    pub code_size: u8,
    pub ui_size: u8,
    pub light_theme: bool,
    pub language: Language,
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
        // Resetting Interface settings must not re-enable automatic network checks.
        *self = Self {
            updates: self.updates,
            ..Self::default()
        };
    }
}
use crate::{locale::Language, platform::Platform};
