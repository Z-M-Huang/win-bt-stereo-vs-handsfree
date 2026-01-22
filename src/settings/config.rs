//! Configuration management with versioning and migration

use crate::error::{AppError, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Current configuration version
pub const CONFIG_VERSION: u32 = 2;

/// Portable mode marker filename
const PORTABLE_MARKER: &str = "portable.txt";

/// Configuration filename
const CONFIG_FILENAME: &str = "config.toml";

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Configuration version for migration
    #[serde(default = "default_version")]
    pub config_version: u32,

    /// General settings
    #[serde(default)]
    pub general: GeneralConfig,

    /// Notification settings
    #[serde(default)]
    pub notifications: NotificationConfig,

    /// Logging settings
    #[serde(default)]
    pub logging: LoggingConfig,

    /// Update settings
    #[serde(default)]
    pub updates: UpdateConfig,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Start with Windows
    #[serde(default)]
    pub auto_start: bool,

    /// Start minimized to tray
    #[serde(default = "default_true")]
    pub start_minimized: bool,

    /// Automatically mute mic when stereo is preferred
    #[serde(default)]
    pub prefer_stereo: bool,

    /// Polling interval in milliseconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u32,

    /// Language override (None = use system locale, Some = use specified locale)
    #[serde(default)]
    pub language: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_poll_interval() -> u32 {
    500
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            start_minimized: true,
            prefer_stereo: false,
            poll_interval_ms: 500,
            language: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// Show notification on mode change
    #[serde(default = "default_true")]
    pub notify_mode_change: bool,

    /// Show notification when app starts using mic
    #[serde(default = "default_true")]
    pub notify_mic_usage: bool,

    /// Show notification for errors
    #[serde(default = "default_true")]
    pub notify_errors: bool,

    /// Show notification for updates
    #[serde(default = "default_true")]
    pub notify_updates: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            notify_mode_change: true,
            notify_mic_usage: true,
            notify_errors: true,
            notify_updates: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Maximum log file size in bytes
    #[serde(default = "default_max_log_size")]
    pub max_file_size: u64,

    /// Number of log files to keep
    #[serde(default = "default_max_log_files")]
    pub max_files: u32,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_max_log_size() -> u64 {
    5 * 1024 * 1024 // 5MB
}

fn default_max_log_files() -> u32 {
    3
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            max_file_size: 5 * 1024 * 1024,
            max_files: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Check for updates automatically
    #[serde(default = "default_true")]
    pub auto_check: bool,

    /// Check interval in hours
    #[serde(default = "default_check_interval")]
    pub check_interval_hours: u32,

    /// Last check timestamp (Unix timestamp)
    #[serde(default)]
    pub last_check: u64,

    /// Skipped version (don't notify for this version)
    #[serde(default)]
    pub skipped_version: Option<String>,
}

fn default_check_interval() -> u32 {
    24
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            check_interval_hours: 24,
            last_check: 0,
            skipped_version: None,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            general: GeneralConfig::default(),
            notifications: NotificationConfig::default(),
            logging: LoggingConfig::default(),
            updates: UpdateConfig::default(),
        }
    }
}

impl AppConfig {
    /// Migrate config from older version
    fn migrate(&mut self) {
        if self.config_version < CONFIG_VERSION {
            info!(
                "Migrating config from version {} to {}",
                self.config_version, CONFIG_VERSION
            );

            // Add migration logic here as versions are added
            // v1 to v2: Added language field to GeneralConfig
            // Old configs without language field will default to None (system locale)
            if self.config_version < 2 {
                // language field defaults to None via serde, no action needed
                info!("Migrated config from v1 to v2: added language field");
            }

            self.config_version = CONFIG_VERSION;
        }
    }
}

/// Manages configuration loading, saving, and migration
pub struct ConfigManager {
    config_path: PathBuf,
    is_portable: bool,
}

impl ConfigManager {
    /// Create a new config manager, detecting portable vs installed mode
    pub fn new() -> Result<Self> {
        let (config_path, is_portable) = Self::detect_config_path()?;
        Ok(Self {
            config_path,
            is_portable,
        })
    }

    /// Detect whether we're running in portable mode and get config path
    fn detect_config_path() -> Result<(PathBuf, bool)> {
        let exe_path = std::env::current_exe()
            .map_err(|e| AppError::ConfigError(format!("Could not get exe path: {}", e)))?;
        let exe_dir = exe_path.parent().ok_or_else(|| {
            AppError::ConfigError("Could not get exe directory".to_string())
        })?;

        // Check for portable marker
        let portable_marker = exe_dir.join(PORTABLE_MARKER);
        if portable_marker.exists() {
            debug!("Portable mode detected via marker file");
            return Ok((exe_dir.join(CONFIG_FILENAME), true));
        }

        // Check if running from Program Files (indicates installed mode)
        let is_program_files = exe_dir
            .to_string_lossy()
            .to_lowercase()
            .contains("program files");

        if is_program_files {
            // Installed mode - use AppData
            let app_data = std::env::var("LOCALAPPDATA")
                .map_err(|_| AppError::ConfigError("LOCALAPPDATA not set".to_string()))?;
            let config_dir = PathBuf::from(app_data).join("BtAudioModeManager");
            fs::create_dir_all(&config_dir)?;
            Ok((config_dir.join(CONFIG_FILENAME), false))
        } else {
            // Not in Program Files, treat as portable
            debug!("Portable mode detected (not in Program Files)");
            Ok((exe_dir.join(CONFIG_FILENAME), true))
        }
    }

    /// Check if running in portable mode
    pub fn is_portable(&self) -> bool {
        self.is_portable
    }

    /// Get the config file path
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Get the log directory
    pub fn log_dir(&self) -> PathBuf {
        if self.is_portable {
            self.config_path.parent().unwrap().join("logs")
        } else {
            self.config_path.parent().unwrap().to_path_buf()
        }
    }

    /// Load configuration from file
    pub fn load(&self) -> Result<AppConfig> {
        if !self.config_path.exists() {
            info!("Config file not found, using defaults");
            return Ok(AppConfig::default());
        }

        let content = fs::read_to_string(&self.config_path)
            .map_err(|e| AppError::ConfigError(format!("Could not read config: {}", e)))?;

        let mut config: AppConfig = toml::from_str(&content)
            .map_err(|e| AppError::ConfigError(format!("Could not parse config: {}", e)))?;

        // Migrate if needed
        if config.config_version < CONFIG_VERSION {
            config.migrate();
            // Save migrated config
            self.save(&config)?;
        }

        info!("Loaded config from {:?}", self.config_path);
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, config: &AppConfig) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(config)
            .map_err(|e| AppError::ConfigError(format!("Could not serialize config: {}", e)))?;

        fs::write(&self.config_path, content)
            .map_err(|e| AppError::ConfigError(format!("Could not write config: {}", e)))?;

        info!("Saved config to {:?}", self.config_path);
        Ok(())
    }

    /// Set auto-start in Windows registry
    pub fn set_auto_start(&self, enabled: bool) -> Result<()> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        };
        use std::os::windows::ffi::OsStrExt;

        let key_path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let value_name = "BtAudioModeManager";

        let key_path_wide: Vec<u16> = std::ffi::OsStr::new(key_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let value_name_wide: Vec<u16> = std::ffi::OsStr::new(value_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let mut key = HKEY::default();
            let result = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(key_path_wide.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut key,
            );

            if result.is_err() {
                return Err(AppError::ConfigError(
                    "Could not open registry key".to_string(),
                ));
            }

            let result = if enabled {
                let exe_path = std::env::current_exe()
                    .map_err(|e| AppError::ConfigError(format!("Could not get exe path: {}", e)))?;
                // Quote the path to prevent CWE-428 (Unquoted Search Path) vulnerability
                // when path contains spaces
                let exe_path_quoted = format!("\"{}\"", exe_path.to_string_lossy());
                let exe_path_wide: Vec<u16> = std::ffi::OsStr::new(&exe_path_quoted)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                RegSetValueExW(
                    key,
                    PCWSTR::from_raw(value_name_wide.as_ptr()),
                    0,
                    REG_SZ,
                    Some(&exe_path_wide.iter().flat_map(|&x| x.to_le_bytes()).collect::<Vec<_>>()),
                )
            } else {
                RegDeleteValueW(key, PCWSTR::from_raw(value_name_wide.as_ptr()))
            };

            let _ = RegCloseKey(key);

            if result.is_err() {
                return Err(AppError::ConfigError(format!(
                    "Could not {} auto-start",
                    if enabled { "enable" } else { "disable" }
                )));
            }
        }

        info!("Auto-start {}", if enabled { "enabled" } else { "disabled" });
        Ok(())
    }

    /// Check if auto-start is enabled
    pub fn is_auto_start_enabled(&self) -> bool {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        };
        use std::os::windows::ffi::OsStrExt;

        let key_path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let value_name = "BtAudioModeManager";

        let key_path_wide: Vec<u16> = std::ffi::OsStr::new(key_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let value_name_wide: Vec<u16> = std::ffi::OsStr::new(value_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(key_path_wide.as_ptr()),
                0,
                KEY_READ,
                &mut key,
            )
            .is_err()
            {
                return false;
            }

            let mut size = 0u32;
            let result = RegQueryValueExW(
                key,
                PCWSTR::from_raw(value_name_wide.as_ptr()),
                None,
                None,
                None,
                Some(&mut size),
            );

            let _ = RegCloseKey(key);
            result.is_ok() && size > 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.config_version, CONFIG_VERSION);
        assert!(!config.general.auto_start);
        assert!(config.general.start_minimized);
        assert_eq!(config.general.poll_interval_ms, 500);
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.config_version, config.config_version);
    }
}
