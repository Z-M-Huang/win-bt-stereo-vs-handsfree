//! Process management with security validation and TOCTOU mitigation
//!
//! This module handles process termination with multiple security layers:
//! - System process blacklist
//! - Privilege level checking
//! - TOCTOU mitigation with mutex-protected operations
//! - User confirmation dialogs
//! - Runtime elevation for privileged processes
//! - Audit logging

use crate::audio::session::MicUsingApp;
use crate::error::{AppError, Result};
use log::{error, info, warn};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Security::{
    CreateWellKnownSid, EqualSid, GetTokenInformation, TokenElevation, TokenUser,
    TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, WinLocalSystemSid,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, TerminateProcess, PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO, SW_SHOWNORMAL,
};

/// System processes that should never be terminated
const SYSTEM_PROCESS_BLACKLIST: &[&str] = &[
    "csrss.exe",
    "winlogon.exe",
    "lsass.exe",
    "services.exe",
    "smss.exe",
    "wininit.exe",
    "svchost.exe",
    "dwm.exe",
    "explorer.exe", // Could crash desktop
    "system",
    "registry",
];

/// Result of a termination attempt for audit logging
#[derive(Debug, Clone)]
pub struct TerminationAttempt {
    pub timestamp: std::time::SystemTime,
    pub process_id: u32,
    pub process_name: String,
    pub outcome: TerminationOutcome,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub enum TerminationOutcome {
    Success,
    Blocked,
    Failed,
    UserCancelled,
    ElevationRequired,
}

impl std::fmt::Display for TerminationOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationOutcome::Success => write!(f, "SUCCESS"),
            TerminationOutcome::Blocked => write!(f, "BLOCKED"),
            TerminationOutcome::Failed => write!(f, "FAILED"),
            TerminationOutcome::UserCancelled => write!(f, "USER_CANCELLED"),
            TerminationOutcome::ElevationRequired => write!(f, "ELEVATION_REQUIRED"),
        }
    }
}

/// Process manager with security validation
pub struct ProcessManager {
    /// Mutex for TOCTOU mitigation - protects check-and-terminate operations
    operation_lock: Arc<Mutex<()>>,
    /// Current list of mic-using apps (shared with monitor thread)
    mic_apps: Arc<Mutex<Vec<MicUsingApp>>>,
    /// Audit log of termination attempts
    audit_log: Mutex<Vec<TerminationAttempt>>,
}

impl ProcessManager {
    /// Create a new process manager
    pub fn new(mic_apps: Arc<Mutex<Vec<MicUsingApp>>>) -> Self {
        Self {
            operation_lock: Arc::new(Mutex::new(())),
            mic_apps,
            audit_log: Mutex::new(Vec::new()),
        }
    }

    /// Get the operation lock for use by monitor thread when updating mic apps list
    pub fn get_operation_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.operation_lock)
    }

    /// Check if a process name is in the system blacklist
    fn is_blacklisted(process_name: &str) -> bool {
        let lower_name = process_name.to_lowercase();
        SYSTEM_PROCESS_BLACKLIST
            .iter()
            .any(|&blocked| lower_name == blocked)
    }

    /// Check if a process is running with elevated privileges (SYSTEM or admin)
    fn is_elevated_process(process_id: u32) -> Result<bool> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_INFORMATION, false, process_id);
            if process.is_err() {
                // Can't open process - likely elevated or system
                return Ok(true);
            }
            let process = process?;

            let mut token = HANDLE::default();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
                let _ = CloseHandle(process);
                // Can't get token - assume elevated
                return Ok(true);
            }

            let mut elevation = TOKEN_ELEVATION::default();
            let mut size = 0u32;
            let result = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            );

            let _ = CloseHandle(token);
            let _ = CloseHandle(process);

            if result.is_err() {
                return Ok(true); // Assume elevated if check fails
            }

            Ok(elevation.TokenIsElevated != 0)
        }
    }

    /// Check if the process is a SYSTEM process by checking token SID
    /// Compares the process token SID against the well-known LocalSystem SID (S-1-5-18)
    fn is_system_process(process_id: u32) -> bool {
        unsafe {
            let process = match OpenProcess(PROCESS_QUERY_INFORMATION, false, process_id) {
                Ok(p) => p,
                Err(_) => return true, // Can't open - assume SYSTEM
            };

            let mut token = HANDLE::default();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
                let _ = CloseHandle(process);
                return true;
            }

            // Get token user info to check SID
            let mut size = 0u32;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);

            if size == 0 {
                let _ = CloseHandle(token);
                let _ = CloseHandle(process);
                return true;
            }

            let mut buffer = vec![0u8; size as usize];
            let result = GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr() as *mut _),
                size,
                &mut size,
            );

            let _ = CloseHandle(token);
            let _ = CloseHandle(process);

            if result.is_err() {
                return true;
            }

            // Extract the TOKEN_USER structure and get the SID pointer
            let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);
            let process_sid = token_user.User.Sid;

            // Create the well-known LocalSystem SID (S-1-5-18)
            let mut system_sid_buffer = vec![0u8; 64]; // MAX_SID_SIZE is typically smaller
            let mut system_sid_size = system_sid_buffer.len() as u32;

            let sid_result = CreateWellKnownSid(
                WinLocalSystemSid,
                None,
                windows::Win32::Security::PSID(system_sid_buffer.as_mut_ptr() as *mut _),
                &mut system_sid_size,
            );

            if sid_result.is_err() {
                // If we can't create the SYSTEM SID, assume it's not a SYSTEM process
                // (fail-open here since we have other protections like blacklist)
                warn!("Failed to create well-known SYSTEM SID for comparison");
                return false;
            }

            // Compare the process SID with the SYSTEM SID
            // In windows-rs 0.58, EqualSid returns Result<()> where:
            // - Ok(()) means SIDs ARE equal (Win32 BOOL TRUE)
            // - Err(_) means SIDs are NOT equal (Win32 BOOL FALSE)
            let system_psid = windows::Win32::Security::PSID(system_sid_buffer.as_ptr() as *mut _);
            let is_system = match EqualSid(process_sid, system_psid) {
                Ok(()) => true,  // SIDs are equal - this IS a SYSTEM process
                Err(_) => false, // SIDs are not equal - not a SYSTEM process
            };

            if is_system {
                info!("Process {} detected as SYSTEM process via SID check", process_id);
            }

            is_system
        }
    }

    /// Get process name from PID
    fn get_process_name(process_id: u32) -> Option<String> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    if entry.th32ProcessID == process_id {
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

    /// Show confirmation dialog before termination
    fn show_confirmation_dialog(process_name: &str, process_id: u32) -> bool {
        let message = format!(
            "Are you sure you want to terminate '{}'?\n\nProcess ID: {}\n\nThis will stop the application from using the microphone.",
            process_name, process_id
        );

        let title = "Confirm Process Termination";

        let message_wide: Vec<u16> = OsStr::new(&message)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let result = MessageBoxW(
                HWND::default(),
                PCWSTR::from_raw(message_wide.as_ptr()),
                PCWSTR::from_raw(title_wide.as_ptr()),
                MB_YESNO | MB_ICONWARNING,
            );
            result == IDYES
        }
    }

    /// Log a termination attempt
    fn log_attempt(&self, attempt: TerminationAttempt) {
        info!(
            "Termination attempt: PID={}, Name={}, Outcome={}, Reason={}",
            attempt.process_id, attempt.process_name, attempt.outcome, attempt.reason
        );

        if let Ok(mut log) = self.audit_log.lock() {
            log.push(attempt);
            // Keep only last 100 entries
            if log.len() > 100 {
                log.remove(0);
            }
        }
    }

    /// Get recent audit log entries
    pub fn get_audit_log(&self) -> Vec<TerminationAttempt> {
        self.audit_log
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    /// Validate that a process can be terminated
    fn validate_termination(&self, process_id: u32) -> Result<(String, bool)> {
        // Get process name
        let process_name = Self::get_process_name(process_id).ok_or_else(|| {
            AppError::ProcessError(format!("Could not find process with ID {}", process_id))
        })?;

        // Check blacklist
        if Self::is_blacklisted(&process_name) {
            return Err(AppError::ProcessError(format!(
                "Process '{}' is a protected system process and cannot be terminated",
                process_name
            )));
        }

        // Check if SYSTEM process
        if Self::is_system_process(process_id) {
            return Err(AppError::ProcessError(format!(
                "Process '{}' is running as SYSTEM and cannot be terminated",
                process_name
            )));
        }

        // Check elevation
        let needs_elevation = Self::is_elevated_process(process_id).unwrap_or(true);

        // Verify process is in mic-using list
        let in_mic_list = self
            .mic_apps
            .lock()
            .map(|apps| apps.iter().any(|app| app.process_id == process_id))
            .unwrap_or(false);

        if !in_mic_list {
            return Err(AppError::ProcessError(format!(
                "Process '{}' is not currently using the microphone",
                process_name
            )));
        }

        Ok((process_name, needs_elevation))
    }

    /// Terminate a process with full security validation
    pub fn terminate_process(&self, process_id: u32, show_dialog: bool) -> Result<()> {
        // Acquire operation lock for TOCTOU mitigation
        let _lock = self.operation_lock.lock().map_err(|_| {
            AppError::ProcessError("Failed to acquire operation lock".to_string())
        })?;

        // Validate the termination request
        let (process_name, needs_elevation) = match self.validate_termination(process_id) {
            Ok(result) => result,
            Err(e) => {
                self.log_attempt(TerminationAttempt {
                    timestamp: std::time::SystemTime::now(),
                    process_id,
                    process_name: Self::get_process_name(process_id)
                        .unwrap_or_else(|| format!("PID {}", process_id)),
                    outcome: TerminationOutcome::Blocked,
                    reason: e.to_string(),
                });
                return Err(e);
            }
        };

        // Show confirmation dialog if requested
        if show_dialog && !Self::show_confirmation_dialog(&process_name, process_id) {
            self.log_attempt(TerminationAttempt {
                timestamp: std::time::SystemTime::now(),
                process_id,
                process_name: process_name.clone(),
                outcome: TerminationOutcome::UserCancelled,
                reason: "User cancelled termination dialog".to_string(),
            });
            return Ok(());
        }

        // If process is elevated, we need to use runtime elevation
        if needs_elevation {
            self.log_attempt(TerminationAttempt {
                timestamp: std::time::SystemTime::now(),
                process_id,
                process_name: process_name.clone(),
                outcome: TerminationOutcome::ElevationRequired,
                reason: "Process requires elevation to terminate".to_string(),
            });
            return self.terminate_with_elevation(process_id, &process_name);
        }

        // Perform the termination
        self.do_terminate(process_id, &process_name)
    }

    /// Actually terminate the process
    fn do_terminate(&self, process_id: u32, process_name: &str) -> Result<()> {
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, false, process_id).map_err(|e| {
                let err = AppError::ProcessError(format!(
                    "Failed to open process '{}': {}",
                    process_name, e
                ));
                self.log_attempt(TerminationAttempt {
                    timestamp: std::time::SystemTime::now(),
                    process_id,
                    process_name: process_name.to_string(),
                    outcome: TerminationOutcome::Failed,
                    reason: format!("OpenProcess failed: {}", e),
                });
                err
            })?;

            let result = TerminateProcess(process, 1);
            let _ = CloseHandle(process);

            if result.is_err() {
                self.log_attempt(TerminationAttempt {
                    timestamp: std::time::SystemTime::now(),
                    process_id,
                    process_name: process_name.to_string(),
                    outcome: TerminationOutcome::Failed,
                    reason: "TerminateProcess failed".to_string(),
                });
                return Err(AppError::ProcessError(format!(
                    "Failed to terminate process '{}'",
                    process_name
                )));
            }

            self.log_attempt(TerminationAttempt {
                timestamp: std::time::SystemTime::now(),
                process_id,
                process_name: process_name.to_string(),
                outcome: TerminationOutcome::Success,
                reason: "Process terminated successfully".to_string(),
            });

            info!("Successfully terminated process '{}' (PID: {})", process_name, process_id);
            Ok(())
        }
    }

    /// Terminate a process using runtime elevation
    fn terminate_with_elevation(&self, process_id: u32, process_name: &str) -> Result<()> {
        // Show dialog explaining elevation requirement
        let message = format!(
            "The process '{}' requires administrator privileges to terminate.\n\n\
             Click OK to continue with elevation, or Cancel to abort.",
            process_name
        );

        let title = "Elevation Required";

        let message_wide: Vec<u16> = OsStr::new(&message)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let result = MessageBoxW(
                HWND::default(),
                PCWSTR::from_raw(message_wide.as_ptr()),
                PCWSTR::from_raw(title_wide.as_ptr()),
                MB_YESNO | MB_ICONWARNING,
            );

            if result != IDYES {
                return Ok(());
            }
        }

        // Get current executable path
        let exe_path = std::env::current_exe().map_err(|e| {
            AppError::ProcessError(format!("Could not get executable path: {}", e))
        })?;

        // Launch elevated instance with special argument
        // SECURITY NOTE: The elevated instance will re-validate everything
        // and NOT trust the PID we pass. It will re-enumerate mic-using apps.
        let args = format!("--terminate-elevated {}", process_id);

        let exe_wide: Vec<u16> = exe_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let args_wide: Vec<u16> = OsStr::new(&args)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb_wide: Vec<u16> = OsStr::new("runas")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let result = ShellExecuteW(
                HWND::default(),
                PCWSTR::from_raw(verb_wide.as_ptr()),
                PCWSTR::from_raw(exe_wide.as_ptr()),
                PCWSTR::from_raw(args_wide.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );

            // ShellExecuteW returns > 32 on success
            if result.0 as usize <= 32 {
                return Err(AppError::ProcessError(
                    "Failed to launch elevated helper".to_string(),
                ));
            }
        }

        info!(
            "Launched elevated helper for process '{}' (PID: {})",
            process_name, process_id
        );
        Ok(())
    }

    /// Handle elevated termination request (called when app is launched with --terminate-elevated)
    /// CRITICAL: This function performs FULL re-validation and does NOT trust the PID argument
    pub fn handle_elevated_termination(requested_pid: u32) -> Result<()> {
        info!("Elevated termination requested for PID {}", requested_pid);

        // Step 1: Re-enumerate current mic-using processes
        let mic_apps = match crate::audio::session::CaptureSessionManager::new_default() {
            Ok(manager) => manager.get_mic_using_apps().unwrap_or_default(),
            Err(e) => {
                error!("Failed to enumerate mic-using apps in elevated context: {}", e);
                return Err(AppError::ProcessError(
                    "Could not verify mic-using applications".to_string(),
                ));
            }
        };

        // Step 2: Verify the requested PID is actually in the current mic-using list
        let app = mic_apps.iter().find(|app| app.process_id == requested_pid);
        let app = match app {
            Some(a) => a,
            None => {
                warn!(
                    "Elevated termination blocked: PID {} is not in current mic-using list",
                    requested_pid
                );
                return Err(AppError::ProcessError(format!(
                    "Process {} is not currently using the microphone",
                    requested_pid
                )));
            }
        };

        // Step 3: Re-check blacklist
        if Self::is_blacklisted(&app.process_name) {
            warn!(
                "Elevated termination blocked: {} is blacklisted",
                app.process_name
            );
            return Err(AppError::ProcessError(format!(
                "Process '{}' is a protected system process",
                app.process_name
            )));
        }

        // Step 4: Re-check SYSTEM process
        if Self::is_system_process(requested_pid) {
            warn!(
                "Elevated termination blocked: {} is a SYSTEM process",
                app.process_name
            );
            return Err(AppError::ProcessError(format!(
                "Process '{}' is running as SYSTEM",
                app.process_name
            )));
        }

        // Step 5: Show confirmation dialog again in elevated context
        if !Self::show_confirmation_dialog(&app.process_name, requested_pid) {
            info!("User cancelled elevated termination");
            return Ok(());
        }

        // Step 6: Perform termination
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, false, requested_pid).map_err(|e| {
                AppError::ProcessError(format!(
                    "Failed to open process '{}': {}",
                    app.process_name, e
                ))
            })?;

            let result = TerminateProcess(process, 1);
            let _ = CloseHandle(process);

            if result.is_err() {
                return Err(AppError::ProcessError(format!(
                    "Failed to terminate process '{}'",
                    app.process_name
                )));
            }
        }

        info!(
            "Elevated termination successful: {} (PID: {})",
            app.process_name, requested_pid
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blacklist_check() {
        assert!(ProcessManager::is_blacklisted("csrss.exe"));
        assert!(ProcessManager::is_blacklisted("CSRSS.EXE"));
        assert!(ProcessManager::is_blacklisted("lsass.exe"));
        assert!(ProcessManager::is_blacklisted("svchost.exe"));
        assert!(!ProcessManager::is_blacklisted("notepad.exe"));
        assert!(!ProcessManager::is_blacklisted("chrome.exe"));
    }

    #[test]
    fn test_termination_outcome_display() {
        assert_eq!(format!("{}", TerminationOutcome::Success), "SUCCESS");
        assert_eq!(format!("{}", TerminationOutcome::Blocked), "BLOCKED");
        assert_eq!(format!("{}", TerminationOutcome::Failed), "FAILED");
        assert_eq!(format!("{}", TerminationOutcome::UserCancelled), "USER_CANCELLED");
        assert_eq!(format!("{}", TerminationOutcome::ElevationRequired), "ELEVATION_REQUIRED");
    }
}
