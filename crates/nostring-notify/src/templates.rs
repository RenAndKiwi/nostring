//! Notification message templates

use serde::{Deserialize, Serialize};

/// Notification urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NotificationLevel {
    /// Gentle reminder (30+ days)
    Reminder = 1,
    /// Warning (7-30 days)
    Warning = 2,
    /// Urgent (1-7 days)
    Urgent = 3,
    /// Critical (expired or about to expire)
    Critical = 4,
}

/// A notification message ready to send
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    /// Message subject (for email)
    pub subject: String,
    /// Message body (plain text)
    pub body: String,
    /// Urgency level
    pub level: NotificationLevel,
}

/// Generate a notification message based on the urgency level
pub fn generate_message(
    level: NotificationLevel,
    days_remaining: f64,
    blocks_remaining: i64,
    current_height: u32,
) -> NotificationMessage {
    let days_str = if days_remaining < 1.0 {
        format!("{:.1} hours", days_remaining * 24.0)
    } else if days_remaining < 2.0 {
        format!("{:.0} day", days_remaining)
    } else {
        format!("{:.0} days", days_remaining)
    };

    let (subject, body) = match level {
        NotificationLevel::Reminder => (
            format!("NoString: Check-in reminder ({} remaining)", days_str),
            format!(
                r#"Hello,

This is a friendly reminder that your NoString inheritance check-in 
expires in approximately {}.

Current block height: {}
Blocks remaining: {}

To reset your timelock and prove you're still in control of your 
Bitcoin, please open NoString and complete a check-in transaction.

Stay sovereign,
NoString"#,
                days_str, current_height, blocks_remaining
            ),
        ),

        NotificationLevel::Warning => (
            format!("⚠️ NoString: Check-in WARNING ({} remaining)", days_str),
            format!(
                r#"⚠️ WARNING: Check-in Required Soon

Your NoString inheritance check-in expires in approximately {}.

Current block height: {}
Blocks remaining: {}

If you do not complete a check-in before the timelock expires, 
your designated heirs will be able to claim your Bitcoin.

Please complete a check-in transaction as soon as possible.

Stay sovereign,
NoString"#,
                days_str, current_height, blocks_remaining
            ),
        ),

        NotificationLevel::Urgent => (
            format!("🚨 NoString: URGENT - Check-in expires in {}!", days_str),
            format!(
                r#"🚨 URGENT: CHECK-IN REQUIRED IMMEDIATELY

Your NoString inheritance check-in expires in approximately {}.

Current block height: {}
Blocks remaining: {}

⚠️ If you do not check in before expiry, your heirs can claim!

Please complete a check-in transaction IMMEDIATELY.

Stay sovereign,
NoString"#,
                days_str, current_height, blocks_remaining
            ),
        ),

        NotificationLevel::Critical => (
            "🔴 NoString: CRITICAL - Timelock EXPIRED or expiring NOW!".to_string(),
            format!(
                r#"🔴 CRITICAL: TIMELOCK EXPIRED OR EXPIRING NOW

Your NoString inheritance timelock has expired or is about to expire!

Current block height: {}
Blocks remaining: {} (may be negative if expired)

⚠️⚠️⚠️ YOUR HEIRS CAN NOW CLAIM YOUR BITCOIN ⚠️⚠️⚠️

If this is intentional (you want heirs to claim), no action needed.

If you're still alive and in control:
1. Open NoString immediately
2. Complete a check-in transaction
3. Monitor for any heir claims

Stay sovereign,
NoString"#,
                current_height, blocks_remaining
            ),
        ),
    };

    NotificationMessage {
        subject,
        body,
        level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_reminder() {
        let msg = generate_message(NotificationLevel::Reminder, 25.0, 3600, 934000);
        assert!(msg.subject.contains("reminder"));
        assert!(msg.body.contains("25 days"));
        assert!(msg.body.contains("934000"));
    }

    #[test]
    fn test_generate_urgent() {
        let msg = generate_message(NotificationLevel::Urgent, 0.5, 72, 934000);
        assert!(msg.subject.contains("URGENT"));
        assert!(msg.body.contains("12.0 hours"));
    }

    #[test]
    fn test_generate_critical() {
        let msg = generate_message(NotificationLevel::Critical, -1.0, -144, 934000);
        assert!(msg.subject.contains("CRITICAL"));
        assert!(msg.body.contains("EXPIRED"));
    }

    #[test]
    fn test_level_ordering() {
        assert!(NotificationLevel::Critical > NotificationLevel::Urgent);
        assert!(NotificationLevel::Urgent > NotificationLevel::Warning);
        assert!(NotificationLevel::Warning > NotificationLevel::Reminder);
    }
}
