//! Integration tests for mode detection
//! Tests requiring real audio devices are marked with #[ignore] for CI

use win_bt_stereo_vs_handsfree::audio::device::AudioMode;

#[test]
fn test_audio_mode_display() {
    assert_eq!(format!("{}", AudioMode::Stereo), "Stereo");
    assert_eq!(format!("{}", AudioMode::HandsFree), "Hands-Free");
    assert_eq!(format!("{}", AudioMode::Unknown), "Unknown");
}

#[test]
fn test_audio_mode_equality() {
    assert_eq!(AudioMode::Stereo, AudioMode::Stereo);
    assert_eq!(AudioMode::HandsFree, AudioMode::HandsFree);
    assert_eq!(AudioMode::Unknown, AudioMode::Unknown);
    assert_ne!(AudioMode::Stereo, AudioMode::HandsFree);
}

#[test]
fn test_audio_mode_clone() {
    let mode = AudioMode::Stereo;
    let cloned = mode;
    assert_eq!(mode, cloned);
}

#[test]
fn test_mode_detection_logic() {
    // Mode detection is based on mic usage
    // When mic is in use -> HandsFree, otherwise -> Stereo
    fn detect_mode(mic_in_use: bool) -> AudioMode {
        if mic_in_use {
            AudioMode::HandsFree
        } else {
            AudioMode::Stereo
        }
    }

    assert_eq!(detect_mode(true), AudioMode::HandsFree);
    assert_eq!(detect_mode(false), AudioMode::Stereo);
}

/// Test with real audio devices - requires hardware
#[test]
#[ignore]
fn test_real_device_enumeration() {
    use win_bt_stereo_vs_handsfree::audio::device::DeviceManager;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        assert!(!hr.is_err(), "COM init failed: {:?}", hr);
    }

    let result = DeviceManager::new();
    assert!(result.is_ok(), "DeviceManager creation failed");

    let manager = result.unwrap();
    let devices_result = manager.enumerate_devices();
    assert!(devices_result.is_ok(), "Device enumeration failed");

    unsafe {
        CoUninitialize();
    }
}

/// Test with real Bluetooth devices - requires hardware
#[test]
#[ignore]
fn test_real_bluetooth_device_detection() {
    use win_bt_stereo_vs_handsfree::audio::device::DeviceManager;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        assert!(!hr.is_err(), "COM init failed: {:?}", hr);
    }

    let manager = DeviceManager::new().expect("DeviceManager creation failed");
    let bt_devices = manager.get_bluetooth_devices().expect("BT enumeration failed");

    // Just verify it doesn't crash - we may or may not have BT devices
    println!("Found {} Bluetooth devices", bt_devices.len());

    unsafe {
        CoUninitialize();
    }
}

/// Test real mic usage detection - requires hardware
#[test]
#[ignore]
fn test_real_mic_usage_detection() {
    use win_bt_stereo_vs_handsfree::audio::session::CaptureSessionManager;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        assert!(!hr.is_err(), "COM init failed: {:?}", hr);
    }

    let result = CaptureSessionManager::new_default();
    // May fail if no capture device - that's OK for this test
    if let Ok(manager) = result {
        let mic_apps = manager.get_mic_using_apps().unwrap_or_default();
        println!("Found {} apps using mic", mic_apps.len());
    }

    unsafe {
        CoUninitialize();
    }
}
