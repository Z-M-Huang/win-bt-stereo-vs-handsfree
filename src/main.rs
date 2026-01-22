//! Bluetooth Audio Mode Manager - Main Entry Point
//!
//! A Windows system tray application for managing Bluetooth audio device modes.

#![windows_subsystem = "windows"]

// Initialize i18n for the binary (shares locales with library)
rust_i18n::i18n!("locales", fallback = "en");

use win_bt_stereo_vs_handsfree::audio::{AudioMode, AudioMonitor, MonitorEvent, get_apps_using_bluetooth_output};
use win_bt_stereo_vs_handsfree::bluetooth;
use win_bt_stereo_vs_handsfree::error::{AppError, ErrorSeverity, Result};
use win_bt_stereo_vs_handsfree::logging::{init_logging, parse_log_level, LoggingConfig};
use win_bt_stereo_vs_handsfree::notifications::{register_aumid, NotificationManager, NotificationType};
use win_bt_stereo_vs_handsfree::process::ProcessManager;
use win_bt_stereo_vs_handsfree::settings::{AppConfig, ConfigManager};
use win_bt_stereo_vs_handsfree::tray::{MenuBuilder, MenuEvent, TrayIconManager};
use win_bt_stereo_vs_handsfree::update::UpdateChecker;
use log::{error, info, warn};
use muda::MenuEvent as MudaMenuEvent;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, BOOL, HANDLE, HWND};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_C_EVENT, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MessageBoxW, PeekMessageW, TranslateMessage, MB_ICONERROR,
    MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MSG, PM_REMOVE,
};

/// Named mutex for single-instance enforcement
const SINGLE_INSTANCE_MUTEX: &str = "Global\\BtAudioModeManager_SingleInstance";

/// Global shutdown flag for Ctrl+C handling
static SHUTDOWN_FLAG: AtomicBool = AtomicBool::new(false);

/// Console control handler for Ctrl+C, Ctrl+Break, and close events
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        x if x == CTRL_C_EVENT || x == CTRL_BREAK_EVENT || x == CTRL_CLOSE_EVENT => {
            info!("Received shutdown signal (type: {})", ctrl_type);
            SHUTDOWN_FLAG.store(true, Ordering::SeqCst);
            BOOL::from(true) // Signal handled
        }
        _ => BOOL::from(false), // Pass to next handler
    }
}

/// RAII guard to ensure device is removed from reconnecting set even on panic
struct ReconnectGuard {
    device_name: String,
    reconnecting_devices: Arc<Mutex<HashSet<String>>>,
}

impl ReconnectGuard {
    fn new(device_name: &str, reconnecting_devices: Arc<Mutex<HashSet<String>>>) -> Self {
        Self {
            device_name: device_name.to_string(),
            reconnecting_devices,
        }
    }
}

impl Drop for ReconnectGuard {
    fn drop(&mut self) {
        // Remove device from reconnecting set
        if let Ok(mut reconnecting) = self.reconnecting_devices.lock() {
            reconnecting.remove(&self.device_name);
        }
    }
}

/// Main application state
struct App {
    config_manager: ConfigManager,
    config: AppConfig,
    audio_monitor: Option<AudioMonitor>,
    process_manager: ProcessManager,
    tray_manager: Option<TrayIconManager>,
    menu_builder: MenuBuilder,
    notification_manager: NotificationManager,
    update_checker: UpdateChecker,
    settings_window: win_bt_stereo_vs_handsfree::settings::SettingsWindow,
    mic_apps: Arc<Mutex<Vec<win_bt_stereo_vs_handsfree::audio::MicUsingApp>>>,
    reconnecting_devices: Arc<Mutex<HashSet<String>>>,
    /// Devices that have been forced to stereo mode (HFP disabled)
    forced_stereo_devices: HashSet<String>,
    running: bool,
    last_update_check: Instant,
}

impl App {
    /// Create a new application instance
    fn new() -> Result<Self> {
        let config_manager = ConfigManager::new()?;
        let config = config_manager.load()?;

        let mic_apps = Arc::new(Mutex::new(Vec::new()));
        let process_manager = ProcessManager::new(Arc::clone(&mic_apps));

        let notification_manager = NotificationManager::new();
        let update_checker = UpdateChecker::default();

        Ok(Self {
            config_manager,
            config,
            audio_monitor: None,
            process_manager,
            tray_manager: None,
            menu_builder: MenuBuilder::new(),
            notification_manager,
            update_checker,
            settings_window: win_bt_stereo_vs_handsfree::settings::SettingsWindow::new(),
            mic_apps,
            reconnecting_devices: Arc::new(Mutex::new(HashSet::new())),
            forced_stereo_devices: HashSet::new(),
            running: true,
            last_update_check: Instant::now(),
        })
    }

    /// Initialize the application
    fn init(&mut self) -> Result<()> {
        // Update notification settings from config
        self.notification_manager.update_settings(
            self.config.notifications.notify_mode_change,
            self.config.notifications.notify_mic_usage,
            self.config.notifications.notify_errors,
            self.config.notifications.notify_updates,
        );

        // Build initial menu
        let menu = self.menu_builder.build(
            AudioMode::Unknown,
            &[],
            &[],
            &self.forced_stereo_devices,
        )?;

        // Create tray icon
        self.tray_manager = Some(TrayIconManager::new(menu)?);

        // Start audio monitor
        self.audio_monitor = Some(AudioMonitor::start()?);

        info!("Application initialized successfully");
        Ok(())
    }

    /// Process events from the audio monitor
    fn process_audio_events(&mut self) -> Result<()> {
        if let Some(ref monitor) = self.audio_monitor {
            while let Some(event) = monitor.try_recv_event() {
                match event {
                    MonitorEvent::StateUpdate { mode, mic_using_apps, devices } => {
                        // Acquire the TOCTOU operation lock before updating mic apps
                        // This ensures atomicity with process termination validation
                        let operation_lock = self.process_manager.get_operation_lock();
                        let _guard = operation_lock.lock().unwrap_or_else(|e| e.into_inner());

                        // Update shared mic apps list (while holding operation lock)
                        // Keep mic_apps for process termination validation
                        if let Ok(mut apps) = self.mic_apps.lock() {
                            *apps = mic_using_apps.clone();
                        }

                        // Release operation lock before UI updates (drop _guard)
                        drop(_guard);

                        // Get apps using Bluetooth output (these are the HFP-causing apps)
                        let hfp_apps = get_apps_using_bluetooth_output();

                        // Update tray icon
                        if let Some(ref mut tray) = self.tray_manager {
                            tray.update_mode(mode)?;

                            // Rebuild menu with HFP apps (not mic apps)
                            let menu = self.menu_builder.build(mode, &hfp_apps, &devices, &self.forced_stereo_devices)?;
                            tray.update_menu(menu)?;
                        }
                    }
                    MonitorEvent::ModeChanged { old_mode, new_mode } => {
                        self.notification_manager.show(NotificationType::ModeChange {
                            old: old_mode,
                            new: new_mode,
                        })?;
                    }
                    MonitorEvent::Error(msg) => {
                        warn!("Audio monitor error: {}", msg);
                    }
                    MonitorEvent::Shutdown => {
                        info!("Audio monitor shutdown");
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle menu events
    fn handle_menu_event(&mut self, event: &MudaMenuEvent) -> Result<()> {
        if let Some(menu_event) = self.menu_builder.handle_event(event) {
            match menu_event {
                MenuEvent::TerminateApp(pid) => {
                    info!("Terminate app {} requested", pid);
                    if let Err(e) = self.process_manager.terminate_process(pid, true) {
                        self.notification_manager.show(NotificationType::Error {
                            message: e.to_string(),
                            severity: ErrorSeverity::Recoverable,
                        })?;
                    }
                }
                MenuEvent::ForceStereo(device_name) => {
                    info!("Force stereo requested for: {}", device_name);

                    // Force stereo is quick - just disable HFP service
                    match bluetooth::disable_hfp_by_name(&device_name) {
                        Ok(_) => {
                            // Track that this device has been forced to stereo
                            self.forced_stereo_devices.insert(device_name.clone());
                            self.notification_manager.show(NotificationType::Info {
                                title: rust_i18n::t!("notify_stereo_mode").to_string(),
                                message: rust_i18n::t!("msg_device_stereo", device = &device_name).to_string(),
                            })?;
                        }
                        Err(e) => {
                            error!("Failed to force stereo for {}: {}", device_name, e);
                            self.notification_manager.show(NotificationType::Error {
                                message: rust_i18n::t!("msg_stereo_failed", error = e.to_string()).to_string(),
                                severity: ErrorSeverity::Recoverable,
                            })?;
                        }
                    }
                }
                MenuEvent::AllowHandsFree(device_name) => {
                    info!("Allow hands-free requested for: {}", device_name);

                    // Re-enable HFP service
                    match bluetooth::enable_hfp_by_name(&device_name) {
                        Ok(_) => {
                            // Remove from forced stereo tracking
                            self.forced_stereo_devices.remove(&device_name);
                            self.notification_manager.show(NotificationType::Info {
                                title: rust_i18n::t!("notify_hands_free_enabled").to_string(),
                                message: rust_i18n::t!("msg_device_hands_free", device = &device_name).to_string(),
                            })?;
                        }
                        Err(e) => {
                            error!("Failed to enable hands-free for {}: {}", device_name, e);
                            self.notification_manager.show(NotificationType::Error {
                                message: rust_i18n::t!("msg_hands_free_failed", error = e.to_string()).to_string(),
                                severity: ErrorSeverity::Recoverable,
                            })?;
                        }
                    }
                }
                MenuEvent::ReconnectDevice(device_name) => {
                    info!("Reconnect requested for: {}", device_name);

                    // Check if device is already reconnecting
                    {
                        let reconnecting = self.reconnecting_devices.lock().unwrap();
                        if reconnecting.contains(&device_name) {
                            self.notification_manager.show(NotificationType::Info {
                                title: rust_i18n::t!("notify_already_reconnecting").to_string(),
                                message: rust_i18n::t!("msg_device_already_reconnecting", device = &device_name).to_string(),
                            })?;
                            return Ok(());
                        }
                    }

                    // Show reconnecting notification
                    self.notification_manager.show(NotificationType::Info {
                        title: rust_i18n::t!("notify_reconnecting").to_string(),
                        message: rust_i18n::t!("msg_device_reconnecting", device = &device_name).to_string(),
                    })?;

                    // Spawn background thread for reconnect
                    let name = device_name.clone();
                    let reconnecting_devices = Arc::clone(&self.reconnecting_devices);
                    let notification_manager = self.notification_manager.clone();

                    std::thread::spawn(move || {
                        // Use guard to ensure device is removed from set even on panic
                        let _guard = ReconnectGuard::new(&name, Arc::clone(&reconnecting_devices));

                        // Add device to reconnecting set
                        {
                            let mut reconnecting = reconnecting_devices.lock().unwrap();
                            reconnecting.insert(name.clone());
                        }

                        // Perform reconnect
                        match bluetooth::reconnect_by_name(&name) {
                            Ok(_) => {
                                info!("Successfully reconnected {}", name);
                                let _ = notification_manager.show(NotificationType::Info {
                                    title: rust_i18n::t!("notify_reconnected").to_string(),
                                    message: rust_i18n::t!("msg_device_reconnected", device = &name).to_string(),
                                });
                            }
                            Err(e) => {
                                error!("Failed to reconnect {}: {}", name, e);
                                let _ = notification_manager.show(NotificationType::Error {
                                    message: rust_i18n::t!("msg_reconnect_failed", device = &name, error = e.to_string()).to_string(),
                                    severity: ErrorSeverity::Recoverable,
                                });
                            }
                        }
                    });
                }
                MenuEvent::OpenSettings => {
                    info!("Open settings requested");
                    self.settings_window.open(self.config.clone(), &self.config_manager)?;
                }
                MenuEvent::CheckUpdates => {
                    info!("Check updates requested");
                    self.check_for_updates()?;
                }
                MenuEvent::ShowAbout => {
                    info!("Show about requested");
                    show_about_dialog();
                }
                MenuEvent::Exit => {
                    info!("Exit requested");
                    self.running = false;
                }
            }
        }
        Ok(())
    }

    /// Check for updates
    fn check_for_updates(&mut self) -> Result<()> {
        info!("Checking for updates...");
        match self.update_checker.check_for_updates() {
            Ok(Some(update_info)) => {
                self.notification_manager.show(NotificationType::UpdateAvailable {
                    version: update_info.version,
                })?;
            }
            Ok(None) => {
                info!("No updates available");
                self.notification_manager.show(NotificationType::Info {
                    title: rust_i18n::t!("notify_up_to_date").to_string(),
                    message: rust_i18n::t!("msg_latest_version", version = env!("CARGO_PKG_VERSION")).to_string(),
                })?;
            }
            Err(e) => {
                warn!("Update check failed: {}", e);
                self.notification_manager.show(NotificationType::Info {
                    title: rust_i18n::t!("notify_update_check_failed").to_string(),
                    message: rust_i18n::t!("msg_update_check_error", error = e.to_string()).to_string(),
                })?;
            }
        }
        self.last_update_check = Instant::now();
        Ok(())
    }

    /// Process settings window messages
    fn process_settings_events(&mut self) -> Result<()> {
        if let Some(msg) = self.settings_window.try_recv() {
            match msg {
                win_bt_stereo_vs_handsfree::settings::window::SettingsMessage::Closed(Some(new_config)) => {
                    // Check if language changed
                    let language_changed = new_config.general.language != self.config.general.language;

                    // Handle auto-start change
                    if new_config.general.auto_start != self.config.general.auto_start {
                        self.config_manager.set_auto_start(new_config.general.auto_start)?;
                    }

                    // Save config
                    self.config = new_config;
                    self.config_manager.save(&self.config)?;

                    // Update notification settings
                    self.notification_manager.update_settings(
                        self.config.notifications.notify_mode_change,
                        self.config.notifications.notify_mic_usage,
                        self.config.notifications.notify_errors,
                        self.config.notifications.notify_updates,
                    );

                    // Handle language change
                    if language_changed {
                        // Reinitialize i18n with new language
                        win_bt_stereo_vs_handsfree::i18n::init(self.config.general.language.as_deref());
                        // Menu will be rebuilt with new language on next audio state update (within 500ms)
                        info!("Language changed, i18n reinitialized");
                    }

                    info!("Settings saved");
                }
                win_bt_stereo_vs_handsfree::settings::window::SettingsMessage::Closed(None) => {
                    info!("Settings cancelled");
                }
                win_bt_stereo_vs_handsfree::settings::window::SettingsMessage::Error(e) => {
                    warn!("Settings error: {}", e);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Run the main event loop
    fn run(&mut self) -> Result<()> {
        info!("Starting main event loop");

        // Register for menu events
        let menu_channel = MudaMenuEvent::receiver();

        // Auto update check interval
        let update_check_interval = Duration::from_secs(
            self.config.updates.check_interval_hours as u64 * 3600
        );

        let mut msg = MSG::default();

        while self.running && !SHUTDOWN_FLAG.load(Ordering::SeqCst) {
            // Process Windows messages
            unsafe {
                while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }

            // Process menu events
            if let Ok(event) = menu_channel.try_recv() {
                if let Err(e) = self.handle_menu_event(&event) {
                    error!("Menu event error: {}", e);
                }
            }

            // Process audio events
            if let Err(e) = self.process_audio_events() {
                error!("Audio event error: {}", e);
            }

            // Process settings events
            if let Err(e) = self.process_settings_events() {
                error!("Settings event error: {}", e);
            }

            // Auto update check
            if self.config.updates.auto_check
                && self.last_update_check.elapsed() > update_check_interval
            {
                let _ = self.check_for_updates();
            }

            // Sleep to prevent busy loop
            std::thread::sleep(Duration::from_millis(50));
        }

        Ok(())
    }

    /// Shutdown the application
    fn shutdown(&mut self) {
        info!("Shutting down application");

        if let Some(ref mut monitor) = self.audio_monitor {
            monitor.shutdown();
        }

        // Save config on exit
        if let Err(e) = self.config_manager.save(&self.config) {
            error!("Failed to save config on exit: {}", e);
        }
    }
}

/// Check for single instance using named mutex
fn check_single_instance() -> Result<HANDLE> {
    let mutex_name: Vec<u16> = OsStr::new(SINGLE_INSTANCE_MUTEX)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mutex = CreateMutexW(None, true, PCWSTR::from_raw(mutex_name.as_ptr()))
            .map_err(|e| AppError::WindowsApiError(e))?;

        let last_error = GetLastError();
        // ERROR_ALREADY_EXISTS = 183
        if last_error.0 == 183 {
            let _ = CloseHandle(mutex);
            return Err(AppError::ConfigError(
                rust_i18n::t!("msg_already_running").to_string(),
            ));
        }

        Ok(mutex)
    }
}

/// Show about dialog
fn show_about_dialog() {
    let version = env!("CARGO_PKG_VERSION");
    let message = format!(
        "{}\n\n{}\n\n{}\n\n{}\n{}",
        rust_i18n::t!("about_app_name"),
        rust_i18n::t!("about_version", version = version),
        rust_i18n::t!("about_description"),
        rust_i18n::t!("about_author"),
        rust_i18n::t!("about_license")
    );

    let title = rust_i18n::t!("about_title");

    let message_wide: Vec<u16> = OsStr::new(&message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let title_wide: Vec<u16> = OsStr::new(&*title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            HWND::default(),
            PCWSTR::from_raw(message_wide.as_ptr()),
            PCWSTR::from_raw(title_wide.as_ptr()),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
        );
    }
}

/// Handle elevated termination request
fn handle_elevated_termination(pid_str: &str) {
    let pid: u32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            error!("Invalid PID for elevated termination: {}", pid_str);
            return;
        }
    };

    // Initialize COM for audio session enumeration
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            error!("COM init failed in elevated helper: {:?}", hr);
            return;
        }
    }

    // Perform elevated termination with full re-validation
    match ProcessManager::handle_elevated_termination(pid) {
        Ok(()) => {
            info!("Elevated termination completed for PID {}", pid);
        }
        Err(e) => {
            error!("Elevated termination failed: {}", e);
            let message = format!("Could not terminate process: {}", e);
            let message_wide: Vec<u16> = OsStr::new(&message)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let title_wide: Vec<u16> = OsStr::new("Error")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            unsafe {
                MessageBoxW(
                    HWND::default(),
                    PCWSTR::from_raw(message_wide.as_ptr()),
                    PCWSTR::from_raw(title_wide.as_ptr()),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
    }

    unsafe {
        CoUninitialize();
    }
}

fn main() {
    // Check for elevated termination mode
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--terminate-elevated" {
        handle_elevated_termination(&args[2]);
        return;
    }

    // Check single instance
    let mutex = match check_single_instance() {
        Ok(m) => m,
        Err(e) => {
            let message = e.to_string();
            let message_wide: Vec<u16> = OsStr::new(&message)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let title_wide: Vec<u16> = OsStr::new("Bluetooth Audio Mode Manager")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            unsafe {
                MessageBoxW(
                    HWND::default(),
                    PCWSTR::from_raw(message_wide.as_ptr()),
                    PCWSTR::from_raw(title_wide.as_ptr()),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            return;
        }
    };

    // Initialize COM
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            eprintln!("Failed to initialize COM: {:?}", hr);
            let _ = CloseHandle(mutex);
            return;
        }
    }

    // Initialize logging
    let config_manager = match ConfigManager::new() {
        Ok(cm) => cm,
        Err(e) => {
            eprintln!("Failed to initialize config manager: {}", e);
            unsafe {
                CoUninitialize();
                let _ = CloseHandle(mutex);
            }
            return;
        }
    };

    let config = config_manager.load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {}, using defaults", e);
        AppConfig::default()
    });
    let log_config = LoggingConfig {
        level: parse_log_level(&config.logging.level),
        log_dir: config_manager.log_dir(),
        max_file_size: config.logging.max_file_size,
        max_files: config.logging.max_files,
    };

    if let Err(e) = init_logging(log_config) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    // Initialize i18n with configured or system locale
    win_bt_stereo_vs_handsfree::i18n::init(config.general.language.as_deref());

    // Register console control handler for Ctrl+C
    unsafe {
        if let Err(e) = SetConsoleCtrlHandler(Some(console_ctrl_handler), true) {
            warn!("Failed to set console control handler: {:?}", e);
        }
    }

    info!("Bluetooth Audio Mode Manager starting...");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // Register AUMID for toast notifications
    // This allows notifications to appear in the Windows notification center
    if let Err(e) = register_aumid() {
        warn!("Failed to register AUMID for notifications: {}", e);
        // Continue anyway - notifications will still appear as popups
    }

    // Create and run application
    let result = App::new().and_then(|mut app| {
        app.init()?;
        app.run()?;
        app.shutdown();
        Ok(())
    });

    if let Err(e) = result {
        error!("Application error: {}", e);
    }

    // Cleanup
    unsafe {
        CoUninitialize();
        let _ = ReleaseMutex(mutex);
        let _ = CloseHandle(mutex);
    }

    info!("Bluetooth Audio Mode Manager stopped");
}
