//! Background monitoring thread for audio mode changes

use crate::audio::device::{AudioMode, BluetoothAudioDevice, DeviceManager};
use crate::audio::session::{CaptureSessionManager, MicUsingApp};
use crate::error::Result;
use log::{debug, error, info, warn};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Commands sent to the monitor thread
#[derive(Debug, Clone)]
pub enum MonitorCommand {
    /// Request current state
    GetState,
    /// Force refresh of device list
    RefreshDevices,
    /// Mute a specific app
    MuteApp(u32),
    /// Unmute a specific app
    UnmuteApp(u32),
    /// Mute all mic-using apps (force stereo)
    MuteAll,
    /// Shutdown the monitor
    Shutdown,
}

/// Events sent from the monitor thread
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    /// Current state update
    StateUpdate {
        mode: AudioMode,
        mic_using_apps: Vec<MicUsingApp>,
        devices: Vec<BluetoothAudioDevice>,
    },
    /// Mode changed
    ModeChanged {
        old_mode: AudioMode,
        new_mode: AudioMode,
    },
    /// Error occurred
    Error(String),
    /// Monitor is shutting down
    Shutdown,
}

/// Shared state between monitor thread and main thread
pub struct MonitorState {
    pub current_mode: AudioMode,
    pub mic_using_apps: Vec<MicUsingApp>,
    pub bluetooth_devices: Vec<BluetoothAudioDevice>,
    pub last_update: std::time::Instant,
}

impl Default for MonitorState {
    fn default() -> Self {
        Self {
            current_mode: AudioMode::Unknown,
            mic_using_apps: Vec::new(),
            bluetooth_devices: Vec::new(),
            last_update: std::time::Instant::now(),
        }
    }
}

/// Audio monitor that runs in a background thread
pub struct AudioMonitor {
    command_tx: Sender<MonitorCommand>,
    event_rx: Receiver<MonitorEvent>,
    state: Arc<Mutex<MonitorState>>,
    thread_handle: Option<JoinHandle<()>>,
}

impl AudioMonitor {
    /// Create and start a new audio monitor
    pub fn start() -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(MonitorState::default()));
        let state_clone = Arc::clone(&state);

        let thread_handle = thread::spawn(move || {
            monitor_thread(command_rx, event_tx, state_clone);
        });

        Ok(Self {
            command_tx,
            event_rx,
            state,
            thread_handle: Some(thread_handle),
        })
    }

    /// Send a command to the monitor
    pub fn send_command(&self, cmd: MonitorCommand) -> Result<()> {
        self.command_tx
            .send(cmd)
            .map_err(|e| crate::error::AppError::AudioSessionError(e.to_string()))
    }

    /// Try to receive an event (non-blocking)
    pub fn try_recv_event(&self) -> Option<MonitorEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Get the current state
    pub fn get_state(&self) -> MonitorState {
        self.state.lock().unwrap().clone()
    }

    /// Request a state update
    pub fn request_state(&self) -> Result<()> {
        self.send_command(MonitorCommand::GetState)
    }

    /// Mute a specific app
    pub fn mute_app(&self, process_id: u32) -> Result<()> {
        self.send_command(MonitorCommand::MuteApp(process_id))
    }

    /// Mute all apps (force stereo mode)
    pub fn force_stereo(&self) -> Result<()> {
        self.send_command(MonitorCommand::MuteAll)
    }

    /// Shutdown the monitor
    pub fn shutdown(&mut self) {
        let _ = self.send_command(MonitorCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Clone for MonitorState {
    fn clone(&self) -> Self {
        Self {
            current_mode: self.current_mode,
            mic_using_apps: self.mic_using_apps.clone(),
            bluetooth_devices: self.bluetooth_devices.clone(),
            last_update: self.last_update,
        }
    }
}

impl Drop for AudioMonitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The main monitor thread function
fn monitor_thread(
    command_rx: Receiver<MonitorCommand>,
    event_tx: Sender<MonitorEvent>,
    state: Arc<Mutex<MonitorState>>,
) {
    info!("Audio monitor thread started");

    // Initialize COM for this thread
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            error!("Failed to initialize COM in monitor thread: {:?}", hr);
            let _ = event_tx.send(MonitorEvent::Error(format!("COM init failed: {:?}", hr)));
            return;
        }
    }

    let poll_interval = Duration::from_millis(500);
    let mut last_mode = AudioMode::Unknown;

    loop {
        // Check for commands (non-blocking)
        match command_rx.try_recv() {
            Ok(MonitorCommand::Shutdown) => {
                info!("Monitor thread received shutdown command");
                let _ = event_tx.send(MonitorEvent::Shutdown);
                break;
            }
            Ok(MonitorCommand::MuteApp(pid)) => {
                handle_mute_app(pid, &event_tx);
            }
            Ok(MonitorCommand::MuteAll) => {
                handle_mute_all(&event_tx);
            }
            Ok(MonitorCommand::UnmuteApp(pid)) => {
                handle_unmute_app(pid, &event_tx);
            }
            Ok(MonitorCommand::GetState) | Ok(MonitorCommand::RefreshDevices) => {
                // Will be handled in the regular poll below
            }
            Err(mpsc::TryRecvError::Empty) => {
                // No command, continue polling
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                info!("Command channel disconnected, shutting down monitor");
                break;
            }
        }

        // Poll current state
        match poll_audio_state() {
            Ok((mode, mic_apps, devices)) => {
                // Update shared state
                {
                    let mut state_guard = state.lock().unwrap();
                    state_guard.current_mode = mode;
                    state_guard.mic_using_apps = mic_apps.clone();
                    state_guard.bluetooth_devices = devices.clone();
                    state_guard.last_update = std::time::Instant::now();
                }

                // Check for mode change
                if mode != last_mode && last_mode != AudioMode::Unknown {
                    info!("Audio mode changed: {:?} -> {:?}", last_mode, mode);
                    let _ = event_tx.send(MonitorEvent::ModeChanged {
                        old_mode: last_mode,
                        new_mode: mode,
                    });
                }
                last_mode = mode;

                // Send state update
                let _ = event_tx.send(MonitorEvent::StateUpdate {
                    mode,
                    mic_using_apps: mic_apps,
                    devices,
                });
            }
            Err(e) => {
                warn!("Error polling audio state: {}", e);
                let _ = event_tx.send(MonitorEvent::Error(e.to_string()));
            }
        }

        thread::sleep(poll_interval);
    }

    // Cleanup COM
    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }

    info!("Audio monitor thread stopped");
}

/// Poll the current audio state
fn poll_audio_state() -> Result<(AudioMode, Vec<MicUsingApp>, Vec<BluetoothAudioDevice>)> {
    let device_manager = DeviceManager::new()?;
    let devices = device_manager.get_bluetooth_devices()?;

    // Log detected Bluetooth devices at debug level
    for device in &devices {
        debug!(
            "BT Device: {} | Rate: {:?}Hz | Channels: {:?}",
            device.device.name,
            device.sample_rate,
            device.channels
        );
    }

    // Get mic-using apps from all capture devices (for display in menu)
    let mic_apps = get_all_mic_using_apps();

    // Log mic apps at debug level
    for app in &mic_apps {
        debug!("Mic app: {} (PID: {}) - BT mic: {}",
            app.process_name, app.process_id, app.is_using_bluetooth_mic);
    }

    // Mode detection using peak meter channel count
    // This is the most reliable method for Windows 11 unified audio endpoints:
    // - 1 channel (mono) = HFP mode (hands-free profile)
    // - 2 channels (stereo) = A2DP mode (stereo profile)
    let mode = if devices.is_empty() {
        AudioMode::Unknown
    } else {
        match device_manager.is_bluetooth_device_in_hfp_mode() {
            Ok(Some(true)) => {
                debug!("Mode: HandsFree (mono audio detected via peak meter)");
                AudioMode::HandsFree
            }
            Ok(Some(false)) => {
                AudioMode::Stereo
            }
            Ok(None) | Err(_) => {
                // Fallback: check if BT mic is in use
                let bt_mic_in_use = mic_apps.iter().any(|app| app.is_using_bluetooth_mic);
                if bt_mic_in_use {
                    debug!("Mode: HandsFree (BT microphone in use)");
                    AudioMode::HandsFree
                } else {
                    AudioMode::Stereo
                }
            }
        }
    };

    // Update all Bluetooth devices to reflect the actual detected mode
    let mut devices = devices;
    for device in &mut devices {
        device.current_mode = mode;
    }

    Ok((mode, mic_apps, devices))
}

/// Get mic-using apps from all capture devices
fn get_all_mic_using_apps() -> Vec<MicUsingApp> {
    // Check ALL capture devices, not just the default
    CaptureSessionManager::get_all_mic_using_apps()
}

/// Handle mute app command - searches ALL capture devices
fn handle_mute_app(pid: u32, event_tx: &Sender<MonitorEvent>) {
    if let Err(e) = CaptureSessionManager::mute_app_on_all_devices(pid) {
        let _ = event_tx.send(MonitorEvent::Error(format!("Failed to mute app: {}", e)));
    } else {
        info!("Muted app with PID {}", pid);
    }
}

/// Handle unmute app command - searches ALL capture devices
fn handle_unmute_app(pid: u32, event_tx: &Sender<MonitorEvent>) {
    if let Err(e) = CaptureSessionManager::unmute_app_on_all_devices(pid) {
        let _ = event_tx.send(MonitorEvent::Error(format!("Failed to unmute app: {}", e)));
    } else {
        info!("Unmuted app with PID {}", pid);
    }
}

/// Handle mute all command (force stereo)
fn handle_mute_all(event_tx: &Sender<MonitorEvent>) {
    match CaptureSessionManager::new_default() {
        Ok(session_manager) => {
            if let Err(e) = session_manager.mute_all() {
                let _ = event_tx.send(MonitorEvent::Error(format!(
                    "Failed to mute all apps: {}",
                    e
                )));
            } else {
                info!("Muted all mic-using apps to force stereo mode");
            }
        }
        Err(e) => {
            let _ = event_tx.send(MonitorEvent::Error(format!(
                "Failed to access capture device: {}",
                e
            )));
        }
    }
}
