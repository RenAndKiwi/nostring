//! NoString Notification Service
//!
//! Alerts users before their inheritance timelock expires.
//!
//! # Delivery Methods
//!
//! - **Email**: SMTP with user-provided credentials
//! - **Nostr DM**: Encrypted direct message using owner's keys
//!
//! # Example
//!
//! ```ignore
//! use nostring_notify::{NotificationService, NotifyConfig, Threshold};
//!
//! let config = NotifyConfig {
//!     thresholds: vec![
//!         Threshold::days(30),  // Gentle reminder
//!         Threshold::days(7),   // Warning
//!         Threshold::days(1),   // Urgent
//!     ],
//!     email: Some(EmailConfig { ... }),
//!     nostr: Some(NostrConfig { ... }),
//! };
//!
//! let service = NotificationService::new(config);
//! service.check_and_notify(blocks_remaining).await?;
//! ```

mod config;
pub mod nostr_dm;
mod smtp;
pub mod templates;

pub use config::{EmailConfig, NostrConfig, NotifyConfig, Threshold};
pub use templates::NotificationLevel;

use thiserror::Error;

/// Errors from notification operations
#[derive(Error, Debug)]
pub enum NotifyError {
    #[error("Email send failed: {0}")]
    EmailFailed(String),

    #[error("Nostr DM failed: {0}")]
    NostrFailed(String),

    #[error("Electrum error: {0}")]
    Electrum(#[from] nostring_electrum::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Notification service for check-in reminders
pub struct NotificationService {
    config: NotifyConfig,
}

impl NotificationService {
    /// Create a new notification service
    pub fn new(config: NotifyConfig) -> Self {
        Self { config }
    }

    /// Check timelock status and send notifications if needed
    ///
    /// # Arguments
    /// * `blocks_remaining` - Blocks until timelock expires
    /// * `current_height` - Current blockchain height
    ///
    /// # Returns
    /// The notification level that was triggered (if any)
    pub async fn check_and_notify(
        &self,
        blocks_remaining: i64,
        current_height: u32,
    ) -> Result<Option<NotificationLevel>, NotifyError> {
        // Convert blocks to approximate days (10 min/block)
        let days_remaining = blocks_remaining as f64 * 10.0 / 60.0 / 24.0;

        // Find the first threshold that should trigger
        let level = self
            .config
            .thresholds
            .iter()
            .filter(|t| days_remaining <= t.days as f64)
            .map(|t| t.level)
            .max();

        let Some(level) = level else {
            return Ok(None); // No threshold triggered
        };

        // Generate notification content
        let message =
            templates::generate_message(level, days_remaining, blocks_remaining, current_height);

        // Send via configured channels
        let mut sent_any = false;

        if let Some(ref email_config) = self.config.email {
            if email_config.enabled {
                match smtp::send_email(email_config, &message).await {
                    Ok(_) => {
                        log::info!("Email notification sent for level {:?}", level);
                        sent_any = true;
                    }
                    Err(e) => {
                        log::error!("Email notification failed: {}", e);
                    }
                }
            }
        }

        if let Some(ref nostr_config) = self.config.nostr {
            if nostr_config.enabled {
                match nostr_dm::send_dm(nostr_config, &message).await {
                    Ok(_) => {
                        log::info!("Nostr DM sent for level {:?}", level);
                        sent_any = true;
                    }
                    Err(e) => {
                        log::error!("Nostr DM failed: {}", e);
                    }
                }
            }
        }

        if sent_any {
            Ok(Some(level))
        } else {
            Err(NotifyError::Config(
                "No notification channels enabled or all failed".into(),
            ))
        }
    }

    /// Calculate days remaining from blocks
    pub fn blocks_to_days(blocks: i64) -> f64 {
        blocks as f64 * 10.0 / 60.0 / 24.0
    }

    /// Calculate blocks from days
    pub fn days_to_blocks(days: f64) -> i64 {
        (days * 24.0 * 60.0 / 10.0) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocks_to_days() {
        // 144 blocks/day at 10 min/block
        assert!((NotificationService::blocks_to_days(144) - 1.0).abs() < 0.01);
        assert!((NotificationService::blocks_to_days(4320) - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_days_to_blocks() {
        assert_eq!(NotificationService::days_to_blocks(1.0), 144);
        assert_eq!(NotificationService::days_to_blocks(30.0), 4320);
    }

    #[test]
    fn test_threshold_detection() {
        let config = NotifyConfig {
            thresholds: vec![
                Threshold {
                    days: 30,
                    level: NotificationLevel::Reminder,
                },
                Threshold {
                    days: 7,
                    level: NotificationLevel::Warning,
                },
                Threshold {
                    days: 1,
                    level: NotificationLevel::Urgent,
                },
                Threshold {
                    days: 0,
                    level: NotificationLevel::Critical,
                },
            ],
            email: None,
            nostr: None,
        };

        // 45 days remaining - no notification
        let blocks_45 = NotificationService::days_to_blocks(45.0);
        let level = config
            .thresholds
            .iter()
            .filter(|t| 45.0 <= t.days as f64)
            .map(|t| t.level)
            .max();
        assert!(level.is_none());

        // 25 days remaining - reminder level
        let level = config
            .thresholds
            .iter()
            .filter(|t| 25.0 <= t.days as f64)
            .map(|t| t.level)
            .max();
        assert_eq!(level, Some(NotificationLevel::Reminder));
    }
}
