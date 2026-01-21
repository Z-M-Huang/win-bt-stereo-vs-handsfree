//! Tests for configuration loading, saving, and migration

use win_bt_stereo_vs_handsfree::settings::config::{AppConfig, CONFIG_VERSION};

#[test]
fn test_default_config() {
    let config = AppConfig::default();

    assert_eq!(config.config_version, CONFIG_VERSION);
    assert!(!config.general.auto_start);
    assert!(config.general.start_minimized);
    assert!(!config.general.prefer_stereo);
    assert_eq!(config.general.poll_interval_ms, 500);
}

#[test]
fn test_default_notification_config() {
    let config = AppConfig::default();

    assert!(config.notifications.notify_mode_change);
    assert!(config.notifications.notify_mic_usage);
    assert!(config.notifications.notify_errors);
    assert!(config.notifications.notify_updates);
}

#[test]
fn test_default_logging_config() {
    let config = AppConfig::default();

    assert_eq!(config.logging.level, "info");
    assert_eq!(config.logging.max_file_size, 5 * 1024 * 1024);
    assert_eq!(config.logging.max_files, 3);
}

#[test]
fn test_default_update_config() {
    let config = AppConfig::default();

    assert!(config.updates.auto_check);
    assert_eq!(config.updates.check_interval_hours, 24);
    assert_eq!(config.updates.last_check, 0);
    assert!(config.updates.skipped_version.is_none());
}

#[test]
fn test_config_serialization() {
    let config = AppConfig::default();

    let toml_str = toml::to_string(&config).expect("Serialization failed");
    assert!(!toml_str.is_empty());

    let parsed: AppConfig = toml::from_str(&toml_str).expect("Deserialization failed");
    assert_eq!(parsed.config_version, config.config_version);
    assert_eq!(parsed.general.auto_start, config.general.auto_start);
}

#[test]
fn test_config_roundtrip() {
    let mut config = AppConfig::default();
    config.general.auto_start = true;
    config.general.prefer_stereo = true;
    config.notifications.notify_mode_change = false;
    config.updates.skipped_version = Some("1.2.3".to_string());

    let toml_str = toml::to_string(&config).expect("Serialization failed");
    let parsed: AppConfig = toml::from_str(&toml_str).expect("Deserialization failed");

    assert!(parsed.general.auto_start);
    assert!(parsed.general.prefer_stereo);
    assert!(!parsed.notifications.notify_mode_change);
    assert_eq!(parsed.updates.skipped_version, Some("1.2.3".to_string()));
}

#[test]
fn test_config_partial_deserialization() {
    // Test that missing fields get default values
    let partial_toml = r#"
        config_version = 1

        [general]
        auto_start = true
    "#;

    let config: AppConfig = toml::from_str(partial_toml).expect("Partial deserialization failed");

    assert_eq!(config.config_version, 1);
    assert!(config.general.auto_start);
    // Defaults for missing fields
    assert!(config.general.start_minimized);
    assert!(config.notifications.notify_mode_change);
}

#[test]
fn test_config_with_extra_fields() {
    // Test that unknown fields are ignored
    let toml_with_extra = r#"
        config_version = 1
        unknown_field = "should be ignored"

        [general]
        auto_start = false
        also_unknown = 123

        [notifications]
        notify_mode_change = true
    "#;

    let config: AppConfig = toml::from_str(toml_with_extra).expect("Deserialization with extra fields failed");
    assert_eq!(config.config_version, 1);
    assert!(!config.general.auto_start);
}

#[test]
fn test_config_version() {
    // Ensure CONFIG_VERSION is at least 1
    assert!(CONFIG_VERSION >= 1);
}

#[test]
fn test_poll_interval_bounds() {
    let config = AppConfig::default();
    // Poll interval should be reasonable (not too fast, not too slow)
    assert!(config.general.poll_interval_ms >= 100);
    assert!(config.general.poll_interval_ms <= 10000);
}

#[test]
fn test_log_max_size_reasonable() {
    let config = AppConfig::default();
    // Log size should be reasonable (1MB to 100MB)
    assert!(config.logging.max_file_size >= 1024 * 1024);
    assert!(config.logging.max_file_size <= 100 * 1024 * 1024);
}

#[test]
fn test_update_interval_reasonable() {
    let config = AppConfig::default();
    // Update check interval should be at least 1 hour, at most 1 week
    assert!(config.updates.check_interval_hours >= 1);
    assert!(config.updates.check_interval_hours <= 168);
}
