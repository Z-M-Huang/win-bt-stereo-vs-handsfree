//! Windows toast notification handling

use crate::audio::device::AudioMode;
use crate::error::{AppError, ErrorSeverity, Result};
use log::{debug, info, warn};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::{HSTRING, PCWSTR};
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
};

/// Application User Model ID for toast notifications
/// This must match any Start menu shortcut for the app to work properly
const APP_USER_MODEL_ID: &str = "Z-M-Huang.BtAudioModeManager";

/// Display name shown in notification center
const APP_DISPLAY_NAME: &str = "Bluetooth Audio Manager";

/// Register the Application User Model ID (AUMID) in the Windows Registry.
/// This is required for toast notifications to appear in the notification center
/// for unpackaged desktop applications.
///
/// The registration is done under HKEY_CURRENT_USER so no admin privileges are required.
pub fn register_aumid() -> Result<()> {
    unsafe {
        // Registry path: HKEY_CURRENT_USER\Software\Classes\AppUserModelId\<AUMID>
        let subkey = format!("Software\\Classes\\AppUserModelId\\{}", APP_USER_MODEL_ID);
        let subkey_wide: Vec<u16> = OsStr::new(&subkey)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut hkey = HKEY::default();
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(subkey_wide.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );

        if result.is_err() {
            warn!("Failed to create registry key for AUMID: {:?}", result);
            return Err(AppError::ConfigError(format!(
                "Failed to create AUMID registry key: {:?}",
                result
            )));
        }

        // Set DisplayName value
        let display_name_wide: Vec<u16> = OsStr::new(APP_DISPLAY_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let value_name: Vec<u16> = OsStr::new("DisplayName")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let result = RegSetValueExW(
            hkey,
            PCWSTR::from_raw(value_name.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                display_name_wide.as_ptr() as *const u8,
                display_name_wide.len() * 2,
            )),
        );

        if result.is_err() {
            warn!("Failed to set DisplayName registry value: {:?}", result);
        }

        // Set IconUri value (path to app icon)
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let icon_path = exe_dir.join("resources").join("app.ico");
                let icon_path_str = icon_path.to_string_lossy();
                let icon_uri_wide: Vec<u16> = OsStr::new(icon_path_str.as_ref())
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let icon_value_name: Vec<u16> = OsStr::new("IconUri")
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                let result = RegSetValueExW(
                    hkey,
                    PCWSTR::from_raw(icon_value_name.as_ptr()),
                    0,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(
                        icon_uri_wide.as_ptr() as *const u8,
                        icon_uri_wide.len() * 2,
                    )),
                );

                if result.is_err() {
                    warn!("Failed to set IconUri registry value: {:?}", result);
                }
            }
        }

        // Close the registry key
        let _ = RegCloseKey(hkey);

        info!("AUMID registered successfully: {}", APP_USER_MODEL_ID);
        Ok(())
    }
}

/// Notification types
#[derive(Debug, Clone)]
pub enum NotificationType {
    /// Audio mode changed
    ModeChange { old: AudioMode, new: AudioMode },
    /// New app started using microphone
    MicUsageStart { app_name: String },
    /// App stopped using microphone
    MicUsageStop { app_name: String },
    /// Update available
    UpdateAvailable { version: String },
    /// Error notification
    Error { message: String, severity: ErrorSeverity },
    /// Generic info notification
    Info { title: String, message: String },
}

/// Manages Windows notifications
#[derive(Clone)]
pub struct NotificationManager {
    enabled: bool,
    notify_mode_change: bool,
    notify_mic_usage: bool,
    notify_errors: bool,
    notify_updates: bool,
    use_toast: bool,
    /// If true, always use MessageBox even when toast is enabled (for unregistered apps)
    force_message_box: bool,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            enabled: true,
            notify_mode_change: true,
            notify_mic_usage: true,
            notify_errors: true,
            notify_updates: true,
            use_toast: true,
            // Try toast first - it will appear briefly even without AUMID registration
            // If it doesn't work well, user can set this to true in settings
            force_message_box: false,
        }
    }

    /// Set whether to force MessageBox instead of toast (for unpackaged apps)
    pub fn set_force_message_box(&mut self, force: bool) {
        self.force_message_box = force;
    }

    /// Update notification settings
    pub fn update_settings(
        &mut self,
        notify_mode_change: bool,
        notify_mic_usage: bool,
        notify_errors: bool,
        notify_updates: bool,
    ) {
        self.notify_mode_change = notify_mode_change;
        self.notify_mic_usage = notify_mic_usage;
        self.notify_errors = notify_errors;
        self.notify_updates = notify_updates;
    }

    /// Enable or disable all notifications
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Show a notification based on type
    pub fn show(&self, notification: NotificationType) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        match &notification {
            NotificationType::ModeChange { old, new } => {
                if self.notify_mode_change {
                    let title = "Audio Mode Changed";
                    let message = format!("Switched from {} to {}", old, new);
                    self.show_notification(title, &message, ToastIcon::Info)?;
                }
            }
            NotificationType::MicUsageStart { app_name } => {
                if self.notify_mic_usage {
                    let title = "Microphone In Use";
                    let message = format!("{} started using the microphone", app_name);
                    self.show_notification(title, &message, ToastIcon::Info)?;
                }
            }
            NotificationType::MicUsageStop { app_name } => {
                if self.notify_mic_usage {
                    let title = "Microphone Released";
                    let message = format!("{} stopped using the microphone", app_name);
                    self.show_notification(title, &message, ToastIcon::Info)?;
                }
            }
            NotificationType::UpdateAvailable { version } => {
                if self.notify_updates {
                    let title = "Update Available";
                    let message = format!("Version {} is available. Check menu to update.", version);
                    self.show_notification(title, &message, ToastIcon::Info)?;
                }
            }
            NotificationType::Error { message, severity } => {
                if self.notify_errors {
                    let icon = match severity {
                        ErrorSeverity::Fatal => ToastIcon::Error,
                        ErrorSeverity::Recoverable => ToastIcon::Warning,
                        ErrorSeverity::Minor => return Ok(()), // Don't show toast for minor
                    };
                    let title = match severity {
                        ErrorSeverity::Fatal => "Error",
                        ErrorSeverity::Recoverable => "Warning",
                        ErrorSeverity::Minor => "Notice",
                    };
                    self.show_notification(title, message, icon)?;
                }
            }
            NotificationType::Info { title, message } => {
                self.show_notification(title, message, ToastIcon::Info)?;
            }
        }

        Ok(())
    }

    /// Show a notification - tries toast first, falls back to MessageBox
    fn show_notification(&self, title: &str, message: &str, icon: ToastIcon) -> Result<()> {
        // For unpackaged apps, toast notifications won't appear in the notification center
        // without proper AUMID registration (Start menu shortcut). Use MessageBox instead.
        if self.force_message_box {
            return self.show_message_box(title, message, icon);
        }

        if self.use_toast {
            match self.show_windows_toast(title, message) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    debug!("Toast notification failed, falling back to MessageBox: {}", e);
                }
            }
        }

        // Fallback to MessageBox
        self.show_message_box(title, message, icon)
    }

    /// Show Windows toast notification using WinRT API
    fn show_windows_toast(&self, title: &str, message: &str) -> Result<()> {
        // Escape XML special characters
        let title_escaped = escape_xml(title);
        let message_escaped = escape_xml(message);

        // Create toast XML content
        // Using ToastGeneric template for Windows 10/11
        let toast_xml = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastGeneric">
                        <text>{}</text>
                        <text>{}</text>
                    </binding>
                </visual>
                <audio silent="true"/>
            </toast>"#,
            title_escaped, message_escaped
        );

        // Parse the XML
        let xml_doc = XmlDocument::new()
            .map_err(|e| AppError::ConfigError(format!("Failed to create XmlDocument: {}", e)))?;

        xml_doc
            .LoadXml(&HSTRING::from(&toast_xml))
            .map_err(|e| AppError::ConfigError(format!("Failed to load toast XML: {}", e)))?;

        // Create the toast notification
        let toast = ToastNotification::CreateToastNotification(&xml_doc)
            .map_err(|e| AppError::ConfigError(format!("Failed to create toast: {}", e)))?;

        // Get the toast notifier with our App User Model ID
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))
            .map_err(|e| AppError::ConfigError(format!("Failed to create notifier: {}", e)))?;

        // Show the toast
        notifier
            .Show(&toast)
            .map_err(|e| AppError::ConfigError(format!("Failed to show toast: {}", e)))?;

        info!("Toast notification shown: {} - {}", title, message);
        Ok(())
    }

    /// Show a message box as fallback (async - spawns a thread)
    fn show_message_box(&self, title: &str, message: &str, icon: ToastIcon) -> Result<()> {
        let title = title.to_string();
        let message = message.to_string();

        // Spawn a thread so MessageBox doesn't block the main event loop
        std::thread::spawn(move || {
            let title_wide: Vec<u16> = OsStr::new(&title)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let message_wide: Vec<u16> = OsStr::new(&message)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let icon_flags = match icon {
                ToastIcon::Info => MB_ICONINFORMATION,
                ToastIcon::Warning => MB_ICONWARNING,
                ToastIcon::Error => MB_ICONERROR,
            };

            unsafe {
                MessageBoxW(
                    HWND::default(),
                    PCWSTR::from_raw(message_wide.as_ptr()),
                    PCWSTR::from_raw(title_wide.as_ptr()),
                    MB_OK | icon_flags | MB_SETFOREGROUND,
                );
            }
        });

        Ok(())
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Toast notification icon type (used for MessageBox fallback)
#[derive(Debug, Clone, Copy)]
enum ToastIcon {
    Info,
    Warning,
    Error,
}

/// Escape XML special characters
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_manager_new() {
        let manager = NotificationManager::new();
        assert!(manager.enabled);
        assert!(manager.notify_mode_change);
    }

    #[test]
    fn test_notification_disabled() {
        let mut manager = NotificationManager::new();
        manager.set_enabled(false);
        // Should not error even when disabled
        let result = manager.show(NotificationType::Info {
            title: "Test".to_string(),
            message: "Test message".to_string(),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("Hello & World"), "Hello &amp; World");
        assert_eq!(escape_xml("<test>"), "&lt;test&gt;");
        assert_eq!(escape_xml("\"quoted\""), "&quot;quoted&quot;");
    }
}
