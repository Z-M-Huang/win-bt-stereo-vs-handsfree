//! Bluetooth Audio Mode Manager Library
//!
//! A Windows application for managing Bluetooth audio device modes (stereo vs hands-free).

// Initialize i18n with locales directory and English fallback
rust_i18n::i18n!("locales", fallback = "en");

pub mod audio;
pub mod bluetooth;
pub mod error;
pub mod i18n;
pub mod logging;
pub mod notifications;
pub mod process;
pub mod settings;
pub mod tray;
pub mod update;

pub use error::{AppError, ErrorSeverity, Result};
