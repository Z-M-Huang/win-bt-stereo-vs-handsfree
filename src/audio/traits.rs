//! Trait abstractions over WASAPI interfaces for testability
//! These traits allow mocking COM interfaces in unit tests

use crate::error::Result;
use std::sync::Arc;

/// Represents an audio session for a specific application
pub trait AudioSessionTrait: Send + Sync {
    fn get_process_id(&self) -> u32;
    fn get_display_name(&self) -> String;
    fn get_icon_path(&self) -> Option<String>;
    fn is_active(&self) -> bool;
    fn get_volume(&self) -> f32;
    fn set_volume(&self, volume: f32) -> Result<()>;
    fn is_muted(&self) -> bool;
    fn set_muted(&self, muted: bool) -> Result<()>;
}

/// Enumerates audio sessions on a capture device
pub trait AudioSessionEnumerator: Send + Sync {
    fn get_sessions(&self) -> Result<Vec<Arc<dyn AudioSessionTrait>>>;
    fn refresh(&mut self) -> Result<()>;
}

/// Manages audio sessions for a specific endpoint
pub trait AudioSessionManager: Send + Sync {
    fn get_capture_session_enumerator(&self) -> Result<Box<dyn AudioSessionEnumerator>>;
    fn is_mic_in_use(&self) -> Result<bool>;
}

/// Factory for creating audio session managers
pub trait AudioManagerFactory: Send + Sync {
    fn create_for_default_capture(&self) -> Result<Box<dyn AudioSessionManager>>;
    fn create_for_device(&self, device_id: &str) -> Result<Box<dyn AudioSessionManager>>;
}

/// Mock implementations for testing
/// Available in tests and with the "test-mocks" feature
#[cfg(any(test, feature = "test-mocks"))]
pub mod mocks {
    use super::*;
    use std::sync::Mutex;

    pub struct MockAudioSession {
        pub process_id: u32,
        pub display_name: String,
        pub is_active: bool,
        pub volume: Mutex<f32>,
        pub muted: Mutex<bool>,
    }

    impl MockAudioSession {
        pub fn new(process_id: u32, name: &str) -> Self {
            Self {
                process_id,
                display_name: name.to_string(),
                is_active: true,
                volume: Mutex::new(1.0),
                muted: Mutex::new(false),
            }
        }
    }

    impl AudioSessionTrait for MockAudioSession {
        fn get_process_id(&self) -> u32 {
            self.process_id
        }

        fn get_display_name(&self) -> String {
            self.display_name.clone()
        }

        fn get_icon_path(&self) -> Option<String> {
            None
        }

        fn is_active(&self) -> bool {
            self.is_active
        }

        fn get_volume(&self) -> f32 {
            *self.volume.lock().unwrap()
        }

        fn set_volume(&self, volume: f32) -> Result<()> {
            *self.volume.lock().unwrap() = volume;
            Ok(())
        }

        fn is_muted(&self) -> bool {
            *self.muted.lock().unwrap()
        }

        fn set_muted(&self, muted: bool) -> Result<()> {
            *self.muted.lock().unwrap() = muted;
            Ok(())
        }
    }

    pub struct MockSessionEnumerator {
        pub sessions: Vec<Arc<dyn AudioSessionTrait>>,
    }

    impl AudioSessionEnumerator for MockSessionEnumerator {
        fn get_sessions(&self) -> Result<Vec<Arc<dyn AudioSessionTrait>>> {
            Ok(self.sessions.clone())
        }

        fn refresh(&mut self) -> Result<()> {
            Ok(())
        }
    }
}
