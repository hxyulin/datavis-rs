//! Internationalization (i18n) support
//!
//! Provides language selection and translation functions.
//!
//! The `i18n!` macro is initialized at the crate root (lib.rs).

/// Supported languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    /// Get the locale code for this language
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::SimplifiedChinese => "zh-CN",
        }
    }

    /// Get the display name for this language (in its native script)
    pub fn display_name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::SimplifiedChinese => "简体中文",
        }
    }

    /// Get all available languages
    pub fn all() -> &'static [Language] {
        &[Language::English, Language::SimplifiedChinese]
    }

    /// Parse a language from its locale code
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Language::English),
            "zh-CN" => Some(Language::SimplifiedChinese),
            _ => None,
        }
    }
}

/// Set the current language
pub fn set_language(lang: Language) {
    rust_i18n::set_locale(lang.code());
}

/// Get the current language
pub fn current_language() -> Language {
    let locale = rust_i18n::locale();
    Language::from_code(&locale).unwrap_or_default()
}
