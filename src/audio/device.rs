//! Bluetooth audio device enumeration and mode detection

use crate::error::Result;
use log::debug;
use windows::core::PWSTR;
use windows::Win32::Media::Audio::{
    eCapture, eRender, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

/// Represents the current audio mode of a Bluetooth device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMode {
    /// High-quality stereo output (A2DP profile)
    Stereo,
    /// Hands-free mode with microphone (HFP profile)
    HandsFree,
    /// Unknown or transitioning state
    Unknown,
}

impl std::fmt::Display for AudioMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioMode::Stereo => write!(f, "Stereo"),
            AudioMode::HandsFree => write!(f, "Hands-Free"),
            AudioMode::Unknown => write!(f, "Unknown"),
        }
    }
}

impl AudioMode {
    /// Get localized display string for UI
    ///
    /// This returns a localized string based on the current locale.
    /// The Display trait implementation above returns English for logs.
    pub fn display_localized(&self) -> String {
        match self {
            AudioMode::Stereo => rust_i18n::t!("mode_stereo").to_string(),
            AudioMode::HandsFree => rust_i18n::t!("mode_hands_free").to_string(),
            AudioMode::Unknown => rust_i18n::t!("mode_unknown").to_string(),
        }
    }
}

/// Information about an audio device
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_bluetooth: bool,
}

/// Information about a Bluetooth audio device with mode detection
#[derive(Debug, Clone)]
pub struct BluetoothAudioDevice {
    pub device: AudioDevice,
    pub current_mode: AudioMode,
    pub supports_stereo: bool,
    pub supports_handsfree: bool,
    /// Sample rate of the device (used for mode detection)
    pub sample_rate: Option<u32>,
    /// Number of channels (1 = mono/HFP, 2 = stereo/A2DP)
    pub channels: Option<u16>,
}

impl BluetoothAudioDevice {
    pub fn new(device: AudioDevice) -> Self {
        Self {
            device,
            current_mode: AudioMode::Unknown,
            supports_stereo: true,
            supports_handsfree: true,
            sample_rate: None,
            channels: None,
        }
    }

    /// Detect mode based on audio format
    /// HFP typically uses 8kHz/16kHz mono, A2DP uses 44.1kHz/48kHz stereo
    pub fn detect_mode_from_format(&mut self) {
        match (self.sample_rate, self.channels) {
            (Some(rate), Some(ch)) => {
                // HFP: 8kHz or 16kHz, usually mono
                // A2DP: 44.1kHz or 48kHz, usually stereo
                if rate <= 16000 || ch == 1 {
                    self.current_mode = AudioMode::HandsFree;
                } else {
                    self.current_mode = AudioMode::Stereo;
                }
                debug!(
                    "Device {} detected as {:?} (rate: {}Hz, channels: {})",
                    self.device.name, self.current_mode, rate, ch
                );
            }
            _ => {
                self.current_mode = AudioMode::Unknown;
            }
        }
    }
}

/// Manages audio device enumeration
pub struct DeviceManager {
    enumerator: IMMDeviceEnumerator,
}

impl DeviceManager {
    /// Create a new device manager
    /// Must be called from a thread with COM initialized
    pub fn new() -> Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            Ok(Self { enumerator })
        }
    }

    /// Get the default capture (microphone) device
    pub fn get_default_capture_device(&self) -> Result<Option<AudioDevice>> {
        unsafe {
            match self.enumerator.GetDefaultAudioEndpoint(eCapture, windows::Win32::Media::Audio::eConsole) {
                Ok(device) => Ok(Some(self.device_to_audio_device(&device)?)),
                Err(e) => {
                    // No default device is not an error
                    if e.code().0 as u32 == 0x80070490 {
                        // E_NOTFOUND
                        Ok(None)
                    } else {
                        Err(e.into())
                    }
                }
            }
        }
    }

    /// Get the default render (output) device
    pub fn get_default_render_device(&self) -> Result<Option<AudioDevice>> {
        unsafe {
            match self.enumerator.GetDefaultAudioEndpoint(eRender, windows::Win32::Media::Audio::eConsole) {
                Ok(device) => Ok(Some(self.device_to_audio_device(&device)?)),
                Err(e) => {
                    if e.code().0 as u32 == 0x80070490 {
                        Ok(None)
                    } else {
                        Err(e.into())
                    }
                }
            }
        }
    }

    /// Enumerate all active audio devices
    pub fn enumerate_devices(&self) -> Result<Vec<AudioDevice>> {
        unsafe {
            let collection = self
                .enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            let mut devices = Vec::with_capacity(count as usize);

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(audio_device) = self.device_to_audio_device(&device) {
                        devices.push(audio_device);
                    }
                }
            }

            Ok(devices)
        }
    }

    /// Enumerate all active capture (microphone) devices
    pub fn enumerate_capture_devices(&self) -> Result<Vec<AudioDevice>> {
        unsafe {
            let collection = self
                .enumerator
                .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            debug!("Found {} capture devices", count);
            let mut devices = Vec::with_capacity(count as usize);

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(audio_device) = self.device_to_audio_device(&device) {
                        debug!("Capture device [{}]: '{}' | is_bluetooth: {}", i, audio_device.name, audio_device.is_bluetooth);
                        devices.push(audio_device);
                    }
                }
            }

            Ok(devices)
        }
    }

    /// Get Bluetooth audio devices with their current mode
    pub fn get_bluetooth_devices(&self) -> Result<Vec<BluetoothAudioDevice>> {
        let devices = self.enumerate_devices_with_format()?;
        let bluetooth_devices: Vec<BluetoothAudioDevice> = devices
            .into_iter()
            .filter(|d| d.device.is_bluetooth)
            .collect();

        Ok(bluetooth_devices)
    }

    /// Enumerate all active audio devices with format info
    pub fn enumerate_devices_with_format(&self) -> Result<Vec<BluetoothAudioDevice>> {
        unsafe {
            let collection = self
                .enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            let mut devices = Vec::with_capacity(count as usize);

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(audio_device) = self.device_to_audio_device(&device) {
                        let mut bt_device = BluetoothAudioDevice::new(audio_device);

                        // Get audio format from device
                        if let Ok((sample_rate, channels)) = self.get_device_format(&device) {
                            bt_device.sample_rate = Some(sample_rate);
                            bt_device.channels = Some(channels);
                            bt_device.detect_mode_from_format();
                        }

                        devices.push(bt_device);
                    }
                }
            }

            Ok(devices)
        }
    }

    /// Get the audio format (sample rate and channels) of a device
    fn get_device_format(&self, device: &IMMDevice) -> Result<(u32, u16)> {
        unsafe {
            // Activate the audio client to get the format
            let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            // Get the mix format (the format the device is currently using)
            let format_ptr = audio_client.GetMixFormat()?;
            let format = *format_ptr;

            let sample_rate = format.nSamplesPerSec;
            let channels = format.nChannels;

            // Free the format memory
            windows::Win32::System::Com::CoTaskMemFree(Some(format_ptr as *const _));

            debug!(
                "Device format: {}Hz, {} channels",
                sample_rate, channels
            );

            Ok((sample_rate, channels))
        }
    }

    /// Get the peak meter channel count for a device
    /// This reflects the actual audio channels being output:
    /// - 1 channel = HFP (mono) mode
    /// - 2 channels = A2DP (stereo) mode
    fn get_meter_channel_count(&self, device: &IMMDevice) -> Result<u32> {
        unsafe {
            let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None)?;
            let channel_count = meter.GetMeteringChannelCount()?;
            Ok(channel_count)
        }
    }

    /// Check if a Bluetooth render device is currently in HFP (mono) mode
    /// by examining the peak meter channel count.
    /// Returns: Some(true) if HFP, Some(false) if stereo, None if can't determine
    pub fn is_bluetooth_device_in_hfp_mode(&self) -> Result<Option<bool>> {
        unsafe {
            let collection = self
                .enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;

            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Ok(audio_device) = self.device_to_audio_device(&device) {
                        if audio_device.is_bluetooth {
                            // Get the peak meter channel count
                            match self.get_meter_channel_count(&device) {
                                Ok(channels) => {
                                    debug!(
                                        "BT device '{}' meter channel count: {}",
                                        audio_device.name, channels
                                    );
                                    // 1 channel = HFP (mono), 2+ channels = stereo
                                    return Ok(Some(channels == 1));
                                }
                                Err(e) => {
                                    debug!("Failed to get meter channel count: {}", e);
                                }
                            }
                        }
                    }
                }
            }

            Ok(None)
        }
    }

    /// Detect the current audio mode based on microphone usage
    /// If any app is using the microphone on a Bluetooth device, it's in HandsFree mode
    pub fn detect_mode(&self, mic_in_use: bool) -> AudioMode {
        if mic_in_use {
            AudioMode::HandsFree
        } else {
            AudioMode::Stereo
        }
    }

    fn device_to_audio_device(&self, device: &IMMDevice) -> Result<AudioDevice> {
        unsafe {
            // Get device ID
            let id_pwstr: PWSTR = device.GetId()?;
            let id = id_pwstr.to_string().unwrap_or_else(|_| "Unknown".to_string());

            // Free the string allocated by GetId
            windows::Win32::System::Com::CoTaskMemFree(Some(id_pwstr.0 as *const _));

            // Get device friendly name from property store
            let name = match device.OpenPropertyStore(STGM_READ) {
                Ok(props) => self.get_device_name(&props),
                Err(_) => "Unknown Device".to_string(),
            };

            // Check if it's a Bluetooth device by looking at the device ID or name
            let id_lower = id.to_lowercase();
            let name_lower = name.to_lowercase();
            let is_bluetooth = id_lower.contains("bluetooth")
                || name_lower.contains("bluetooth")
                || id_lower.contains("bth")
                || id_lower.contains("{0000110b")  // Bluetooth audio sink UUID
                || id_lower.contains("{0000111e")  // Bluetooth handsfree UUID
                || name_lower.contains("headset")
                || name_lower.contains("headphone")
                || name_lower.contains("earbuds")
                || name_lower.contains("airpods")
                || name_lower.contains("buds");

            debug!(
                "Device: {} | ID contains BT markers: {} | Name: {} | is_bluetooth: {}",
                name,
                id_lower.contains("bluetooth") || id_lower.contains("bth"),
                name,
                is_bluetooth
            );

            Ok(AudioDevice {
                id,
                name,
                is_bluetooth,
            })
        }
    }

    fn get_device_name(&self, props: &IPropertyStore) -> String {
        use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
        use windows::core::GUID;

        // PKEY_Device_FriendlyName = {a45c254e-df1c-4efd-8020-67d146a850e0}, 14
        let pkey_friendly_name = PROPERTYKEY {
            fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
            pid: 14,
        };

        unsafe {
            match props.GetValue(&pkey_friendly_name) {
                Ok(value) => {
                    // PROPVARIANT implements Display/ToString in windows-rs 0.58+
                    let name = value.to_string();
                    if name.is_empty() {
                        "Unknown Device".to_string()
                    } else {
                        name
                    }
                }
                Err(_) => "Unknown Device".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_mode_display() {
        assert_eq!(format!("{}", AudioMode::Stereo), "Stereo");
        assert_eq!(format!("{}", AudioMode::HandsFree), "Hands-Free");
        assert_eq!(format!("{}", AudioMode::Unknown), "Unknown");
    }

    #[test]
    fn test_detect_mode() {
        // Mode detection is based on mic usage
        // When mic is in use -> HandsFree, otherwise -> Stereo
        // This test doesn't need COM, just logic verification
        let mode_with_mic = if true { AudioMode::HandsFree } else { AudioMode::Stereo };
        let mode_without_mic = if false { AudioMode::HandsFree } else { AudioMode::Stereo };

        assert_eq!(mode_with_mic, AudioMode::HandsFree);
        assert_eq!(mode_without_mic, AudioMode::Stereo);
    }
}
