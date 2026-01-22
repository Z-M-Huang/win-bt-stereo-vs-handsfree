//! Bluetooth device control functions
//!
//! Provides Win32 API-based control of Bluetooth audio devices, including
//! device enumeration and service reconnection.

use crate::error::{AppError, Result};
use log::{debug, info, warn};
use std::mem;
use std::thread;
use std::time::Duration;
use windows::core::GUID;
use windows::Win32::Devices::Bluetooth::{
    BluetoothEnumerateInstalledServices, BluetoothFindDeviceClose, BluetoothFindFirstDevice,
    BluetoothFindNextDevice, BluetoothSetServiceState, BLUETOOTH_DEVICE_INFO,
    BLUETOOTH_DEVICE_SEARCH_PARAMS, HBLUETOOTH_DEVICE_FIND,
};
use windows::Win32::Foundation::{BOOL, ERROR_NOT_FOUND, ERROR_SERVICE_DOES_NOT_EXIST, HANDLE};

/// Delay in milliseconds between disabling and re-enabling services
const RECONNECT_DELAY_MS: u64 = 1000;

/// Maximum number of services a device can have
const MAX_SERVICES: usize = 64;

/// Hands-Free Profile (HFP) service GUID
/// Standard Bluetooth SIG UUID: 0x111E
const HFP_SERVICE_GUID: GUID = GUID::from_u128(0x0000111E_0000_1000_8000_00805F9B34FB);

/// Reconnect a Bluetooth device by name
///
/// This is the main public API that finds a device by name and reconnects it
/// by disabling and re-enabling its Bluetooth services.
///
/// # Arguments
/// * `name` - The friendly name of the device to reconnect
///
/// # Returns
/// * `Ok(())` if reconnection succeeded
/// * `Err(AppError)` with user-friendly error message if failed
///
/// # Example
/// ```no_run
/// use win_bt_stereo_vs_handsfree::bluetooth::reconnect_by_name;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// reconnect_by_name("Sony WH-1000XM4")?;
/// # Ok(())
/// # }
/// ```
pub fn reconnect_by_name(name: &str) -> Result<()> {
    info!("Reconnecting Bluetooth device: {}", name);

    // Find the device
    let device_info = find_bluetooth_device_by_name(name)?;

    // Get installed services
    let services = get_device_services(&device_info)?;

    if services.is_empty() {
        warn!("No services found for device: {}", name);
        return Err(AppError::ConfigError(
            "Device has no Bluetooth services configured".to_string(),
        ));
    }

    // Reconnect the device
    reconnect_device(&device_info, &services)?;

    info!("Successfully reconnected device: {}", name);
    Ok(())
}

/// Disable HFP (Hands-Free Profile) for a Bluetooth device to force stereo mode
///
/// This disables only the HFP service, keeping A2DP (stereo audio) connected.
/// This is faster than a full reconnect and forces the device into stereo mode.
///
/// # Arguments
/// * `name` - The friendly name of the device
///
/// # Returns
/// * `Ok(())` if HFP was disabled successfully
/// * `Err(AppError)` if the operation failed
pub fn disable_hfp_by_name(name: &str) -> Result<()> {
    info!("Disabling HFP for device: {}", name);

    let device_info = find_bluetooth_device_by_name(name)?;

    // Check if device has HFP service installed
    let services = get_device_services(&device_info)?;
    let has_hfp = services.iter().any(|s| *s == HFP_SERVICE_GUID);

    if !has_hfp {
        warn!("Device '{}' does not have HFP service installed", name);
        return Err(AppError::ConfigError(
            "Device does not support Hands-Free Profile".to_string(),
        ));
    }

    // Disable HFP service
    disable_service(&device_info, &HFP_SERVICE_GUID)?;

    info!("HFP disabled for '{}' - device should switch to stereo mode", name);
    Ok(())
}

/// Enable HFP (Hands-Free Profile) for a Bluetooth device to allow hands-free mode
///
/// This re-enables the HFP service after it was disabled by `disable_hfp_by_name`.
///
/// # Arguments
/// * `name` - The friendly name of the device
///
/// # Returns
/// * `Ok(())` if HFP was enabled successfully
/// * `Err(AppError)` if the operation failed
pub fn enable_hfp_by_name(name: &str) -> Result<()> {
    info!("Enabling HFP for device: {}", name);

    let device_info = find_bluetooth_device_by_name(name)?;

    // Enable HFP service
    enable_service(&device_info, &HFP_SERVICE_GUID)?;

    info!("HFP enabled for '{}' - hands-free mode now available", name);
    Ok(())
}

/// Find a Bluetooth device by its friendly name
///
/// Enumerates paired Bluetooth devices and finds one matching the given name.
/// Uses case-insensitive matching with tie-breaking logic.
///
/// # Arguments
/// * `name` - The friendly name to search for
///
/// # Returns
/// * `Ok(BLUETOOTH_DEVICE_INFO)` if device found
/// * `Err(AppError)` if not found or enumeration failed
fn find_bluetooth_device_by_name(name: &str) -> Result<BLUETOOTH_DEVICE_INFO> {
    unsafe {
        let mut search_params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: BOOL(1),
            fReturnRemembered: BOOL(1),
            fReturnUnknown: BOOL(0),
            fReturnConnected: BOOL(1),
            fIssueInquiry: BOOL(0),
            cTimeoutMultiplier: 1,
            hRadio: HANDLE::default(),
        };

        let mut device_info = BLUETOOTH_DEVICE_INFO {
            dwSize: mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };

        // Start device enumeration
        let h_find = BluetoothFindFirstDevice(&mut search_params, &mut device_info)
            .map_err(|_| AppError::ConfigError("No Bluetooth devices found".to_string()))?;

        if h_find.is_invalid() {
            return Err(AppError::ConfigError("No Bluetooth devices found".to_string()));
        }

        // Ensure handle is closed even on error
        let result = find_matching_device(h_find, name, device_info);

        // Close the find handle
        let _ = BluetoothFindDeviceClose(h_find);

        result
    }
}

/// Helper function to find matching device from enumeration
///
/// Implements device name matching logic with tie-breaker
fn find_matching_device(
    h_find: HBLUETOOTH_DEVICE_FIND,
    target_name: &str,
    first_device: BLUETOOTH_DEVICE_INFO,
) -> Result<BLUETOOTH_DEVICE_INFO> {
    let target_normalized = normalize_name(target_name);
    let mut best_match: Option<(BLUETOOTH_DEVICE_INFO, MatchQuality)> = None;

    // Check first device
    let device_name = device_name_from_info(&first_device);
    let match_quality = check_name_match(&target_normalized, &device_name);

    if match_quality != MatchQuality::NoMatch {
        debug!("Device match: '{}' (quality: {:?})", device_name, match_quality);
        best_match = Some((first_device, match_quality));

        // If exact match, return immediately
        if match_quality == MatchQuality::Exact {
            return Ok(first_device);
        }
    }

    // Continue checking remaining devices
    unsafe {
        loop {
            let mut device_info = BLUETOOTH_DEVICE_INFO {
                dwSize: mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
                ..Default::default()
            };

            if BluetoothFindNextDevice(h_find, &mut device_info).is_err() {
                break;
            }

            let device_name = device_name_from_info(&device_info);
            let match_quality = check_name_match(&target_normalized, &device_name);

            if match_quality != MatchQuality::NoMatch {
                debug!("Device match: '{}' (quality: {:?})", device_name, match_quality);

                // Prefer exact match over contains
                if match_quality == MatchQuality::Exact {
                    return Ok(device_info);
                }

                // Update best match if this is better
                if best_match.is_none() || match_quality > best_match.as_ref().unwrap().1 {
                    best_match = Some((device_info, match_quality));
                }
            }
        }
    }

    // Return best match if found
    if let Some((device_info, quality)) = best_match {
        let device_name = device_name_from_info(&device_info);

        if quality == MatchQuality::Contains {
            warn!(
                "Using fuzzy match for '{}' -> '{}' (prefer exact match)",
                target_name, device_name
            );
        }

        Ok(device_info)
    } else {
        Err(AppError::ConfigError(format!(
            "Bluetooth device '{}' not found",
            target_name
        )))
    }
}

/// Match quality for device name matching
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchQuality {
    NoMatch = 0,
    Contains = 1,
    Exact = 2,
}

/// Normalize a device name for comparison
fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Extract device name from BLUETOOTH_DEVICE_INFO
fn device_name_from_info(info: &BLUETOOTH_DEVICE_INFO) -> String {
    let name_u16: Vec<u16> = info
        .szName
        .iter()
        .take_while(|&&c| c != 0)
        .copied()
        .collect();

    String::from_utf16_lossy(&name_u16)
}

/// Check if target name matches device name
fn check_name_match(target_normalized: &str, device_name: &str) -> MatchQuality {
    let device_normalized = normalize_name(device_name);

    // Exact match (case-insensitive)
    if target_normalized == device_normalized {
        return MatchQuality::Exact;
    }

    // Contains match (either direction)
    if target_normalized.contains(&device_normalized)
        || device_normalized.contains(target_normalized) {
        return MatchQuality::Contains;
    }

    MatchQuality::NoMatch
}

/// Get installed Bluetooth services for a device
///
/// # Arguments
/// * `device` - The device to query
///
/// # Returns
/// * `Ok(Vec<GUID>)` - List of installed service GUIDs
/// * `Err(AppError)` if enumeration failed
fn get_device_services(device: &BLUETOOTH_DEVICE_INFO) -> Result<Vec<GUID>> {
    unsafe {
        let mut service_count: u32 = MAX_SERVICES as u32;
        let mut services: Vec<GUID> = vec![GUID::zeroed(); MAX_SERVICES];

        let result = BluetoothEnumerateInstalledServices(
            HANDLE::default(),
            device,
            &mut service_count,
            Some(services.as_mut_ptr()),
        );

        if result != 0 {
            return Err(AppError::ConfigError(
                "Could not enumerate device services".to_string(),
            ));
        }

        // Truncate to actual count
        services.truncate(service_count as usize);

        debug!("Found {} services for device", service_count);
        Ok(services)
    }
}

/// Reconnect a device by disabling and re-enabling its services
///
/// Implements partial failure recovery: if re-enable fails for some services,
/// retries individually before giving up.
///
/// # Arguments
/// * `device` - The device to reconnect
/// * `services` - List of service GUIDs to reconnect
///
/// # Returns
/// * `Ok(())` if all services reconnected successfully
/// * `Err(AppError)` if reconnection failed
fn reconnect_device(device: &BLUETOOTH_DEVICE_INFO, services: &[GUID]) -> Result<()> {
    let device_name = device_name_from_info(device);

    // Disable all services
    info!("Disabling {} services for '{}'", services.len(), device_name);
    for (i, service) in services.iter().enumerate() {
        match disable_service(device, service) {
            Ok(_) => debug!("Disabled service {}/{}", i + 1, services.len()),
            Err(e) => {
                warn!("Failed to disable service {}: {}", i + 1, e);
                // Continue trying other services
            }
        }
    }

    // Wait for Windows to release services
    thread::sleep(Duration::from_millis(RECONNECT_DELAY_MS));

    // Re-enable all services
    info!("Re-enabling {} services for '{}'", services.len(), device_name);
    let mut failed_services = Vec::new();

    for (i, service) in services.iter().enumerate() {
        match enable_service(device, service) {
            Ok(_) => debug!("Enabled service {}/{}", i + 1, services.len()),
            Err(e) => {
                warn!("Failed to enable service {}: {}", i + 1, e);
                failed_services.push((i, *service, e));
            }
        }
    }

    // Retry failed services
    if !failed_services.is_empty() {
        warn!("Retrying {} failed services", failed_services.len());
        thread::sleep(Duration::from_millis(500));

        let mut still_failed = Vec::new();
        for (i, service, _) in failed_services {
            if let Err(e) = enable_service(device, &service) {
                still_failed.push((i, e));
            } else {
                debug!("Retry succeeded for service {}", i + 1);
            }
        }

        // If any services still failed, return error
        if !still_failed.is_empty() {
            let error_msg = format!(
                "Failed to reconnect {} of {} services. Try reconnecting manually via Windows Bluetooth settings.",
                still_failed.len(),
                services.len()
            );
            return Err(AppError::ConfigError(error_msg));
        }
    }

    Ok(())
}

/// Disable a Bluetooth service
fn disable_service(device: &BLUETOOTH_DEVICE_INFO, service: &GUID) -> Result<()> {
    unsafe {
        let result = BluetoothSetServiceState(
            HANDLE::default(),
            device,
            service,
            0, // 0 = disable
        );

        if result != 0 {
            return Err(map_win32_error(result, "disable"));
        }

        Ok(())
    }
}

/// Enable a Bluetooth service
fn enable_service(device: &BLUETOOTH_DEVICE_INFO, service: &GUID) -> Result<()> {
    unsafe {
        let result = BluetoothSetServiceState(
            HANDLE::default(),
            device,
            service,
            1, // 1 = enable
        );

        if result != 0 {
            return Err(map_win32_error(result, "enable"));
        }

        Ok(())
    }
}

/// Map Win32 error codes to user-friendly messages
fn map_win32_error(error_code: u32, operation: &str) -> AppError {
    let message = match error_code {
        x if x == ERROR_NOT_FOUND.0 => "Device not found".to_string(),
        x if x == ERROR_SERVICE_DOES_NOT_EXIST.0 => "Service not available".to_string(),
        _ => format!("Bluetooth operation failed ({} - code: {})", operation, error_code),
    };

    AppError::ConfigError(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("  Sony WH-1000XM4  "), "sony wh-1000xm4");
        assert_eq!(normalize_name("HEADPHONES"), "headphones");
    }

    #[test]
    fn test_check_name_match_exact() {
        let target = "sony wh-1000xm4";
        let device = "Sony WH-1000XM4";
        assert_eq!(check_name_match(target, device), MatchQuality::Exact);
    }

    #[test]
    fn test_check_name_match_contains() {
        let target = "sony";
        let device = "Sony WH-1000XM4";
        assert_eq!(check_name_match(target, device), MatchQuality::Contains);
    }

    #[test]
    fn test_check_name_match_no_match() {
        let target = "bose";
        let device = "Sony WH-1000XM4";
        assert_eq!(check_name_match(target, device), MatchQuality::NoMatch);
    }

    #[test]
    fn test_match_quality_ordering() {
        assert!(MatchQuality::Exact > MatchQuality::Contains);
        assert!(MatchQuality::Contains > MatchQuality::NoMatch);
    }
}
