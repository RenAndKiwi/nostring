//! Email sending via SMTP
//!
//! Provides direct SMTP email delivery for:
//! - Sending Shamir shares to heirs via email
//! - Sending descriptor backups
//! - General inheritance notifications
//!
//! For templated notification emails, see `nostring-notify::smtp`.

use crate::EmailError;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};

/// SMTP configuration for sending emails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// SMTP server hostname
    pub host: String,
    /// SMTP port (587 for STARTTLS, 465 for implicit TLS, 25 for plaintext)
    pub port: u16,
    /// SMTP username
    pub username: String,
    /// SMTP password
    pub password: String,
    /// Sender "From" address
    pub from_address: String,
    /// Use plaintext (no TLS) — only for local testing
    #[serde(default)]
    pub plaintext: bool,
}

/// An email to send.
#[derive(Debug, Clone)]
pub struct OutgoingEmail {
    /// Recipient email address
    pub to: String,
    /// Email subject line
    pub subject: String,
    /// Plain text body
    pub body: String,
}

/// Send a single email via SMTP.
pub async fn send_email(config: &SmtpConfig, email: &OutgoingEmail) -> Result<(), EmailError> {
    let message = Message::builder()
        .from(
            config
                .from_address
                .parse()
                .map_err(|e| EmailError::Smtp(format!("Invalid from address: {}", e)))?,
        )
        .to(email
            .to
            .parse()
            .map_err(|e| EmailError::Smtp(format!("Invalid to address: {}", e)))?)
        .subject(&email.subject)
        .body(email.body.clone())
        .map_err(|e| EmailError::Smtp(format!("Failed to build email: {}", e)))?;

    let transport = build_transport(config)?;

    transport
        .send(message)
        .await
        .map_err(|e| EmailError::Smtp(format!("Send failed: {}", e)))?;

    Ok(())
}

/// Send a Shamir share to an heir via email.
///
/// Wraps the share in a human-readable email explaining what it is
/// and how to use it.
pub async fn send_share_email(
    config: &SmtpConfig,
    heir_email: &str,
    heir_name: &str,
    share: &str,
    owner_npub: &str,
) -> Result<(), EmailError> {
    let subject = format!("NoString: Your inheritance share from {}", owner_npub);
    let body = format!(
        r#"Hello {heir_name},

You have been designated as an heir in a NoString inheritance plan.

This email contains your pre-distributed Shamir share. Keep it safe — you will
need it to recover the owner's Nostr identity if their inheritance activates.

YOUR SHARE (keep this secret):
{share}

OWNER'S IDENTITY:
{owner_npub}

WHAT TO DO WITH THIS:
1. Store this share securely (password manager, printed in a safe, etc.)
2. Do NOT share it with anyone else
3. If the owner's inheritance activates, you will receive a second share
4. Combine both shares in NoString to recover their Nostr identity

WHAT IS THIS?
This is one piece of a Shamir's Secret Sharing scheme. A single share reveals
nothing about the secret. Only when combined with enough other shares can the
original secret be reconstructed.

Learn more: https://nostring.xyz

— NoString (automated message)
"#,
        heir_name = heir_name,
        share = share,
        owner_npub = owner_npub,
    );

    send_email(
        config,
        &OutgoingEmail {
            to: heir_email.to_string(),
            subject,
            body,
        },
    )
    .await
}

/// Send a descriptor backup to an heir via email.
///
/// This is the critical inheritance delivery — contains everything
/// an heir needs to claim their Bitcoin.
pub async fn send_descriptor_email(
    config: &SmtpConfig,
    heir_email: &str,
    heir_name: &str,
    descriptor_backup: &str,
) -> Result<(), EmailError> {
    let subject = "NoString: Inheritance descriptor backup — ACTION REQUIRED".to_string();
    let body = format!(
        r#"Hello {heir_name},

IMPORTANT: A NoString inheritance timelock is approaching expiration.

The owner has not checked in, and the inheritance policy is activating.
Below is the descriptor backup you need to claim your inheritance.

=== DESCRIPTOR BACKUP ===
{descriptor_backup}
=== END BACKUP ===

WHAT TO DO:
1. Open NoString (or any miniscript-compatible wallet like Liana or Electrum)
2. Import the descriptor from the backup above
3. Wait for your timelock to mature (check the backup for your specific timing)
4. Sign and broadcast your claim transaction

If you also received Shamir shares, combine them to recover the owner's
Nostr identity.

This is an automated message from the NoString inheritance system.
No one else has access to this information — it was encrypted in transit.

— NoString
"#,
        heir_name = heir_name,
        descriptor_backup = descriptor_backup,
    );

    send_email(
        config,
        &OutgoingEmail {
            to: heir_email.to_string(),
            subject,
            body,
        },
    )
    .await
}

/// Send multiple emails (e.g., to all heirs).
pub async fn send_bulk(
    config: &SmtpConfig,
    emails: &[OutgoingEmail],
) -> Vec<Result<(), EmailError>> {
    let mut results = Vec::with_capacity(emails.len());
    for email in emails {
        results.push(send_email(config, email).await);
    }
    results
}

fn build_transport(config: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, EmailError> {
    let creds = Credentials::new(config.username.clone(), config.password.clone());

    if config.plaintext {
        Ok(
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
                .credentials(creds)
                .port(config.port)
                .build(),
        )
    } else {
        Ok(AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
            .map_err(|e| EmailError::Smtp(format!("SMTP relay error: {}", e)))?
            .credentials(creds)
            .port(config.port)
            .build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outgoing_email_construction() {
        let email = OutgoingEmail {
            to: "heir@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Hello".to_string(),
        };
        assert_eq!(email.to, "heir@example.com");
    }

    #[test]
    fn test_smtp_config() {
        let config = SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            username: "user".to_string(),
            password: "pass".to_string(),
            from_address: "noreply@nostring.dev".to_string(),
            plaintext: false,
        };
        assert_eq!(config.port, 587);
        assert!(!config.plaintext);
    }
}
