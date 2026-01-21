//! Logging setup with rotation support

use crate::error::{AppError, Result};
use log::LevelFilter;
use simplelog::{CombinedLogger, ConfigBuilder, SharedLogger, WriteLogger};
#[cfg(debug_assertions)]
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

/// Default log filename
const LOG_FILENAME: &str = "win_bt_stereo_vs_handsfree.log";

/// Logging configuration
pub struct LoggingConfig {
    pub level: LevelFilter,
    pub log_dir: PathBuf,
    pub max_file_size: u64,
    pub max_files: u32,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
            log_dir: PathBuf::from("."),
            max_file_size: 5 * 1024 * 1024, // 5MB
            max_files: 3,
        }
    }
}

/// Initialize the logging system
pub fn init_logging(config: LoggingConfig) -> Result<()> {
    // Ensure log directory exists
    fs::create_dir_all(&config.log_dir)?;

    let log_path = config.log_dir.join(LOG_FILENAME);

    // Rotate logs if needed
    rotate_logs(&log_path, config.max_file_size, config.max_files)?;

    // Create log file
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(AppError::IoError)?;

    // Build logger configuration
    let log_config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_target_level(LevelFilter::Error)
        .set_location_level(LevelFilter::Debug)
        .set_thread_level(LevelFilter::Off)
        .build();

    let mut loggers: Vec<Box<dyn SharedLogger>> = Vec::new();

    // Terminal logger (for debug builds)
    #[cfg(debug_assertions)]
    {
        loggers.push(TermLogger::new(
            config.level,
            log_config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ));
    }

    // File logger
    loggers.push(WriteLogger::new(config.level, log_config, log_file));

    CombinedLogger::init(loggers)
        .map_err(|e| AppError::ConfigError(format!("Logger init failed: {}", e)))?;

    log::info!("Logging initialized at level {:?}", config.level);
    log::info!("Log file: {:?}", log_path);

    Ok(())
}

/// Rotate log files if the current log exceeds max size
fn rotate_logs(log_path: &PathBuf, max_size: u64, max_files: u32) -> Result<()> {
    if !log_path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(log_path)?;
    if metadata.len() < max_size {
        return Ok(());
    }

    log::debug!("Rotating logs, current size: {} bytes", metadata.len());

    // Delete oldest file if at max
    let oldest = log_path.with_extension(format!("log.{}", max_files));
    if oldest.exists() {
        fs::remove_file(&oldest)?;
    }

    // Rotate existing files
    for i in (1..max_files).rev() {
        let old_name = log_path.with_extension(format!("log.{}", i));
        let new_name = log_path.with_extension(format!("log.{}", i + 1));
        if old_name.exists() {
            fs::rename(&old_name, &new_name)?;
        }
    }

    // Rename current log to .log.1
    let backup = log_path.with_extension("log.1");
    fs::rename(log_path, &backup)?;

    Ok(())
}

/// Parse log level from string
pub fn parse_log_level(level_str: &str) -> LevelFilter {
    match level_str.to_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" | "warning" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off" => LevelFilter::Off,
        _ => LevelFilter::Info,
    }
}

/// Get log level as string
#[allow(dead_code)]
pub fn log_level_to_string(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::Trace => "trace",
        LevelFilter::Debug => "debug",
        LevelFilter::Info => "info",
        LevelFilter::Warn => "warn",
        LevelFilter::Error => "error",
        LevelFilter::Off => "off",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_level() {
        assert_eq!(parse_log_level("info"), LevelFilter::Info);
        assert_eq!(parse_log_level("DEBUG"), LevelFilter::Debug);
        assert_eq!(parse_log_level("Warning"), LevelFilter::Warn);
        assert_eq!(parse_log_level("invalid"), LevelFilter::Info);
    }

    #[test]
    fn test_log_level_to_string() {
        assert_eq!(log_level_to_string(LevelFilter::Info), "info");
        assert_eq!(log_level_to_string(LevelFilter::Debug), "debug");
    }
}
