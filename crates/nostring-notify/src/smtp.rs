//! SMTP email sending

use crate::config::EmailConfig;
use crate::templates::NotificationMessage;
use crate::NotifyError;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// Send an email notification
pub async fn send_email(
    config: &EmailConfig,
    notification: &NotificationMessage,
) -> Result<(), NotifyError> {
    // Build the email message
    let email = Message::builder()
        .from(
            config
                .from_address
                .parse()
                .map_err(|e| NotifyError::EmailFailed(format!("Invalid from address: {}", e)))?,
        )
        .to(config
            .to_address
            .parse()
            .map_err(|e| NotifyError::EmailFailed(format!("Invalid to address: {}", e)))?)
        .subject(&notification.subject)
        .body(notification.body.clone())
        .map_err(|e| NotifyError::EmailFailed(format!("Failed to build email: {}", e)))?;

    // Configure SMTP transport
    let creds = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());

    let mailer = SmtpTransport::relay(&config.smtp_host)
        .map_err(|e| NotifyError::EmailFailed(format!("SMTP relay error: {}", e)))?
        .credentials(creds)
        .port(config.smtp_port)
        .build();

    // Send the email
    mailer
        .send(&email)
        .map_err(|e| NotifyError::EmailFailed(format!("SMTP send failed: {}", e)))?;

    log::info!(
        "Email notification sent to {} (level: {:?})",
        config.to_address,
        notification.level
    );

    Ok(())
}

/// Send an email notification to an arbitrary recipient using the configured SMTP.
///
/// Unlike `send_email`, this overrides the `to_address` with a custom recipient.
/// Used for heir descriptor delivery.
pub async fn send_email_to_recipient(
    config: &EmailConfig,
    recipient_email: &str,
    notification: &NotificationMessage,
) -> Result<(), NotifyError> {
    let email = Message::builder()
        .from(
            config
                .from_address
                .parse()
                .map_err(|e| NotifyError::EmailFailed(format!("Invalid from address: {}", e)))?,
        )
        .to(recipient_email
            .parse()
            .map_err(|e| NotifyError::EmailFailed(format!("Invalid to address: {}", e)))?)
        .subject(&notification.subject)
        .body(notification.body.clone())
        .map_err(|e| NotifyError::EmailFailed(format!("Failed to build email: {}", e)))?;

    let creds = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());

    let mailer = SmtpTransport::relay(&config.smtp_host)
        .map_err(|e| NotifyError::EmailFailed(format!("SMTP relay error: {}", e)))?
        .credentials(creds)
        .port(config.smtp_port)
        .build();

    mailer
        .send(&email)
        .map_err(|e| NotifyError::EmailFailed(format!("SMTP send failed: {}", e)))?;

    log::info!(
        "Email notification sent to {} (level: {:?})",
        recipient_email,
        notification.level
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{generate_message, NotificationLevel};

    #[test]
    fn test_email_builder() {
        // Test that we can build a valid email message
        let notification = generate_message(NotificationLevel::Reminder, 25.0, 3600, 934000);

        let email = Message::builder()
            .from("noreply@nostring.dev".parse().unwrap())
            .to("test@example.com".parse().unwrap())
            .subject(&notification.subject)
            .body(notification.body.clone());

        assert!(email.is_ok());
    }

    // Note: Actual SMTP tests require a real server
    // Use: cargo test --package nostring-notify -- --ignored
}
