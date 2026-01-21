//! System tray icon management

use crate::audio::device::AudioMode;
use crate::error::{AppError, Result};
use image::GenericImageView;
use log::{debug, info, warn};
use muda::Menu;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Icon states for different audio modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Stereo,
    HandsFree,
    Unknown,
}

impl From<AudioMode> for IconState {
    fn from(mode: AudioMode) -> Self {
        match mode {
            AudioMode::Stereo => IconState::Stereo,
            AudioMode::HandsFree => IconState::HandsFree,
            AudioMode::Unknown => IconState::Unknown,
        }
    }
}

/// Manages the system tray icon
pub struct TrayIconManager {
    tray_icon: TrayIcon,
    current_state: IconState,
}

impl TrayIconManager {
    /// Create a new tray icon manager
    pub fn new(menu: Menu) -> Result<Self> {
        let icon = Self::load_icon(IconState::Unknown)?;

        let tray_icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Bluetooth Audio Mode Manager")
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| AppError::TrayIconFailed(e.to_string()))?;

        info!("Tray icon created successfully");

        Ok(Self {
            tray_icon,
            current_state: IconState::Unknown,
        })
    }

    /// Load the appropriate icon for the given state
    fn load_icon(state: IconState) -> Result<Icon> {
        // Try to load from ICO file first
        let icon_path = match state {
            IconState::Stereo => "resources/tray_stereo.ico",
            IconState::HandsFree => "resources/tray_handsfree.ico",
            IconState::Unknown => "resources/tray_unknown.ico",
        };

        // Try loading from file, fall back to generated icon
        match Self::load_icon_from_file(icon_path) {
            Ok(icon) => {
                debug!("Loaded tray icon from {}", icon_path);
                Ok(icon)
            }
            Err(e) => {
                warn!("Failed to load icon from {}: {}, using fallback", icon_path, e);
                Self::generate_fallback_icon(state)
            }
        }
    }

    /// Load icon from a PNG file
    fn load_icon_from_file(path: &str) -> Result<Icon> {
        // Try relative path first, then try from executable directory
        let img = image::open(path)
            .or_else(|_| {
                // Try from executable directory
                if let Ok(exe_path) = std::env::current_exe() {
                    if let Some(exe_dir) = exe_path.parent() {
                        let full_path = exe_dir.join(path);
                        return image::open(&full_path);
                    }
                }
                Err(image::ImageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Icon file not found",
                )))
            })
            .map_err(|e| AppError::TrayIconFailed(format!("Failed to load image: {}", e)))?;

        let (width, height) = img.dimensions();
        let rgba = img.into_rgba8().into_raw();

        Icon::from_rgba(rgba, width, height)
            .map_err(|e| AppError::TrayIconFailed(format!("Failed to create icon: {}", e)))
    }

    /// Generate a simple fallback icon if PNG loading fails
    fn generate_fallback_icon(state: IconState) -> Result<Icon> {
        let (r, g, b) = match state {
            IconState::Stereo => (0, 200, 0),      // Green for stereo
            IconState::HandsFree => (255, 165, 0), // Orange for hands-free
            IconState::Unknown => (128, 128, 128), // Gray for unknown
        };

        // Create a simple 32x32 RGBA icon
        let size = 32;
        let mut rgba = vec![0u8; size * size * 4];

        // Draw a filled circle
        let center = size as f32 / 2.0;
        let radius = size as f32 / 2.0 - 2.0;

        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - center;
                let dy = y as f32 - center;
                let dist = (dx * dx + dy * dy).sqrt();

                let idx = (y * size + x) * 4;
                if dist <= radius {
                    rgba[idx] = r;
                    rgba[idx + 1] = g;
                    rgba[idx + 2] = b;
                    rgba[idx + 3] = 255;
                } else {
                    rgba[idx + 3] = 0; // Transparent
                }
            }
        }

        Icon::from_rgba(rgba, size as u32, size as u32)
            .map_err(|e| AppError::TrayIconFailed(format!("Failed to create icon: {}", e)))
    }

    /// Update the tray icon for a new audio mode
    pub fn update_mode(&mut self, mode: AudioMode) -> Result<()> {
        let new_state = IconState::from(mode);

        if new_state != self.current_state {
            let icon = Self::load_icon(new_state)?;
            self.tray_icon
                .set_icon(Some(icon))
                .map_err(|e| AppError::TrayIconFailed(e.to_string()))?;

            let tooltip = match new_state {
                IconState::Stereo => "Bluetooth Audio: Stereo Mode",
                IconState::HandsFree => "Bluetooth Audio: Hands-Free Mode",
                IconState::Unknown => "Bluetooth Audio Mode Manager",
            };

            self.tray_icon
                .set_tooltip(Some(tooltip))
                .map_err(|e| AppError::TrayIconFailed(e.to_string()))?;

            self.current_state = new_state;
            debug!("Tray icon updated to {:?}", new_state);
        }

        Ok(())
    }

    /// Update the context menu
    pub fn update_menu(&mut self, menu: Menu) -> Result<()> {
        self.tray_icon.set_menu(Some(Box::new(menu)));
        Ok(())
    }

    /// Get the current icon state
    pub fn current_state(&self) -> IconState {
        self.current_state
    }
}
