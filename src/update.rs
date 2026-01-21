//! GitHub release update checking with security validation

use crate::error::{AppError, Result};
use log::{info, warn};
use semver::Version;
use sha2::{Digest, Sha256};
use std::time::Duration;

/// GitHub repository for update checks
const GITHUB_OWNER: &str = "Z-M-Huang";
const GITHUB_REPO: &str = "win-bt-stereo-vs-handsfree";

/// Current application version (from Cargo.toml)
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Update check timeout
const UPDATE_TIMEOUT: Duration = Duration::from_secs(5);

/// Information about an available update
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub release_url: String,
    pub download_url: Option<String>,
    pub release_notes: String,
    pub checksum: Option<String>,
}

/// Update checker for GitHub releases
pub struct UpdateChecker {
    current_version: Version,
    last_check_result: Option<UpdateInfo>,
}

impl UpdateChecker {
    /// Create a new update checker
    pub fn new() -> Result<Self> {
        let current_version = Version::parse(CURRENT_VERSION).map_err(|e| {
            AppError::UpdateCheckError(format!("Invalid current version: {}", e))
        })?;

        Ok(Self {
            current_version,
            last_check_result: None,
        })
    }

    /// Get the current version
    #[allow(dead_code)]
    pub fn current_version(&self) -> &Version {
        &self.current_version
    }

    /// Check for updates from GitHub releases
    pub fn check_for_updates(&mut self) -> Result<Option<UpdateInfo>> {
        info!("Checking for updates...");

        let api_url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            GITHUB_OWNER, GITHUB_REPO
        );

        // Use ureq for HTTPS request
        let response = ureq::get(&api_url)
            .set("User-Agent", &format!("BtAudioModeManager/{}", CURRENT_VERSION))
            .set("Accept", "application/vnd.github.v3+json")
            .timeout(UPDATE_TIMEOUT)
            .call()
            .map_err(|e| {
                match e {
                    ureq::Error::Status(429, _) => {
                        warn!("Rate limited by GitHub API");
                        AppError::UpdateCheckError("Rate limited, try again later".to_string())
                    }
                    ureq::Error::Status(code, _) => {
                        AppError::UpdateCheckError(format!("HTTP error: {}", code))
                    }
                    ureq::Error::Transport(t) => {
                        AppError::UpdateCheckError(format!("Network error: {}", t))
                    }
                }
            })?;

        let body = response.into_string().map_err(|e| {
            AppError::UpdateCheckError(format!("Could not read response: {}", e))
        })?;

        // Parse JSON response
        let release: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            AppError::UpdateCheckError(format!("Could not parse response: {}", e))
        })?;

        // Extract version
        let tag_name = release["tag_name"]
            .as_str()
            .ok_or_else(|| AppError::UpdateCheckError("Missing tag_name".to_string()))?;

        // Remove 'v' prefix if present and sanitize
        let version_str = tag_name.trim_start_matches('v');
        let sanitized_version = Self::sanitize_version(version_str)?;

        let latest_version = Version::parse(&sanitized_version).map_err(|e| {
            AppError::UpdateCheckError(format!("Invalid version '{}': {}", version_str, e))
        })?;

        // Compare versions
        if latest_version <= self.current_version {
            info!("Already up to date ({})", CURRENT_VERSION);
            self.last_check_result = None;
            return Ok(None);
        }

        info!("Update available: {} -> {}", CURRENT_VERSION, latest_version);

        // Get release URL
        let release_url = release["html_url"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Get release notes
        let release_notes = release["body"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Find download URL for x64 portable exe
        let download_url = release["assets"]
            .as_array()
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    let name = asset["name"].as_str()?;
                    // Look for x64 portable binary (x64-only support)
                    if name.contains("x64") && name.contains("portable") && name.ends_with(".exe") {
                        asset["browser_download_url"].as_str().map(String::from)
                    } else {
                        None
                    }
                })
            });

        // Find checksum file
        let checksum = self.fetch_checksum(&release);

        let update_info = UpdateInfo {
            version: latest_version.to_string(),
            release_url,
            download_url,
            release_notes,
            checksum,
        };

        self.last_check_result = Some(update_info.clone());
        Ok(Some(update_info))
    }

    /// Fetch SHA256 checksum from release assets
    fn fetch_checksum(&self, release: &serde_json::Value) -> Option<String> {
        let checksum_url = release["assets"]
            .as_array()
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    let name = asset["name"].as_str()?;
                    if name == "SHA256SUMS.txt" || name.contains("checksum") {
                        asset["browser_download_url"].as_str().map(String::from)
                    } else {
                        None
                    }
                })
            })?;

        match ureq::get(&checksum_url)
            .timeout(UPDATE_TIMEOUT)
            .call()
        {
            Ok(response) => {
                let content = response.into_string().ok()?;
                // Parse checksum file (format: "hash  filename")
                for line in content.lines() {
                    if line.contains("portable") && line.contains(".exe") {
                        if let Some(hash) = line.split_whitespace().next() {
                            return Some(hash.to_string());
                        }
                    }
                }
                None
            }
            Err(e) => {
                warn!("Could not fetch checksum file: {}", e);
                None
            }
        }
    }

    /// Sanitize version string to prevent injection
    fn sanitize_version(version: &str) -> Result<String> {
        // Only allow digits, dots, and common pre-release identifiers
        let sanitized: String = version
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '+')
            .take(50) // Limit length
            .collect();

        if sanitized.is_empty() {
            return Err(AppError::UpdateCheckError("Empty version string".to_string()));
        }

        // Verify it looks like a version
        if !sanitized.chars().next().unwrap().is_ascii_digit() {
            return Err(AppError::UpdateCheckError(
                "Version must start with digit".to_string(),
            ));
        }

        Ok(sanitized)
    }

    /// Verify downloaded file against checksum
    #[allow(dead_code)]
    pub fn verify_checksum(file_path: &std::path::Path, expected_hash: &str) -> Result<bool> {
        let mut file = std::fs::File::open(file_path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        let result = hasher.finalize();
        let computed_hash = hex::encode(result);

        let expected_lower = expected_hash.to_lowercase();
        let matches = computed_hash == expected_lower;

        if matches {
            info!("Checksum verified for {:?}", file_path);
        } else {
            warn!(
                "Checksum mismatch for {:?}: expected {}, got {}",
                file_path, expected_lower, computed_hash
            );
        }

        Ok(matches)
    }

    /// Get the last check result
    #[allow(dead_code)]
    pub fn last_result(&self) -> Option<&UpdateInfo> {
        self.last_check_result.as_ref()
    }
}

impl Default for UpdateChecker {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            current_version: Version::new(0, 1, 0),
            last_check_result: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_version() {
        assert!(UpdateChecker::sanitize_version("1.0.0").is_ok());
        assert!(UpdateChecker::sanitize_version("1.0.0-beta").is_ok());
        assert!(UpdateChecker::sanitize_version("1.0.0+build123").is_ok());
        assert!(UpdateChecker::sanitize_version("v1.0.0").is_err()); // starts with v
        assert!(UpdateChecker::sanitize_version("").is_err());
    }

    #[test]
    fn test_current_version_parse() {
        let checker = UpdateChecker::new().unwrap();
        // Verify version was parsed (major should be reasonable)
        assert!(checker.current_version.major < 1000);
    }
}
