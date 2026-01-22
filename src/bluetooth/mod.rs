//! Bluetooth device control module
//!
//! Provides functionality to enumerate and control Bluetooth devices using Win32 APIs.

pub mod control;

pub use control::{disable_hfp_by_name, enable_hfp_by_name, reconnect_by_name};
