//! Internationalization support using rust-i18n
//!
//! Provides locale detection from Windows and initialization of the i18n system.

use log::{info, warn};
use windows::Win32::Globalization::GetUserDefaultLocaleName;

/// Detect the user's OS locale using Windows API
///
/// Returns the locale string (e.g., "en-US", "zh-CN") or falls back to "en" on failure.
pub fn detect_locale() -> String {
    unsafe {
        let mut buffer = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
        let len = GetUserDefaultLocaleName(&mut buffer);

        if len > 0 && len <= buffer.len() as i32 {
            // Convert UTF-16 to String, removing the null terminator
            match String::from_utf16(&buffer[..len as usize - 1]) {
                Ok(locale) => {
                    info!("Detected system locale: {}", locale);
                    locale
                }
                Err(e) => {
                    warn!("Failed to convert locale to UTF-8: {}, falling back to 'en'", e);
                    "en".to_string()
                }
            }
        } else {
            warn!("GetUserDefaultLocaleName failed or returned invalid length, falling back to 'en'");
            "en".to_string()
        }
    }
}

/// Initialize the i18n system with optional language override
///
/// If `config_language` is Some, uses that locale. Otherwise, detects the system locale.
pub fn init(config_language: Option<&str>) {
    let locale = match config_language {
        Some(lang) => {
            info!("Using configured language: {}", lang);
            lang.to_string()
        }
        None => detect_locale(),
    };

    rust_i18n::set_locale(&locale);
    info!("Locale set to: {}", locale);
}

/// Get list of supported languages with their display names
///
/// Returns a vector of (locale_code, display_name) tuples for use in settings UI.
/// The first entry is empty string for "System Default".
pub fn get_language_display_names() -> Vec<(&'static str, &'static str)> {
    vec![
        ("", "System Default"),
        ("en", "English"),
        ("zh-CN", "简体中文"),
        ("zh-TW", "繁體中文"),
        ("es", "Español"),
        ("de", "Deutsch"),
        ("fr", "Français"),
        ("ja", "日本語"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_locale_returns_string() {
        let locale = detect_locale();
        assert!(!locale.is_empty(), "Locale should not be empty");
        assert!(locale.len() >= 2, "Locale should be at least 2 characters (e.g., 'en')");
    }

    #[test]
    fn test_init_with_none_uses_system_locale() {
        // This test verifies that init(None) doesn't crash
        // We can't assert the exact locale as it depends on the test environment
        init(None);
        let current_locale = rust_i18n::locale();
        assert!(!current_locale.is_empty());
    }

    #[test]
    fn test_init_with_some_uses_override() {
        init(Some("ja"));
        let current_locale = rust_i18n::locale();
        // rust-i18n may normalize the locale (e.g., "ja" might become "ja-JP")
        // so we just verify it starts with "ja"
        assert!(current_locale.starts_with("ja"), "Locale should be Japanese");
    }

    #[test]
    fn test_get_language_display_names_returns_expected_list() {
        let languages = get_language_display_names();
        assert_eq!(languages.len(), 8, "Should have 8 language options");
        assert_eq!(languages[0].0, "", "First option should be empty string for system default");
        assert_eq!(languages[1].0, "en", "Second option should be English");
        assert_eq!(languages[2].0, "zh-CN", "Third option should be Simplified Chinese");
    }
}
