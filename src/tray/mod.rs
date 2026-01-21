//! System tray integration module

pub mod icon;
pub mod menu;

pub use icon::TrayIconManager;
pub use menu::{MenuBuilder, MenuEvent};
