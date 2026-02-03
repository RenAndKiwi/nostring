//! SMTP email sending

use crate::config::EmailConfig;
use crate::templates::NotificationMessage;
use crate::NotifyError;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// Send an email notification (async — safe for tokio runtimes)
pub async fn send_email(
    config: &EmailConfig,
    notification: &NotificationMessage,
) -> Result<(), NotifyError> {
    let email = build_message(&config.from_address, &config.to_address, notification)?;

    let mailer = build_async_transport(config)?;

    mailer
        .send(email)
        .await
        .map_err(|e| NotifyError::EmailFailed(format!("SMTP send failed: {}", e)))?;

    log::info!(
        "Email notification sent to {} (level: {:?})",
        config.to_address,
        notification.level
    );

    Ok(())
}

/// Send an email notification to an arbitrary recipient (async).
///
/// Unlike `send_email`, this overrides the `to_address` with a custom recipient.
/// Used for heir descriptor delivery.
pub async fn send_email_to_recipient(
    config: &EmailConfig,
    recipient_email: &str,
    notification: &NotificationMessage,
) -> Result<(), NotifyError> {
    let email = build_message(&config.from_address, recipient_email, notification)?;

    let mailer = build_async_transport(config)?;

    mailer
        .send(email)
        .await
        .map_err(|e| NotifyError::EmailFailed(format!("SMTP send failed: {}", e)))?;

    log::info!(
        "Email notification sent to {} (level: {:?})",
        recipient_email,
        notification.level
    );

    Ok(())
}

/// Build a `lettre::Message` from addresses and notification content.
fn build_message(
    from: &str,
    to: &str,
    notification: &NotificationMessage,
) -> Result<Message, NotifyError> {
    Message::builder()
        .from(
            from.parse()
                .map_err(|e| NotifyError::EmailFailed(format!("Invalid from address: {}", e)))?,
        )
        .to(to
            .parse()
            .map_err(|e| NotifyError::EmailFailed(format!("Invalid to address: {}", e)))?)
        .subject(&notification.subject)
        .body(notification.body.clone())
        .map_err(|e| NotifyError::EmailFailed(format!("Failed to build email: {}", e)))
}

/// Build an async SMTP transport from config.
fn build_async_transport(
    config: &EmailConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, NotifyError> {
    let creds = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());

    Ok(
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
            .map_err(|e| NotifyError::EmailFailed(format!("SMTP relay error: {}", e)))?
            .credentials(creds)
            .port(config.smtp_port)
            .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{generate_message, NotificationLevel};

    #[test]
    fn test_email_builder() {
        // Test that we can build a valid email message
        let notification = generate_message(NotificationLevel::Reminder, 25.0, 3600, 934000);

        let email = build_message("noreply@nostring.dev", "test@example.com", &notification);

        assert!(email.is_ok());
    }

    // Note: Actual SMTP tests require a real server
    // Use: cargo test --package nostring-notify -- --ignored
}
