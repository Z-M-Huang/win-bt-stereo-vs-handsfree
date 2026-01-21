//! Settings window UI using native-windows-gui

use crate::error::{AppError, Result};
use crate::settings::config::{AppConfig, ConfigManager};
use log::debug;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Messages for settings window communication
#[derive(Debug, Clone)]
pub enum SettingsMessage {
    /// Request to open settings window
    Open,
    /// Settings window closed (with optional config changes)
    Closed(Option<AppConfig>),
    /// Error occurred
    Error(String),
}

/// Settings window manager
pub struct SettingsWindow {
    tx: Sender<SettingsMessage>,
    rx: Receiver<SettingsMessage>,
    is_open: bool,
}

impl SettingsWindow {
    /// Create a new settings window manager
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            is_open: false,
        }
    }

    /// Check if settings window is currently open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Open the settings window
    pub fn open(&mut self, config: AppConfig, config_manager: &ConfigManager) -> Result<()> {
        if self.is_open {
            debug!("Settings window already open");
            return Ok(());
        }

        self.is_open = true;
        let tx = self.tx.clone();
        let is_auto_start = config_manager.is_auto_start_enabled();

        thread::spawn(move || {
            match show_settings_window(config, is_auto_start) {
                Ok(updated_config) => {
                    let _ = tx.send(SettingsMessage::Closed(updated_config));
                }
                Err(e) => {
                    let _ = tx.send(SettingsMessage::Error(e.to_string()));
                }
            }
        });

        Ok(())
    }

    /// Try to receive a message from the settings window
    pub fn try_recv(&mut self) -> Option<SettingsMessage> {
        match self.rx.try_recv() {
            Ok(msg) => {
                if matches!(msg, SettingsMessage::Closed(_) | SettingsMessage::Error(_)) {
                    self.is_open = false;
                }
                Some(msg)
            }
            Err(_) => None,
        }
    }
}

impl Default for SettingsWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Show the settings window (runs on separate thread)
fn show_settings_window(
    config: AppConfig,
    is_auto_start: bool,
) -> Result<Option<AppConfig>> {
    use native_windows_gui as nwg;

    // Initialize NWG
    nwg::init().map_err(|e| AppError::ConfigError(format!("NWG init failed: {}", e)))?;

    // Create fonts
    let mut font = nwg::Font::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(17)
        .build(&mut font)
        .map_err(|e| AppError::ConfigError(format!("Font build failed: {}", e)))?;

    nwg::Font::set_global_default(Some(font));

    // Window dimensions
    let win_width = 360;
    let win_height = 320;
    let margin = 16;
    let group_width = win_width - (margin * 2);

    // Load app icon
    let mut icon = nwg::Icon::default();
    let icon_loaded = load_window_icon(&mut icon);

    // Create window - centered, non-resizable
    let mut window = nwg::Window::default();
    let mut builder = nwg::Window::builder()
        .size((win_width, win_height))
        .center(true)
        .title("Settings")
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE);

    if icon_loaded {
        builder = builder.icon(Some(&icon));
    }

    builder.build(&mut window)
        .map_err(|e| AppError::ConfigError(format!("Window build failed: {}", e)))?;

    // === Startup Group ===
    let mut startup_frame = nwg::Frame::default();
    nwg::Frame::builder()
        .parent(&window)
        .position((margin, 8))
        .size((group_width, 50))
        .build(&mut startup_frame)
        .map_err(|e| AppError::ConfigError(format!("Frame build failed: {}", e)))?;

    let mut auto_start_check = nwg::CheckBox::default();
    nwg::CheckBox::builder()
        .text("Launch at Windows startup")
        .position((margin + 8, 22))
        .size((group_width - 20, 24))
        .parent(&window)
        .check_state(if is_auto_start {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        })
        .build(&mut auto_start_check)
        .map_err(|e| AppError::ConfigError(format!("Checkbox build failed: {}", e)))?;

    // === Notifications Group ===
    let notify_y = 68;
    let mut notify_frame = nwg::Frame::default();
    nwg::Frame::builder()
        .parent(&window)
        .position((margin, notify_y))
        .size((group_width, 120))
        .build(&mut notify_frame)
        .map_err(|e| AppError::ConfigError(format!("Frame build failed: {}", e)))?;

    let mut notify_label = nwg::Label::default();
    nwg::Label::builder()
        .text("Show notifications for:")
        .position((margin + 8, notify_y + 8))
        .size((200, 20))
        .parent(&window)
        .build(&mut notify_label)
        .map_err(|e| AppError::ConfigError(format!("Label build failed: {}", e)))?;

    let mut notify_mode_check = nwg::CheckBox::default();
    nwg::CheckBox::builder()
        .text("Audio mode changes")
        .position((margin + 20, notify_y + 34))
        .size((group_width - 40, 24))
        .parent(&window)
        .check_state(if config.notifications.notify_mode_change {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        })
        .build(&mut notify_mode_check)
        .map_err(|e| AppError::ConfigError(format!("Checkbox build failed: {}", e)))?;

    let mut notify_mic_check = nwg::CheckBox::default();
    nwg::CheckBox::builder()
        .text("Microphone usage")
        .position((margin + 20, notify_y + 60))
        .size((group_width - 40, 24))
        .parent(&window)
        .check_state(if config.notifications.notify_mic_usage {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        })
        .build(&mut notify_mic_check)
        .map_err(|e| AppError::ConfigError(format!("Checkbox build failed: {}", e)))?;

    let mut notify_errors_check = nwg::CheckBox::default();
    nwg::CheckBox::builder()
        .text("Errors and warnings")
        .position((margin + 20, notify_y + 86))
        .size((group_width - 40, 24))
        .parent(&window)
        .check_state(if config.notifications.notify_errors {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        })
        .build(&mut notify_errors_check)
        .map_err(|e| AppError::ConfigError(format!("Checkbox build failed: {}", e)))?;

    // === Updates Group ===
    let update_y = 198;
    let mut update_frame = nwg::Frame::default();
    nwg::Frame::builder()
        .parent(&window)
        .position((margin, update_y))
        .size((group_width, 50))
        .build(&mut update_frame)
        .map_err(|e| AppError::ConfigError(format!("Frame build failed: {}", e)))?;

    let mut update_check = nwg::CheckBox::default();
    nwg::CheckBox::builder()
        .text("Check for updates automatically")
        .position((margin + 8, update_y + 14))
        .size((group_width - 20, 24))
        .parent(&window)
        .check_state(if config.updates.auto_check {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        })
        .build(&mut update_check)
        .map_err(|e| AppError::ConfigError(format!("Checkbox build failed: {}", e)))?;

    // === Footer ===
    let footer_y = win_height - 50;

    // Buttons (right-aligned)
    let btn_width = 80;
    let btn_height = 28;
    let btn_spacing = 10;

    let mut cancel_button = nwg::Button::default();
    nwg::Button::builder()
        .text("Cancel")
        .position((win_width - margin - btn_width, footer_y + 8))
        .size((btn_width, btn_height))
        .parent(&window)
        .build(&mut cancel_button)
        .map_err(|e| AppError::ConfigError(format!("Button build failed: {}", e)))?;

    let mut save_button = nwg::Button::default();
    nwg::Button::builder()
        .text("Save")
        .position((win_width - margin - btn_width * 2 - btn_spacing, footer_y + 8))
        .size((btn_width, btn_height))
        .parent(&window)
        .build(&mut save_button)
        .map_err(|e| AppError::ConfigError(format!("Button build failed: {}", e)))?;

    // Event handler
    let window_handle = window.handle;
    let save_handle = save_button.handle;
    let cancel_handle = cancel_button.handle;

    let result_config: Arc<Mutex<Option<AppConfig>>> = Arc::new(Mutex::new(None));
    let result_config_clone = Arc::clone(&result_config);

    let handler = nwg::full_bind_event_handler(&window_handle, move |event, _evt_data, handle| {
        match event {
            nwg::Event::OnButtonClick => {
                if handle == save_handle {
                    let mut new_config = config.clone();
                    new_config.general.auto_start =
                        auto_start_check.check_state() == nwg::CheckBoxState::Checked;
                    new_config.notifications.notify_mode_change =
                        notify_mode_check.check_state() == nwg::CheckBoxState::Checked;
                    new_config.notifications.notify_mic_usage =
                        notify_mic_check.check_state() == nwg::CheckBoxState::Checked;
                    new_config.notifications.notify_errors =
                        notify_errors_check.check_state() == nwg::CheckBoxState::Checked;
                    new_config.updates.auto_check =
                        update_check.check_state() == nwg::CheckBoxState::Checked;

                    if let Ok(mut guard) = result_config_clone.lock() {
                        *guard = Some(new_config);
                    }

                    nwg::stop_thread_dispatch();
                } else if handle == cancel_handle {
                    nwg::stop_thread_dispatch();
                }
            }
            nwg::Event::OnWindowClose => {
                nwg::stop_thread_dispatch();
            }
            _ => {}
        }
    });

    // Show window and run event loop
    window.set_visible(true);
    nwg::dispatch_thread_events();

    // Cleanup
    nwg::unbind_event_handler(&handler);

    // Extract the saved config from shared state
    let saved_config = result_config.lock().ok().and_then(|guard| guard.clone());

    Ok(saved_config)
}

/// Load the app icon for the settings window
fn load_window_icon(icon: &mut native_windows_gui::Icon) -> bool {
    use native_windows_gui as nwg;

    // Try loading from resources directory (relative path first)
    let paths = [
        "resources/app.ico".to_string(),
        // Try from executable directory
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("resources/app.ico")))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    ];

    for path in &paths {
        if !path.is_empty() && std::path::Path::new(path).exists() {
            if nwg::Icon::builder()
                .source_file(Some(path))
                .build(icon)
                .is_ok()
            {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_window_new() {
        let window = SettingsWindow::new();
        assert!(!window.is_open());
    }
}
