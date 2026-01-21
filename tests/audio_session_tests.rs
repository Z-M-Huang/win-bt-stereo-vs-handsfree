//! Unit tests for audio session enumeration via trait mocks

use win_bt_stereo_vs_handsfree::audio::traits::mocks::{MockAudioSession, MockSessionEnumerator};
use win_bt_stereo_vs_handsfree::audio::traits::{AudioSessionEnumerator, AudioSessionTrait};
use std::sync::Arc;

#[test]
fn test_mock_audio_session_creation() {
    let session = MockAudioSession::new(1234, "test_app");
    assert_eq!(session.get_process_id(), 1234);
    assert_eq!(session.get_display_name(), "test_app");
    assert!(session.is_active());
    assert!(!session.is_muted());
    assert_eq!(session.get_volume(), 1.0);
}

#[test]
fn test_mock_audio_session_mute() {
    let session = MockAudioSession::new(5678, "mutable_app");
    assert!(!session.is_muted());

    session.set_muted(true).unwrap();
    assert!(session.is_muted());

    session.set_muted(false).unwrap();
    assert!(!session.is_muted());
}

#[test]
fn test_mock_audio_session_volume() {
    let session = MockAudioSession::new(9999, "volume_app");
    assert_eq!(session.get_volume(), 1.0);

    session.set_volume(0.5).unwrap();
    assert_eq!(session.get_volume(), 0.5);

    session.set_volume(0.0).unwrap();
    assert_eq!(session.get_volume(), 0.0);
}

#[test]
fn test_mock_session_enumerator_empty() {
    let enumerator = MockSessionEnumerator {
        sessions: Vec::new(),
    };

    let sessions = enumerator.get_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn test_mock_session_enumerator_with_sessions() {
    let session1 = Arc::new(MockAudioSession::new(100, "app1")) as Arc<dyn AudioSessionTrait>;
    let session2 = Arc::new(MockAudioSession::new(200, "app2")) as Arc<dyn AudioSessionTrait>;
    let session3 = Arc::new(MockAudioSession::new(300, "app3")) as Arc<dyn AudioSessionTrait>;

    let enumerator = MockSessionEnumerator {
        sessions: vec![session1, session2, session3],
    };

    let sessions = enumerator.get_sessions().unwrap();
    assert_eq!(sessions.len(), 3);
    assert_eq!(sessions[0].get_process_id(), 100);
    assert_eq!(sessions[1].get_process_id(), 200);
    assert_eq!(sessions[2].get_process_id(), 300);
}

#[test]
fn test_mock_session_enumerator_refresh() {
    let mut enumerator = MockSessionEnumerator {
        sessions: Vec::new(),
    };

    // Refresh should succeed without error
    assert!(enumerator.refresh().is_ok());
}

#[test]
fn test_mock_audio_session_icon_path() {
    let session = MockAudioSession::new(1234, "icon_app");
    // Mock sessions don't have icon paths by default
    assert!(session.get_icon_path().is_none());
}

#[test]
fn test_session_trait_dynamic_dispatch() {
    let session: Arc<dyn AudioSessionTrait> = Arc::new(MockAudioSession::new(1111, "dynamic_app"));

    assert_eq!(session.get_process_id(), 1111);
    assert_eq!(session.get_display_name(), "dynamic_app");
    assert!(session.is_active());
}
