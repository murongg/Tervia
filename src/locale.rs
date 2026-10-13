use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    System,
    Chinese,
    TraditionalChinese,
    English,
    Japanese,
    Korean,
    French,
    German,
    Spanish,
    Portuguese,
    Russian,
}

impl Language {
    pub const ALL: [Self; 11] = [
        Self::System,
        Self::Chinese,
        Self::TraditionalChinese,
        Self::English,
        Self::Japanese,
        Self::Korean,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Portuguese,
        Self::Russian,
    ];

    pub fn from_locale(locale: &str) -> Option<Self> {
        let locale = locale.replace('_', "-").to_ascii_lowercase();
        let parts: Vec<_> = locale.split('-').collect();
        Some(match parts.first().copied()? {
            // An explicit writing system wins over a region (e.g. zh-Hans-TW).
            "zh" if parts.contains(&"hans") => Self::Chinese,
            "zh" if parts
                .iter()
                .any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo")) =>
            {
                Self::TraditionalChinese
            }
            "zh" => Self::Chinese,
            "en" => Self::English,
            "ja" => Self::Japanese,
            "ko" => Self::Korean,
            "fr" => Self::French,
            "de" => Self::German,
            "es" => Self::Spanish,
            "pt" => Self::Portuguese,
            "ru" => Self::Russian,
            _ => return None,
        })
    }

    pub fn preferred(locales: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        locales
            .into_iter()
            .find_map(|locale| Self::from_locale(locale.as_ref()))
            .unwrap_or(Self::English)
    }

    pub fn resolved(self) -> Self {
        static SYSTEM: OnceLock<Language> = OnceLock::new();
        if self == Self::System {
            *SYSTEM.get_or_init(|| Self::preferred(sys_locale::get_locales()))
        } else {
            self
        }
    }

    pub fn code(self) -> &'static str {
        match self.resolved() {
            Self::Chinese => "zh-CN",
            Self::TraditionalChinese => "zh-TW",
            Self::English => "en",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
            Self::Portuguese => "pt",
            Self::Russian => "ru",
            Self::System => unreachable!(),
        }
    }

    pub fn label(self, display: Self) -> &'static str {
        if self == Self::System {
            display.text("follow_system")
        } else {
            self.text("language_native_name")
        }
    }

    pub fn text(self, key: &str) -> &str {
        let catalogs = catalogs();
        catalogs
            .get(self.code())
            .and_then(|catalog| catalog.get(key))
            .or_else(|| catalogs.get("en").and_then(|catalog| catalog.get(key)))
            .map(String::as_str)
            .unwrap_or(key)
    }

    pub fn format(self, key: &str, arguments: &[(&str, &str)]) -> String {
        let mut remaining = self.text(key);
        let mut output = String::new();
        while let Some(start) = remaining.find('{') {
            let Some(end) = remaining[start..].find('}').map(|end| start + end) else {
                break;
            };
            output.push_str(&remaining[..start]);
            let name = &remaining[start + 1..end];
            if let Some((_, value)) = arguments.iter().find(|(key, _)| *key == name) {
                // Substitute once; a path or user value containing braces is literal content.
                output.push_str(value);
            } else {
                output.push_str(&remaining[start..=end]);
            }
            remaining = &remaining[end + 1..];
        }
        output.push_str(remaining);
        output
    }

    pub fn validate_catalogs() -> Result<(), String> {
        let catalogs = catalogs();
        let baseline = &catalogs["en"];
        for language in Self::ALL
            .into_iter()
            .filter(|language| *language != Self::System)
        {
            let Some(catalog) = catalogs.get(language.code()) else {
                return Err(format!("Missing {}", language.code()));
            };
            if catalog.keys().ne(baseline.keys()) {
                return Err(format!("Incomplete {}", language.code()));
            }
            for (key, value) in catalog {
                if value.is_empty() || placeholders(&baseline[key]) != placeholders(value) {
                    return Err(format!("Invalid {}: {key}", language.code()));
                }
            }
        }
        Ok(())
    }
}

fn placeholders(text: &str) -> BTreeSet<&str> {
    text.split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(key, _)| key))
        .collect()
}

type Catalogs = BTreeMap<String, BTreeMap<String, String>>;

fn catalogs() -> &'static Catalogs {
    static CATALOGS: OnceLock<Catalogs> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/locales.json"))
            .expect("Valid bundled language catalogs")
    })
}
