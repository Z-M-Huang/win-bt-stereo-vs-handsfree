//! Context menu building and event handling

use crate::audio::device::{AudioMode, BluetoothAudioDevice};
use crate::audio::session::HfpUsingApp;
use crate::error::Result;
use log::debug;
use muda::{Menu, MenuEvent as MudaMenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;

/// Menu item identifiers
pub const MENU_ID_MODE_DISPLAY: &str = "mode_display";
pub const MENU_ID_SETTINGS: &str = "settings";
pub const MENU_ID_CHECK_UPDATES: &str = "check_updates";
pub const MENU_ID_ABOUT: &str = "about";
pub const MENU_ID_EXIT: &str = "exit";
pub const MENU_PREFIX_TERMINATE_APP: &str = "terminate_app_";
pub const MENU_PREFIX_DEVICE: &str = "device_";

/// Events from menu interactions
#[derive(Debug, Clone)]
pub enum MenuEvent {
    /// Terminate a specific app
    TerminateApp(u32),
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
    pub fn build(
        &mut self,
        mode: AudioMode,
        hfp_apps: &[HfpUsingApp],
        devices: &[BluetoothAudioDevice],
    ) -> Result<Menu> {
        self.item_map.clear();
        let menu = Menu::new();

        // Current mode display (disabled)
        let mode_text = format!("Mode: {}", mode);
        let mode_item = MenuItem::with_id(MENU_ID_MODE_DISPLAY, &mode_text, false, None);
        menu.append(&mode_item)?;

        // Separator
        menu.append(&PredefinedMenuItem::separator())?;

        // Bluetooth devices submenu (if any)
        if !devices.is_empty() {
            let devices_submenu = Submenu::new("Bluetooth Devices", true);
            for device in devices {
                let item_id = format!("{}{}", MENU_PREFIX_DEVICE, &device.device.id);
                let text = format!("{} ({})", device.device.name, device.current_mode);
                let item = MenuItem::with_id(&item_id, &text, false, None);
                devices_submenu.append(&item)?;
                self.item_map.insert(
                    item_id,
                    MenuItemPurpose::Device(device.device.id.clone()),
                );
            }
            menu.append(&devices_submenu)?;
            menu.append(&PredefinedMenuItem::separator())?;
        }

        // Apps using HFP submenu (only shown when in HFP mode and apps detected)
        if mode == AudioMode::HandsFree && !hfp_apps.is_empty() {
            let apps_submenu = Submenu::new(
                format!("Apps Using HFP ({})", hfp_apps.len()),
                true,
            );

            for app in hfp_apps {
                // Create submenu for each app with terminate option
                let app_submenu = Submenu::new(&app.display_name, true);

                // Process info (disabled)
                let info_text = format!("PID: {} - {}", app.process_id, app.process_name);
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
                let terminate_item = MenuItem::with_id(&terminate_id, "Terminate App", true, None);
                app_submenu.append(&terminate_item)?;
                self.item_map.insert(
                    terminate_id,
                    MenuItemPurpose::TerminateApp(app.process_id),
                );

                apps_submenu.append(&app_submenu)?;
            }

            menu.append(&apps_submenu)?;
        }

        menu.append(&PredefinedMenuItem::separator())?;

        // Settings
        let settings_item = MenuItem::with_id(MENU_ID_SETTINGS, "Settings...", true, None);
        menu.append(&settings_item)?;

        // Check for updates
        let updates_item = MenuItem::with_id(MENU_ID_CHECK_UPDATES, "Check for Updates", true, None);
        menu.append(&updates_item)?;

        menu.append(&PredefinedMenuItem::separator())?;

        // Exit
        let exit_item = MenuItem::with_id(MENU_ID_EXIT, "Exit", true, None);
        menu.append(&exit_item)?;

        Ok(menu)
    }

    /// Convert a muda menu event to our MenuEvent enum
    pub fn handle_event(&self, event: &MudaMenuEvent) -> Option<MenuEvent> {
        let id = event.id().0.as_str();
        debug!("Menu event: {}", id);

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
