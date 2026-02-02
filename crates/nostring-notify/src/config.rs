//! Notification configuration

use crate::templates::NotificationLevel;
use serde::{Deserialize, Serialize};

/// Main notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    /// Thresholds that trigger notifications
    pub thresholds: Vec<Threshold>,
    /// Email configuration (optional)
    pub email: Option<EmailConfig>,
    /// Nostr DM configuration (optional)
    pub nostr: Option<NostrConfig>,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            thresholds: vec![
                Threshold::days(30),  // Gentle reminder
                Threshold::days(7),   // Warning
                Threshold::days(1),   // Urgent
                Threshold::days(0),   // Critical - heirs can claim!
            ],
            email: None,
            nostr: None,
        }
    }
}

/// A notification threshold
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Threshold {
    /// Days before expiry to trigger
    pub days: u32,
    /// Notification level
    pub level: NotificationLevel,
}

impl Threshold {
    /// Create a threshold at the given number of days
    pub fn days(days: u32) -> Self {
        let level = match days {
            31.. => NotificationLevel::Reminder,
            8..=30 => NotificationLevel::Reminder,
            2..=7 => NotificationLevel::Warning,
            1 => NotificationLevel::Urgent,
            0 => NotificationLevel::Critical,
        };
        Self { days, level }
    }
    
    /// Create a custom threshold
    pub fn custom(days: u32, level: NotificationLevel) -> Self {
        Self { days, level }
    }
}

/// Email (SMTP) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// Enable email notifications
    pub enabled: bool,
    /// SMTP server hostname
    pub smtp_host: String,
    /// SMTP port (typically 587 for TLS)
    pub smtp_port: u16,
    /// SMTP username
    pub smtp_user: String,
    /// SMTP password (stored securely)
    pub smtp_password: String,
    /// Sender email address
    pub from_address: String,
    /// Recipient email address
    pub to_address: String,
}

impl EmailConfig {
    /// Create a new email config
    pub fn new(
        smtp_host: impl Into<String>,
        smtp_user: impl Into<String>,
        smtp_password: impl Into<String>,
        from_address: impl Into<String>,
        to_address: impl Into<String>,
    ) -> Self {
        Self {
            enabled: true,
            smtp_host: smtp_host.into(),
            smtp_port: 587,
            smtp_user: smtp_user.into(),
            smtp_password: smtp_password.into(),
            from_address: from_address.into(),
            to_address: to_address.into(),
        }
    }
}

/// Nostr DM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrConfig {
    /// Enable Nostr DM notifications
    pub enabled: bool,
    /// Recipient's npub or hex public key
    pub recipient_pubkey: String,
    /// Relay URLs to use for sending
    pub relays: Vec<String>,
    /// Secret key (nsec or hex) - if not provided, derived from seed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

impl NostrConfig {
    /// Create a new Nostr config
    pub fn new(recipient_pubkey: impl Into<String>) -> Self {
        Self {
            enabled: true,
            recipient_pubkey: recipient_pubkey.into(),
            relays: vec![
                "wss://relay.damus.io".into(),
                "wss://relay.nostr.band".into(),
                "wss://nos.lol".into(),
            ],
            secret_key: None,
        }
    }
    
    /// Set custom relays
    pub fn with_relays(mut self, relays: Vec<String>) -> Self {
        self.relays = relays;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NotifyConfig::default();
        assert_eq!(config.thresholds.len(), 4);
        assert!(config.email.is_none());
        assert!(config.nostr.is_none());
    }

    #[test]
    fn test_threshold_days() {
        let t = Threshold::days(30);
        assert_eq!(t.days, 30);
        assert_eq!(t.level, NotificationLevel::Reminder);

        let t = Threshold::days(1);
        assert_eq!(t.level, NotificationLevel::Urgent);
        
        let t = Threshold::days(0);
        assert_eq!(t.level, NotificationLevel::Critical);
    }

    #[test]
    fn test_email_config() {
        let config = EmailConfig::new(
            "smtp.example.com",
            "user@example.com",
            "password",
            "noreply@nostring.dev",
            "owner@example.com",
        );
        assert!(config.enabled);
        assert_eq!(config.smtp_port, 587);
    }

    #[test]
    fn test_nostr_config() {
        let config = NostrConfig::new("npub1...");
        assert!(config.enabled);
        assert!(!config.relays.is_empty());
    }
}
