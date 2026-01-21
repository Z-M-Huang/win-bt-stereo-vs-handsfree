//! Bluetooth Audio Mode Manager Library
//!
//! A Windows application for managing Bluetooth audio device modes (stereo vs hands-free).

pub mod audio;
pub mod error;
pub mod logging;
pub mod notifications;
pub mod process;
pub mod settings;
pub mod tray;
pub mod update;

pub use error::{AppError, ErrorSeverity, Result};
