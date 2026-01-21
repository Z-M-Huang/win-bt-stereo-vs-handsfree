//! WASAPI audio session management and microphone usage detection

use crate::error::{AppError, Result};
use log::{debug, info};
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eCapture, eRender, IAudioSessionControl, IAudioSessionControl2,
    IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, AudioSessionStateActive, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};
use windows::core::GUID;

/// Information about an application using the microphone
#[derive(Debug, Clone)]
pub struct MicUsingApp {
    pub process_id: u32,
    pub process_name: String,
    pub display_name: String,
    pub icon_path: Option<String>,
    pub is_muted: bool,
    /// Whether the app is using a Bluetooth microphone
    pub is_using_bluetooth_mic: bool,
}

impl MicUsingApp {
    pub fn new(process_id: u32, process_name: String, display_name: String) -> Self {
        Self {
            process_id,
            process_name,
            display_name,
            icon_path: None,
            is_muted: false,
            is_using_bluetooth_mic: false,
        }
    }
}

/// Information about an application using HFP (outputting to BT headset in hands-free mode)
#[derive(Debug, Clone)]
pub struct HfpUsingApp {
    pub process_id: u32,
    pub process_name: String,
    pub display_name: String,
}

impl HfpUsingApp {
    pub fn new(process_id: u32, process_name: String, display_name: String) -> Self {
        Self {
            process_id,
            process_name,
            display_name,
        }
    }
}

/// Get apps with active audio sessions on Bluetooth render devices
/// These are apps outputting audio to the BT headset, which may have triggered HFP mode
pub fn get_apps_using_bluetooth_output() -> Vec<HfpUsingApp> {
    let mut apps = Vec::new();
    let mut seen_pids = std::collections::HashSet::new();

    unsafe {
        let enumerator: std::result::Result<IMMDeviceEnumerator, _> =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);

        let enumerator = match enumerator {
            Ok(e) => e,
            Err(_) => return apps,
        };

        // Enumerate all render (output) devices
        let collection = match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            Ok(c) => c,
            Err(_) => return apps,
        };

        let count = match collection.GetCount() {
            Ok(c) => c,
            Err(_) => return apps,
        };

        for i in 0..count {
            if let Ok(device) = collection.Item(i) {
                // Get device ID and name
                let device_id = device.GetId()
                    .map(|id| {
                        let s = id.to_string().unwrap_or_else(|_| "Unknown".to_string());
                        windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
                        s
                    })
                    .unwrap_or_else(|_| "Unknown".to_string());

                // Get friendly name
                let device_name = device.OpenPropertyStore(STGM_READ)
                    .ok()
                    .and_then(|props: IPropertyStore| {
                        let pkey_friendly_name = PROPERTYKEY {
                            fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
                            pid: 14,
                        };
                        props.GetValue(&pkey_friendly_name).ok().map(|v| v.to_string())
                    })
                    .unwrap_or_else(|| device_id.clone());

                // Check if this is a Bluetooth device
                let id_lower = device_id.to_lowercase();
                let name_lower = device_name.to_lowercase();
                let is_bluetooth = id_lower.contains("bluetooth")
                    || id_lower.contains("bth")
                    || id_lower.contains("{0000110b")
                    || id_lower.contains("{0000111e")
                    || name_lower.contains("bluetooth")
                    || name_lower.contains("headset")
                    || name_lower.contains("headphone")
                    || name_lower.contains("hands-free")
                    || name_lower.contains("handsfree")
                    || name_lower.contains("earbuds")
                    || name_lower.contains("airpods")
                    || name_lower.contains("buds");

                if !is_bluetooth {
                    continue;
                }

                debug!("Checking BT render device: {}", device_name);

                // Get session manager for this device
                let session_manager: std::result::Result<IAudioSessionManager2, _> =
                    device.Activate(CLSCTX_ALL, None);

                let session_manager = match session_manager {
                    Ok(sm) => sm,
                    Err(_) => continue,
                };

                // Enumerate sessions
                let session_enum = match session_manager.GetSessionEnumerator() {
                    Ok(se) => se,
                    Err(_) => continue,
                };

                let session_count = match session_enum.GetCount() {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                for j in 0..session_count {
                    if let Ok(session) = session_enum.GetSession(j) {
                        if let Ok(session2) = session.cast::<IAudioSessionControl2>() {
                            // Check if session is active
                            let state = session2.GetState().unwrap_or(windows::Win32::Media::Audio::AudioSessionStateExpired);
                            if state != AudioSessionStateActive {
                                continue;
                            }

                            let pid = session2.GetProcessId().unwrap_or(0);
                            if pid == 0 || !seen_pids.insert(pid) {
                                continue; // Skip system or duplicate
                            }

                            let display_name = session2.GetDisplayName()
                                .map(|n| {
                                    let s = n.to_string().unwrap_or_default();
                                    // Free COM-allocated string to prevent memory leak
                                    windows::Win32::System::Com::CoTaskMemFree(Some(n.0 as *const _));
                                    s
                                })
                                .unwrap_or_default();

                            let process_name = get_process_name(pid)
                                .unwrap_or_else(|| format!("PID {}", pid));

                            debug!("Found app on BT render: {} (PID {})", process_name, pid);

                            apps.push(HfpUsingApp::new(
                                pid,
                                process_name.clone(),
                                if display_name.is_empty() { process_name } else { display_name },
                            ));
                        }
                    }
                }
            }
        }
    }

    apps
}

/// Wrapper around WASAPI audio session
pub struct AudioSession {
    session_control: IAudioSessionControl2,
    volume_control: Option<ISimpleAudioVolume>,
}

impl AudioSession {
    pub fn new(session_control: IAudioSessionControl) -> Result<Self> {
        let session_control2: IAudioSessionControl2 = session_control.cast()?;
        let volume_control = session_control.cast::<ISimpleAudioVolume>().ok();

        Ok(Self {
            session_control: session_control2,
            volume_control,
        })
    }

    /// Get the process ID of the session owner
    pub fn get_process_id(&self) -> Result<u32> {
        unsafe { Ok(self.session_control.GetProcessId()?) }
    }

    /// Get the display name of the session
    pub fn get_display_name(&self) -> Result<String> {
        unsafe {
            let name = self.session_control.GetDisplayName()?;
            let result = name.to_string().unwrap_or_else(|_| String::new());
            // Free COM-allocated string to prevent memory leak
            windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
            Ok(result)
        }
    }

    /// Check if the session is currently active
    pub fn is_active(&self) -> Result<bool> {
        unsafe {
            let state = self.session_control.GetState()?;
            Ok(state == AudioSessionStateActive)
        }
    }

    /// Get the icon path for the session
    pub fn get_icon_path(&self) -> Result<Option<String>> {
        unsafe {
            let path = self.session_control.GetIconPath()?;
            let path_str = path.to_string().unwrap_or_else(|_| String::new());
            // Free COM-allocated string to prevent memory leak
            windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));
            if path_str.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path_str))
            }
        }
    }

    /// Check if the session is muted
    pub fn is_muted(&self) -> Result<bool> {
        if let Some(ref volume) = self.volume_control {
            unsafe {
                let muted = volume.GetMute()?;
                Ok(muted.as_bool())
            }
        } else {
            Ok(false)
        }
    }

    /// Set the mute state of the session
    pub fn set_muted(&self, muted: bool) -> Result<()> {
        if let Some(ref volume) = self.volume_control {
            unsafe {
                volume.SetMute(muted, std::ptr::null())?;
                Ok(())
            }
        } else {
            Err(AppError::AudioSessionError(
                "Volume control not available".to_string(),
            ))
        }
    }

    /// Get the volume level (0.0 to 1.0)
    pub fn get_volume(&self) -> Result<f32> {
        if let Some(ref volume) = self.volume_control {
            unsafe {
                let level = volume.GetMasterVolume()?;
                Ok(level)
            }
        } else {
            Ok(1.0)
        }
    }

    /// Set the volume level (0.0 to 1.0)
    pub fn set_volume(&self, level: f32) -> Result<()> {
        if let Some(ref volume) = self.volume_control {
            unsafe {
                volume.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null())?;
                Ok(())
            }
        } else {
            Err(AppError::AudioSessionError(
                "Volume control not available".to_string(),
            ))
        }
    }
}

/// Manages capture (microphone) audio sessions
pub struct CaptureSessionManager {
    #[allow(dead_code)]
    device: IMMDevice,
    session_manager: IAudioSessionManager2,
}

impl CaptureSessionManager {
    /// Create a session manager for the default capture device
    pub fn new_default() -> Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let device = enumerator.GetDefaultAudioEndpoint(eCapture, windows::Win32::Media::Audio::eConsole)?;

            // Log the default capture device for debugging
            if let Ok(id) = device.GetId() {
                let id_str = id.to_string().unwrap_or_else(|_| "Unknown".to_string());
                windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
                debug!("Default capture device ID: {}", id_str);
            }

            Self::new_for_device(device)
        }
    }

    /// Create a session manager for a specific device
    pub fn new_for_device(device: IMMDevice) -> Result<Self> {
        unsafe {
            let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;

            Ok(Self {
                device,
                session_manager,
            })
        }
    }

    /// Get mic-using apps from ALL capture devices
    pub fn get_all_mic_using_apps() -> Vec<MicUsingApp> {
        let mut all_apps = Vec::new();
        let mut seen_pids = std::collections::HashSet::new();

        unsafe {
            let enumerator: Result<IMMDeviceEnumerator> =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.into());

            let enumerator = match enumerator {
                Ok(e) => e,
                Err(_) => return all_apps,
            };

            // Enumerate all capture devices
            let collection = match enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) {
                Ok(c) => c,
                Err(_) => return all_apps,
            };

            let count = match collection.GetCount() {
                Ok(c) => c,
                Err(_) => return all_apps,
            };

            debug!("Checking {} capture devices for mic usage", count);

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    // Get device ID and name for detection
                    let device_id = device.GetId()
                        .map(|id| {
                            let s = id.to_string().unwrap_or_else(|_| "Unknown".to_string());
                            windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
                            s
                        })
                        .unwrap_or_else(|_| "Unknown".to_string());

                    // Get friendly name from property store
                    let device_name = device.OpenPropertyStore(STGM_READ)
                        .ok()
                        .and_then(|props: IPropertyStore| {
                            let pkey_friendly_name = PROPERTYKEY {
                                fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
                                pid: 14,
                            };
                            props.GetValue(&pkey_friendly_name).ok().map(|v| v.to_string())
                        })
                        .unwrap_or_else(|| device_id.clone());

                    // Check if this is a Bluetooth capture device
                    let id_lower = device_id.to_lowercase();
                    let name_lower = device_name.to_lowercase();
                    let is_bluetooth_device = id_lower.contains("bluetooth")
                        || id_lower.contains("bth")
                        || id_lower.contains("{0000110b")  // BT audio sink
                        || id_lower.contains("{0000111e")  // BT handsfree
                        || name_lower.contains("bluetooth")
                        || name_lower.contains("headset")
                        || name_lower.contains("headphone")
                        || name_lower.contains("hands-free")
                        || name_lower.contains("handsfree");

                    if let Ok(manager) = Self::new_for_device(device) {
                        if let Ok(apps) = manager.get_mic_using_apps() {
                            for mut app in apps {
                                // Avoid duplicates (same app on multiple devices)
                                // But if the same app uses both BT and non-BT mic, prefer BT flag
                                if let Some(existing) = all_apps.iter_mut().find(|a: &&mut MicUsingApp| a.process_id == app.process_id) {
                                    // Update to true if this device is BT
                                    if is_bluetooth_device {
                                        existing.is_using_bluetooth_mic = true;
                                    }
                                } else if seen_pids.insert(app.process_id) {
                                    app.is_using_bluetooth_mic = is_bluetooth_device;
                                    debug!("Found mic app on {} (BT: {}): {} (PID {})",
                                        device_name, is_bluetooth_device, app.process_name, app.process_id);
                                    all_apps.push(app);
                                }
                            }
                        }
                    }
                }
            }
        }

        if all_apps.is_empty() {
            debug!("No mic-using apps found on any capture device");
        }

        all_apps
    }

    /// Get all active capture sessions
    pub fn get_active_sessions(&self) -> Result<Vec<AudioSession>> {
        unsafe {
            let enumerator = self.session_manager.GetSessionEnumerator()?;
            let count = enumerator.GetCount()?;
            debug!("Found {} capture sessions total", count);
            let mut sessions = Vec::new();

            for i in 0..count {
                if let Ok(session_control) = enumerator.GetSession(i) {
                    if let Ok(session) = AudioSession::new(session_control) {
                        let is_active = session.is_active().unwrap_or(false);
                        let pid = session.get_process_id().unwrap_or(0);
                        let state = if is_active { "active" } else { "inactive" };
                        debug!("Session {}: PID {} - {}", i, pid, state);

                        if is_active {
                            sessions.push(session);
                        }
                    }
                }
            }

            debug!("Returning {} active capture sessions", sessions.len());
            Ok(sessions)
        }
    }

    /// Get list of apps currently using the microphone
    pub fn get_mic_using_apps(&self) -> Result<Vec<MicUsingApp>> {
        let sessions = self.get_active_sessions()?;
        let mut apps = Vec::new();

        for session in sessions {
            if let Ok(pid) = session.get_process_id() {
                if pid == 0 {
                    continue; // System session
                }

                let display_name = session.get_display_name().unwrap_or_default();
                let process_name = get_process_name(pid).unwrap_or_else(|| format!("PID {}", pid));
                let icon_path = session.get_icon_path().ok().flatten();
                let is_muted = session.is_muted().unwrap_or(false);

                apps.push(MicUsingApp {
                    process_id: pid,
                    process_name: process_name.clone(),
                    display_name: if display_name.is_empty() {
                        process_name  // Use process name instead of just PID
                    } else {
                        display_name
                    },
                    icon_path,
                    is_muted,
                    is_using_bluetooth_mic: false, // Will be set by get_all_mic_using_apps
                });
            }
        }

        Ok(apps)
    }

    /// Check if any app is using the microphone
    pub fn is_mic_in_use(&self) -> Result<bool> {
        let apps = self.get_mic_using_apps()?;
        Ok(!apps.is_empty())
    }

    /// Mute a specific app's microphone input
    pub fn mute_app(&self, process_id: u32) -> Result<()> {
        let sessions = self.get_active_sessions()?;

        for session in sessions {
            if let Ok(pid) = session.get_process_id() {
                if pid == process_id {
                    session.set_muted(true)?;
                    info!("Muted microphone for process {}", process_id);
                    return Ok(());
                }
            }
        }

        Err(AppError::AudioSessionError(format!(
            "No active session found for process {}",
            process_id
        )))
    }

    /// Unmute a specific app's microphone input
    pub fn unmute_app(&self, process_id: u32) -> Result<()> {
        let sessions = self.get_active_sessions()?;

        for session in sessions {
            if let Ok(pid) = session.get_process_id() {
                if pid == process_id {
                    session.set_muted(false)?;
                    info!("Unmuted microphone for process {}", process_id);
                    return Ok(());
                }
            }
        }

        Err(AppError::AudioSessionError(format!(
            "No active session found for process {}",
            process_id
        )))
    }

    /// Mute all apps using the microphone
    pub fn mute_all(&self) -> Result<()> {
        let sessions = self.get_active_sessions()?;

        for session in sessions {
            if let Ok(pid) = session.get_process_id() {
                if pid != 0 {
                    let _ = session.set_muted(true);
                }
            }
        }

        info!("Muted all microphone sessions");
        Ok(())
    }

    /// Mute an app on ALL capture devices (not just default)
    pub fn mute_app_on_all_devices(process_id: u32) -> Result<()> {
        use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;

        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let collection = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;

            let mut found = false;
            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(manager) = Self::new_for_device(device) {
                        // Try to mute on this device - ignore errors (app might not be on this device)
                        if manager.mute_app(process_id).is_ok() {
                            found = true;
                            info!("Muted PID {} on capture device {}", process_id, i);
                        }
                    }
                }
            }

            if found {
                Ok(())
            } else {
                Err(AppError::AudioSessionError(format!(
                    "No active session found for process {} on any capture device",
                    process_id
                )))
            }
        }
    }

    /// Unmute an app on ALL capture devices (not just default)
    pub fn unmute_app_on_all_devices(process_id: u32) -> Result<()> {
        use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;

        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let collection = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;

            let mut found = false;
            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(manager) = Self::new_for_device(device) {
                        if manager.unmute_app(process_id).is_ok() {
                            found = true;
                            info!("Unmuted PID {} on capture device {}", process_id, i);
                        }
                    }
                }
            }

            if found {
                Ok(())
            } else {
                Err(AppError::AudioSessionError(format!(
                    "No active session found for process {} on any capture device",
                    process_id
                )))
            }
        }
    }
}

/// Get the process name from a process ID
fn get_process_name(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    let name = String::from_utf16_lossy(
                        &entry.szExeFile[..entry
                            .szExeFile
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.szExeFile.len())],
                    );
                    let _ = CloseHandle(snapshot);
                    return Some(name);
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mic_using_app_new() {
        let app = MicUsingApp::new(1234, "test.exe".to_string(), "Test App".to_string());
        assert_eq!(app.process_id, 1234);
        assert_eq!(app.process_name, "test.exe");
        assert_eq!(app.display_name, "Test App");
        assert!(!app.is_muted);
        assert!(!app.is_using_bluetooth_mic);
    }
}
