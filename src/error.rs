use std::fmt;

#[derive(Debug)]
pub enum AppError {
    ComInitFailed(String),
    TrayIconFailed(String),
    AudioSessionError(String),
    ProcessError(String),
    ConfigError(String),
    UpdateCheckError(String),
    IoError(std::io::Error),
    WindowsApiError(windows::core::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::ComInitFailed(msg) => write!(f, "COM initialization failed: {}", msg),
            AppError::TrayIconFailed(msg) => write!(f, "Tray icon creation failed: {}", msg),
            AppError::AudioSessionError(msg) => write!(f, "Audio session error: {}", msg),
            AppError::ProcessError(msg) => write!(f, "Process error: {}", msg),
            AppError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            AppError::UpdateCheckError(msg) => write!(f, "Update check error: {}", msg),
            AppError::IoError(e) => write!(f, "IO error: {}", e),
            AppError::WindowsApiError(e) => write!(f, "Windows API error: {}", e),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IoError(err)
    }
}

impl From<windows::core::Error> for AppError {
    fn from(err: windows::core::Error) -> Self {
        AppError::WindowsApiError(err)
    }
}

impl From<muda::Error> for AppError {
    fn from(err: muda::Error) -> Self {
        AppError::TrayIconFailed(err.to_string())
    }
}

impl From<tray_icon::Error> for AppError {
    fn from(err: tray_icon::Error) -> Self {
        AppError::TrayIconFailed(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Fatal,
    Recoverable,
    Minor,
}

pub struct ErrorContext {
    pub error: AppError,
    pub severity: ErrorSeverity,
    pub context: String,
}

impl ErrorContext {
    pub fn new(error: AppError, severity: ErrorSeverity, context: impl Into<String>) -> Self {
        Self {
            error,
            severity,
            context: context.into(),
        }
    }

    pub fn should_show_toast(&self) -> bool {
        matches!(self.severity, ErrorSeverity::Fatal | ErrorSeverity::Recoverable)
    }
}
