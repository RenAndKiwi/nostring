//! Server configuration — parsed from TOML file + environment variable overrides.
//!
//! Priority: environment variables > config file > defaults.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// General server settings
    #[serde(default)]
    pub server: ServerSection,

    /// Bitcoin / Electrum settings
    #[serde(default)]
    pub bitcoin: BitcoinSection,

    /// Inheritance policy to monitor
    pub policy: PolicySection,

    /// Notification settings
    #[serde(default)]
    pub notifications: NotificationSection,
}

/// General server settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSection {
    /// Data directory (SQLite DB, state files)
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Check interval in seconds (default: 6 hours)
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    /// Log level (error, warn, info, debug, trace)
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            check_interval_secs: default_check_interval(),
            log_level: default_log_level(),
        }
    }
}

/// Bitcoin network settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinSection {
    /// Bitcoin network: "bitcoin", "testnet", "signet", "regtest"
    #[serde(default = "default_network")]
    pub network: String,

    /// Electrum server URL
    #[serde(default = "default_electrum_url")]
    pub electrum_url: String,
}

impl Default for BitcoinSection {
    fn default() -> Self {
        Self {
            network: default_network(),
            electrum_url: default_electrum_url(),
        }
    }
}

/// Inheritance policy to monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySection {
    /// WSH descriptor string for the inheritance policy
    pub descriptor: String,

    /// Timelock duration in blocks
    pub timelock_blocks: u32,

    /// Human-readable label for this policy
    #[serde(default = "default_policy_label")]
    pub label: String,
}

/// Notification channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSection {
    /// Nostr notification settings
    pub nostr: Option<NostrNotifySection>,

    /// Email notification settings
    pub email: Option<EmailNotifySection>,

    /// Notification thresholds in days
    #[serde(default = "default_thresholds")]
    pub threshold_days: Vec<u32>,

    /// Heir contacts for descriptor delivery
    #[serde(default)]
    pub heirs: Vec<HeirContact>,
}

impl Default for NotificationSection {
    fn default() -> Self {
        Self {
            nostr: None,
            email: None,
            threshold_days: default_thresholds(),
            heirs: Vec::new(),
        }
    }
}

/// Nostr notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrNotifySection {
    /// Service key secret (nsec or hex) for sending notifications
    pub service_key: String,

    /// Owner's npub to receive check-in reminders
    pub owner_npub: String,

    /// Relay URLs
    #[serde(default = "default_relays")]
    pub relays: Vec<String>,
}

/// Email notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailNotifySection {
    /// SMTP server hostname
    pub smtp_host: String,

    /// SMTP port (default: 587)
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    /// SMTP username
    pub smtp_user: String,

    /// SMTP password
    pub smtp_password: String,

    /// Sender address
    pub from_address: String,

    /// Owner's email for check-in reminders
    pub owner_email: String,
}

/// Heir contact information for descriptor delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeirContact {
    /// Human-readable label
    pub label: String,

    /// Heir's npub (for Nostr DM delivery)
    pub npub: Option<String>,

    /// Heir's email (for email delivery)
    pub email: Option<String>,
}

// ============================================================================
// Default value functions
// ============================================================================

fn default_data_dir() -> PathBuf {
    PathBuf::from("/data")
}

fn default_check_interval() -> u64 {
    21600 // 6 hours
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_network() -> String {
    "bitcoin".to_string()
}

fn default_electrum_url() -> String {
    "ssl://blockstream.info:700".to_string()
}

fn default_policy_label() -> String {
    "inheritance".to_string()
}

fn default_thresholds() -> Vec<u32> {
    vec![30, 7, 1, 0]
}

fn default_relays() -> Vec<String> {
    vec![
        "wss://relay.damus.io".into(),
        "wss://relay.nostr.band".into(),
        "wss://nos.lol".into(),
    ]
}

fn default_smtp_port() -> u16 {
    587
}

// ============================================================================
// Loading & environment override
// ============================================================================

impl ServerConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: ServerConfig =
            toml::from_str(&contents).with_context(|| "Failed to parse TOML config")?;
        Ok(config)
    }

    /// Apply environment variable overrides.
    ///
    /// Supported env vars:
    /// - `NOSTRING_DATA_DIR`
    /// - `NOSTRING_CHECK_INTERVAL`
    /// - `NOSTRING_LOG_LEVEL`
    /// - `NOSTRING_NETWORK`
    /// - `NOSTRING_ELECTRUM_URL`
    /// - `NOSTRING_DESCRIPTOR`
    /// - `NOSTRING_TIMELOCK_BLOCKS`
    /// - `NOSTRING_SERVICE_KEY`
    /// - `NOSTRING_OWNER_NPUB`
    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("NOSTRING_DATA_DIR") {
            self.server.data_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("NOSTRING_CHECK_INTERVAL") {
            if let Ok(secs) = v.parse::<u64>() {
                self.server.check_interval_secs = secs;
            }
        }
        if let Ok(v) = std::env::var("NOSTRING_LOG_LEVEL") {
            self.server.log_level = v;
        }
        if let Ok(v) = std::env::var("NOSTRING_NETWORK") {
            self.bitcoin.network = v;
        }
        if let Ok(v) = std::env::var("NOSTRING_ELECTRUM_URL") {
            self.bitcoin.electrum_url = v;
        }
        if let Ok(v) = std::env::var("NOSTRING_DESCRIPTOR") {
            self.policy.descriptor = v;
        }
        if let Ok(v) = std::env::var("NOSTRING_TIMELOCK_BLOCKS") {
            if let Ok(blocks) = v.parse::<u32>() {
                self.policy.timelock_blocks = blocks;
            }
        }
        if let Ok(v) = std::env::var("NOSTRING_SERVICE_KEY") {
            if let Some(ref mut nostr) = self.notifications.nostr {
                nostr.service_key = v;
            }
        }
        if let Ok(v) = std::env::var("NOSTRING_OWNER_NPUB") {
            if let Some(ref mut nostr) = self.notifications.nostr {
                nostr.owner_npub = v;
            }
        }
    }

    /// Parse the bitcoin network string to a `bitcoin::Network`.
    pub fn network(&self) -> bitcoin::Network {
        match self.bitcoin.network.as_str() {
            "testnet" | "testnet3" => bitcoin::Network::Testnet,
            "signet" => bitcoin::Network::Signet,
            "regtest" => bitcoin::Network::Regtest,
            _ => bitcoin::Network::Bitcoin,
        }
    }

    /// Validate that the configuration is usable.
    pub fn validate(&self) -> Result<()> {
        // Descriptor must not be empty
        anyhow::ensure!(
            !self.policy.descriptor.is_empty(),
            "policy.descriptor must not be empty"
        );

        // Timelock must be positive
        anyhow::ensure!(
            self.policy.timelock_blocks > 0,
            "policy.timelock_blocks must be > 0"
        );

        // Check interval must be at least 60 seconds
        anyhow::ensure!(
            self.server.check_interval_secs >= 60,
            "server.check_interval_secs must be >= 60"
        );

        // If Nostr notifications configured, need service key and owner npub
        if let Some(ref nostr) = self.notifications.nostr {
            anyhow::ensure!(
                !nostr.service_key.is_empty(),
                "notifications.nostr.service_key must not be empty"
            );
            anyhow::ensure!(
                !nostr.owner_npub.is_empty(),
                "notifications.nostr.owner_npub must not be empty"
            );
        }

        // If email notifications configured, need core SMTP fields
        if let Some(ref email) = self.notifications.email {
            anyhow::ensure!(
                !email.smtp_host.is_empty(),
                "notifications.email.smtp_host must not be empty"
            );
            anyhow::ensure!(
                !email.from_address.is_empty(),
                "notifications.email.from_address must not be empty"
            );
            anyhow::ensure!(
                !email.owner_email.is_empty(),
                "notifications.email.owner_email must not be empty"
            );
        }

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn minimal_toml() -> &'static str {
        r#"
[policy]
descriptor = "wsh(or_d(pk([00000001/84'/0'/0']xpub6.../0/*),and_v(v:pk([00000002/84'/0'/1']xpub6.../0/*),older(26280))))"
timelock_blocks = 26280
"#
    }

    fn full_toml() -> &'static str {
        r#"
[server]
data_dir = "/custom/data"
check_interval_secs = 3600
log_level = "debug"

[bitcoin]
network = "testnet"
electrum_url = "ssl://blockstream.info:993"

[policy]
descriptor = "wsh(or_d(pk(xpub1),and_v(v:pk(xpub2),older(26280))))"
timelock_blocks = 26280
label = "family-inheritance"

[notifications]
threshold_days = [30, 14, 7, 3, 1, 0]

[notifications.nostr]
service_key = "nsec1testkey"
owner_npub = "npub1testowner"
relays = ["wss://relay.damus.io", "wss://nos.lol"]

[notifications.email]
smtp_host = "smtp.example.com"
smtp_port = 587
smtp_user = "user@example.com"
smtp_password = "secret"
from_address = "nostring@example.com"
owner_email = "owner@example.com"

[[notifications.heirs]]
label = "Spouse"
npub = "npub1spouse"
email = "spouse@example.com"

[[notifications.heirs]]
label = "Child"
npub = "npub1child"
"#
    }

    #[test]
    fn test_parse_minimal_config() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", minimal_toml()).unwrap();

        let config = ServerConfig::from_file(file.path()).unwrap();
        assert_eq!(config.policy.timelock_blocks, 26280);
        assert_eq!(config.server.check_interval_secs, 21600); // default
        assert_eq!(config.bitcoin.network, "bitcoin"); // default
        assert!(config.notifications.nostr.is_none());
        assert!(config.notifications.email.is_none());
    }

    #[test]
    fn test_parse_full_config() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", full_toml()).unwrap();

        let config = ServerConfig::from_file(file.path()).unwrap();

        assert_eq!(config.server.data_dir, PathBuf::from("/custom/data"));
        assert_eq!(config.server.check_interval_secs, 3600);
        assert_eq!(config.server.log_level, "debug");
        assert_eq!(config.bitcoin.network, "testnet");
        assert_eq!(config.policy.label, "family-inheritance");

        let nostr = config.notifications.nostr.as_ref().unwrap();
        assert_eq!(nostr.service_key, "nsec1testkey");
        assert_eq!(nostr.owner_npub, "npub1testowner");
        assert_eq!(nostr.relays.len(), 2);

        let email = config.notifications.email.as_ref().unwrap();
        assert_eq!(email.smtp_host, "smtp.example.com");
        assert_eq!(email.owner_email, "owner@example.com");

        assert_eq!(config.notifications.heirs.len(), 2);
        assert_eq!(config.notifications.heirs[0].label, "Spouse");
        assert_eq!(config.notifications.heirs[1].email, None);
    }

    #[test]
    fn test_env_overrides() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", minimal_toml()).unwrap();

        let mut config = ServerConfig::from_file(file.path()).unwrap();

        // Set env vars
        std::env::set_var("NOSTRING_DATA_DIR", "/env/data");
        std::env::set_var("NOSTRING_CHECK_INTERVAL", "1800");
        std::env::set_var("NOSTRING_NETWORK", "signet");

        config.apply_env_overrides();

        assert_eq!(config.server.data_dir, PathBuf::from("/env/data"));
        assert_eq!(config.server.check_interval_secs, 1800);
        assert_eq!(config.bitcoin.network, "signet");

        // Clean up
        std::env::remove_var("NOSTRING_DATA_DIR");
        std::env::remove_var("NOSTRING_CHECK_INTERVAL");
        std::env::remove_var("NOSTRING_NETWORK");
    }

    #[test]
    fn test_network_parsing() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", minimal_toml()).unwrap();
        let config = ServerConfig::from_file(file.path()).unwrap();
        assert_eq!(config.network(), bitcoin::Network::Bitcoin);
    }

    #[test]
    fn test_validation_ok() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", minimal_toml()).unwrap();
        let config = ServerConfig::from_file(file.path()).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_empty_descriptor() {
        let toml = r#"
[policy]
descriptor = ""
timelock_blocks = 26280
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml).unwrap();

        let config = ServerConfig::from_file(file.path()).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_zero_timelock() {
        let toml = r#"
[policy]
descriptor = "wsh(pk(xpub...))"
timelock_blocks = 0
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml).unwrap();

        let config = ServerConfig::from_file(file.path()).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_check_interval_too_low() {
        let toml = r#"
[server]
check_interval_secs = 30

[policy]
descriptor = "wsh(pk(xpub...))"
timelock_blocks = 26280
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml).unwrap();

        let config = ServerConfig::from_file(file.path()).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_default_thresholds() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", minimal_toml()).unwrap();
        let config = ServerConfig::from_file(file.path()).unwrap();
        assert_eq!(config.notifications.threshold_days, vec![30, 7, 1, 0]);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", full_toml()).unwrap();

        let config = ServerConfig::from_file(file.path()).unwrap();
        let serialized = toml::to_string_pretty(&config).unwrap();

        // Should be valid TOML that re-parses
        let reparsed: ServerConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(
            reparsed.policy.timelock_blocks,
            config.policy.timelock_blocks
        );
        assert_eq!(reparsed.bitcoin.network, config.bitcoin.network);
    }
}
