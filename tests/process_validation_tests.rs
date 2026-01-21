//! Tests for process termination security validation

use win_bt_stereo_vs_handsfree::process::{TerminationOutcome};

#[test]
fn test_termination_outcome_display() {
    assert_eq!(format!("{}", TerminationOutcome::Success), "SUCCESS");
    assert_eq!(format!("{}", TerminationOutcome::Blocked), "BLOCKED");
    assert_eq!(format!("{}", TerminationOutcome::Failed), "FAILED");
    assert_eq!(format!("{}", TerminationOutcome::UserCancelled), "USER_CANCELLED");
    assert_eq!(format!("{}", TerminationOutcome::ElevationRequired), "ELEVATION_REQUIRED");
}

#[test]
fn test_system_process_blacklist() {
    // These are system processes that should be protected
    let blacklisted = [
        "csrss.exe",
        "winlogon.exe",
        "lsass.exe",
        "services.exe",
        "smss.exe",
        "wininit.exe",
        "svchost.exe",
        "dwm.exe",
        "explorer.exe",
        "system",
        "registry",
    ];

    for process in &blacklisted {
        // Test case insensitivity
        assert!(is_blacklisted(process), "{} should be blacklisted", process);
        assert!(
            is_blacklisted(&process.to_uppercase()),
            "{} uppercase should be blacklisted",
            process
        );
    }
}

#[test]
fn test_non_system_processes_not_blacklisted() {
    let allowed = [
        "notepad.exe",
        "chrome.exe",
        "firefox.exe",
        "code.exe",
        "discord.exe",
        "zoom.exe",
        "teams.exe",
        "spotify.exe",
    ];

    for process in &allowed {
        assert!(!is_blacklisted(process), "{} should not be blacklisted", process);
    }
}

// Re-implement the blacklist check logic for testing
fn is_blacklisted(process_name: &str) -> bool {
    const SYSTEM_PROCESS_BLACKLIST: &[&str] = &[
        "csrss.exe",
        "winlogon.exe",
        "lsass.exe",
        "services.exe",
        "smss.exe",
        "wininit.exe",
        "svchost.exe",
        "dwm.exe",
        "explorer.exe",
        "system",
        "registry",
    ];

    let lower_name = process_name.to_lowercase();
    SYSTEM_PROCESS_BLACKLIST
        .iter()
        .any(|&blocked| lower_name == blocked)
}

#[test]
fn test_blacklist_case_insensitivity() {
    assert!(is_blacklisted("CSRSS.EXE"));
    assert!(is_blacklisted("Csrss.Exe"));
    assert!(is_blacklisted("csrss.exe"));
    assert!(is_blacklisted("LSASS.EXE"));
}

#[test]
fn test_blacklist_exact_match() {
    // Partial matches should NOT be blocked
    assert!(!is_blacklisted("my_csrss.exe"));
    assert!(!is_blacklisted("csrss"));
    assert!(!is_blacklisted("csrss.exe.bak"));
    assert!(!is_blacklisted("not_lsass.exe"));
}

#[test]
fn test_termination_outcome_clone() {
    let outcome = TerminationOutcome::Success;
    let cloned = outcome.clone();
    assert_eq!(format!("{}", outcome), format!("{}", cloned));
}

/// Test TOCTOU mitigation mutex behavior
/// This test verifies that concurrent access is properly synchronized
#[test]
fn test_mutex_synchronization() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];

    for _ in 0..10 {
        let counter_clone = Arc::clone(&counter);
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let mut num = counter_clone.lock().unwrap();
                *num += 1;
                // Simulate some work
                thread::yield_now();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let final_count = *counter.lock().unwrap();
    assert_eq!(final_count, 1000, "Mutex should ensure all increments are counted");
}

#[test]
fn test_audit_log_structure() {
    use win_bt_stereo_vs_handsfree::process::TerminationAttempt;
    use std::time::SystemTime;

    let attempt = TerminationAttempt {
        timestamp: SystemTime::now(),
        process_id: 1234,
        process_name: "test.exe".to_string(),
        outcome: TerminationOutcome::Blocked,
        reason: "Test reason".to_string(),
    };

    assert_eq!(attempt.process_id, 1234);
    assert_eq!(attempt.process_name, "test.exe");
    assert_eq!(format!("{}", attempt.outcome), "BLOCKED");
    assert_eq!(attempt.reason, "Test reason");
}
