//! Context menu building and event handling

use crate::audio::device::{AudioMode, BluetoothAudioDevice};
use crate::audio::session::HfpUsingApp;
use crate::error::Result;
use log::info;
use muda::{Menu, MenuEvent as MudaMenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::{HashMap, HashSet};

/// Menu item identifiers
pub const MENU_ID_MODE_DISPLAY: &str = "mode_display";
pub const MENU_ID_SETTINGS: &str = "settings";
pub const MENU_ID_CHECK_UPDATES: &str = "check_updates";
pub const MENU_ID_ABOUT: &str = "about";
pub const MENU_ID_EXIT: &str = "exit";
pub const MENU_PREFIX_TERMINATE_APP: &str = "terminate_app_";
pub const MENU_PREFIX_DEVICE: &str = "device_";
pub const MENU_PREFIX_FORCE_STEREO: &str = "force_stereo_";
pub const MENU_PREFIX_ALLOW_HFP: &str = "allow_hfp_";
pub const MENU_PREFIX_RECONNECT: &str = "reconnect_";

/// Events from menu interactions
#[derive(Debug, Clone)]
pub enum MenuEvent {
    /// Terminate a specific app
    TerminateApp(u32),
    /// Force stereo mode by disabling HFP
    ForceStereo(String),
    /// Allow hands-free mode by enabling HFP
    AllowHandsFree(String),
    /// Reconnect a Bluetooth device
    ReconnectDevice(String),
    /// Open settings window
    OpenSettings,
    /// Check for updates
    CheckUpdates,
    /// Show about dialog
    ShowAbout,
    /// Exit the application
    Exit,
}

/// Builds and manages the context menu
pub struct MenuBuilder {
    /// Map of menu item IDs to their purposes
    item_map: HashMap<String, MenuItemPurpose>,
}

/// Internal tracking of menu item purposes for event handling
#[derive(Debug, Clone)]
#[allow(dead_code)] // Device and Static reserved for future device-specific menus
enum MenuItemPurpose {
    TerminateApp(u32),
    ForceStereo(String),
    AllowHandsFree(String),
    ReconnectDevice(String),
    Device(String),
    Static(String),
}

impl MenuBuilder {
    /// Create a new menu builder
    pub fn new() -> Self {
        Self {
            item_map: HashMap::new(),
        }
    }

    /// Build the context menu with current state
    ///
    /// # Arguments
    /// * `mode` - Current audio mode
    /// * `hfp_apps` - Apps outputting to Bluetooth (may have triggered HFP)
    /// * `devices` - Bluetooth audio devices
    /// * `forced_stereo_devices` - Set of device names that have been forced to stereo mode
    pub fn build(
        &mut self,
        mode: AudioMode,
        hfp_apps: &[HfpUsingApp],
        devices: &[BluetoothAudioDevice],
        forced_stereo_devices: &HashSet<String>,
    ) -> Result<Menu> {
        self.item_map.clear();
        let menu = Menu::new();

        // Current mode display (disabled)
        let mode_text = rust_i18n::t!("menu_mode", mode = mode.display_localized());
        let mode_item = MenuItem::with_id(MENU_ID_MODE_DISPLAY, &mode_text, false, None);
        menu.append(&mode_item)?;

        // Bluetooth devices (shown directly in main menu)
        if !devices.is_empty() {
            menu.append(&PredefinedMenuItem::separator())?;

            for device in devices {
                // Create submenu for each device directly in main menu
                let device_text = format!("{} ({})", device.device.name, device.current_mode.display_localized());
                let device_submenu = Submenu::new(&device_text, true);

                // Check if this device has been forced to stereo
                let is_forced_stereo = forced_stereo_devices.contains(&device.device.name);

                // Add Force Stereo option (enabled when HFP is allowed)
                let force_stereo_id = format!("{}{}", MENU_PREFIX_FORCE_STEREO, &device.device.name);
                let force_stereo_item = MenuItem::with_id(&force_stereo_id, &rust_i18n::t!("menu_force_stereo"), !is_forced_stereo, None);
                device_submenu.append(&force_stereo_item)?;
                self.item_map.insert(
                    force_stereo_id,
                    MenuItemPurpose::ForceStereo(device.device.name.clone()),
                );

                // Add Allow Hands Free option (enabled when forced to stereo)
                let allow_hfp_id = format!("{}{}", MENU_PREFIX_ALLOW_HFP, &device.device.name);
                let allow_hfp_item = MenuItem::with_id(&allow_hfp_id, &rust_i18n::t!("menu_allow_hands_free"), is_forced_stereo, None);
                device_submenu.append(&allow_hfp_item)?;
                self.item_map.insert(
                    allow_hfp_id,
                    MenuItemPurpose::AllowHandsFree(device.device.name.clone()),
                );

                device_submenu.append(&PredefinedMenuItem::separator())?;

                // Add Reconnect option (full reconnect)
                let reconnect_id = format!("{}{}", MENU_PREFIX_RECONNECT, &device.device.name);
                let reconnect_item = MenuItem::with_id(&reconnect_id, &rust_i18n::t!("menu_reconnect"), true, None);
                device_submenu.append(&reconnect_item)?;
                self.item_map.insert(
                    reconnect_id,
                    MenuItemPurpose::ReconnectDevice(device.device.name.clone()),
                );

                menu.append(&device_submenu)?;
            }
        }

        // Apps using Bluetooth audio (shown regardless of mode when apps are detected)
        if !hfp_apps.is_empty() {
            menu.append(&PredefinedMenuItem::separator())?;

            // Show header with count
            let header_text = rust_i18n::t!("menu_apps_using_hfp", count = hfp_apps.len());
            let header_item = MenuItem::with_id("apps_header", &header_text, false, None);
            menu.append(&header_item)?;

            for app in hfp_apps {
                // Create submenu for each app with terminate option
                let app_submenu = Submenu::new(&app.display_name, true);

                // Process info (disabled)
                let info_text = rust_i18n::t!("menu_pid_info", pid = app.process_id, name = &app.process_name);
                let info_item = MenuItem::with_id(
                    &format!("info_{}", app.process_id),
                    &info_text,
                    false,
                    None,
                );
                app_submenu.append(&info_item)?;

                app_submenu.append(&PredefinedMenuItem::separator())?;

                // Terminate option
                let terminate_id = format!("{}{}", MENU_PREFIX_TERMINATE_APP, app.process_id);
                let terminate_item = MenuItem::with_id(&terminate_id, &rust_i18n::t!("menu_terminate_app"), true, None);
                app_submenu.append(&terminate_item)?;
                self.item_map.insert(
                    terminate_id,
                    MenuItemPurpose::TerminateApp(app.process_id),
                );

                menu.append(&app_submenu)?;
            }
        }

        menu.append(&PredefinedMenuItem::separator())?;

        // Settings
        let settings_item = MenuItem::with_id(MENU_ID_SETTINGS, &rust_i18n::t!("menu_settings"), true, None);
        menu.append(&settings_item)?;

        // Check for updates
        let updates_item = MenuItem::with_id(MENU_ID_CHECK_UPDATES, &rust_i18n::t!("menu_check_updates"), true, None);
        menu.append(&updates_item)?;

        menu.append(&PredefinedMenuItem::separator())?;

        // Exit
        let exit_item = MenuItem::with_id(MENU_ID_EXIT, &rust_i18n::t!("menu_exit"), true, None);
        menu.append(&exit_item)?;

        Ok(menu)
    }

    /// Convert a muda menu event to our MenuEvent enum
    pub fn handle_event(&self, event: &MudaMenuEvent) -> Option<MenuEvent> {
        let id = event.id().0.as_str();
        info!("Menu event received: '{}'", id);

        match id {
            MENU_ID_SETTINGS => Some(MenuEvent::OpenSettings),
            MENU_ID_CHECK_UPDATES => Some(MenuEvent::CheckUpdates),
            MENU_ID_ABOUT => Some(MenuEvent::ShowAbout),
            MENU_ID_EXIT => Some(MenuEvent::Exit),
            _ => {
                // Check for dynamic items
                if let Some(purpose) = self.item_map.get(id) {
                    match purpose {
                        MenuItemPurpose::TerminateApp(pid) => Some(MenuEvent::TerminateApp(*pid)),
                        MenuItemPurpose::ForceStereo(name) => {
                            Some(MenuEvent::ForceStereo(name.clone()))
                        }
                        MenuItemPurpose::AllowHandsFree(name) => {
                            Some(MenuEvent::AllowHandsFree(name.clone()))
                        }
                        MenuItemPurpose::ReconnectDevice(name) => {
                            Some(MenuEvent::ReconnectDevice(name.clone()))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
        }
    }
}

impl Default for MenuBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_builder_new() {
        let builder = MenuBuilder::new();
        assert!(builder.item_map.is_empty());
    }
}
