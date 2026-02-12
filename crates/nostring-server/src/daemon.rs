//! The daemon loop — periodically polls the blockchain and sends notifications.

use crate::config::ServerConfig;
use anyhow::{Context, Result};
use nostring_electrum::ElectrumClient;
use nostring_notify::{EmailConfig, NostrConfig, NotificationService, NotifyConfig, Threshold};
use nostring_watch::{WatchConfig, WatchEvent, WatchService};
use std::time::Duration;

/// Run the daemon loop. Blocks forever (until shutdown signal).
pub async fn run(config: ServerConfig) -> Result<()> {
    log::info!("NoString server starting…");
    log::info!("  Network:    {}", config.bitcoin.network);
    log::info!("  Electrum:   {}", config.bitcoin.electrum_url);
    log::info!(
        "  Interval:   {} seconds ({:.1} hours)",
        config.server.check_interval_secs,
        config.server.check_interval_secs as f64 / 3600.0
    );
    log::info!("  Data dir:   {}", config.server.data_dir.display());
    log::info!(
        "  Descriptor: {}…",
        &config.policy.descriptor[..config.policy.descriptor.len().min(60)]
    );

    // Ensure data directory exists
    std::fs::create_dir_all(&config.server.data_dir).with_context(|| {
        format!(
            "Failed to create data dir: {}",
            config.server.data_dir.display()
        )
    })?;

    let interval = Duration::from_secs(config.server.check_interval_secs);

    // Run first check immediately, then loop
    let mut first = true;
    loop {
        if !first {
            log::info!(
                "Sleeping {} seconds until next check…",
                config.server.check_interval_secs
            );
            tokio::time::sleep(interval).await;
        }
        first = false;

        match run_check_cycle(&config).await {
            Ok(()) => log::info!("Check cycle completed successfully."),
            Err(e) => log::error!("Check cycle failed: {:#}", e),
        }
    }
}

/// Execute a single check cycle: poll blockchain, evaluate events, send notifications.
pub async fn run_check_cycle(config: &ServerConfig) -> Result<()> {
    log::info!("Starting check cycle…");

    // Connect to Electrum
    let network = config.network();
    let client = ElectrumClient::new(&config.bitcoin.electrum_url, network).with_context(|| {
        format!(
            "Failed to connect to Electrum at {}",
            config.bitcoin.electrum_url
        )
    })?;

    // Set up the watch service
    let watch_state_path = config.server.data_dir.join("watch_state.json");
    let watch_config = WatchConfig {
        state_path: watch_state_path,
        poll_interval_secs: config.server.check_interval_secs,
        min_poll_interval_secs: 0, // Server manages its own interval via tokio::sleep
        warning_threshold_blocks: largest_threshold_blocks(&config.notifications.threshold_days),
    };

    let mut watch =
        WatchService::new(client, watch_config).context("Failed to create WatchService")?;

    // Add the policy if not already tracked
    if watch.get_policy(&config.policy.label).is_none() {
        watch
            .add_policy(
                &config.policy.label,
                &config.policy.descriptor,
                config.policy.timelock_blocks,
            )
            .context("Failed to add policy to WatchService")?;
        log::info!("Policy '{}' added to watch service.", config.policy.label);
    }

    // Poll
    let events = watch.poll().context("Watch poll failed")?;

    let height = watch.state().last_height.unwrap_or(0);
    log::info!("Block height: {}  |  Events: {}", height, events.len());

    // Process events
    let mut blocks_remaining: Option<i64> = None;

    for event in &events {
        match event {
            WatchEvent::UtxoAppeared {
                policy_id,
                outpoint,
                value,
                height,
            } => {
                log::info!(
                    "[{}] UTXO appeared: {} ({} sats) at height {}",
                    policy_id,
                    outpoint,
                    value,
                    height
                );
            }
            WatchEvent::UtxoSpent {
                policy_id,
                outpoint,
                spending_txid,
                spend_type,
            } => {
                log::warn!(
                    "[{}] UTXO spent: {} by {} (type: {:?})",
                    policy_id,
                    outpoint,
                    spending_txid,
                    spend_type
                );
            }
            WatchEvent::TimelockWarning {
                policy_id,
                blocks_remaining: br,
                days_remaining,
            } => {
                log::warn!(
                    "[{}] ⚠️  Timelock warning: {} blocks (~{:.1} days) remaining",
                    policy_id,
                    br,
                    days_remaining
                );
                blocks_remaining = Some(*br);
            }
            WatchEvent::PollError { message } => {
                log::error!("Poll error: {}", message);
            }
        }
    }

    // If no timelock warning event, compute blocks_remaining from state
    if blocks_remaining.is_none() {
        if let Some(policy) = watch.get_policy(&config.policy.label) {
            if let Some(br) = policy.blocks_until_expiry(height) {
                blocks_remaining = Some(br);
            }
        }
    }

    // Send notifications if we have blocks_remaining info
    if let Some(br) = blocks_remaining {
        send_notifications(config, br, height).await?;
    } else if height > 0 {
        log::info!("No active UTXOs — nothing to notify about.");
    }

    Ok(())
}

/// Send owner notifications (and heir delivery when critical).
async fn send_notifications(
    config: &ServerConfig,
    blocks_remaining: i64,
    current_height: u32,
) -> Result<()> {
    let days_remaining = blocks_remaining as f64 * 10.0 / 60.0 / 24.0;

    log::info!(
        "Timelock status: {} blocks (~{:.1} days) remaining",
        blocks_remaining,
        days_remaining
    );

    // Build notify config
    let nostr_config = config.notifications.nostr.as_ref().map(|n| NostrConfig {
        enabled: true,
        recipient_pubkey: n.owner_npub.clone(),
        relays: n.relays.clone(),
        secret_key: Some(n.service_key.clone()),
    });

    let email_config = config.notifications.email.as_ref().map(|e| EmailConfig {
        enabled: true,
        smtp_host: e.smtp_host.clone(),
        smtp_port: e.smtp_port,
        smtp_user: e.smtp_user.clone(),
        smtp_password: e.smtp_password.clone(),
        from_address: e.from_address.clone(),
        to_address: e.owner_email.clone(),
        plaintext: false,
    });

    // Build thresholds from config
    let thresholds: Vec<Threshold> = config
        .notifications
        .threshold_days
        .iter()
        .map(|&d| Threshold::days(d))
        .collect();

    let notify_config = NotifyConfig {
        thresholds,
        email: email_config.clone(),
        nostr: nostr_config,
    };

    let service = NotificationService::new(notify_config);

    // Owner notifications
    match service
        .check_and_notify(blocks_remaining, current_height)
        .await
    {
        Ok(Some(level)) => {
            log::info!("✉️  Owner notification sent: {:?}", level);
        }
        Ok(None) => {
            log::info!("No owner notification needed — timelock healthy.");
        }
        Err(e) => {
            log::error!("Owner notification error: {}", e);
        }
    }

    // Heir descriptor delivery — only when critical (≤1 day / ≤144 blocks)
    if blocks_remaining <= 144 {
        log::warn!("🔴 CRITICAL: Timelock ≤144 blocks — delivering descriptors to heirs…");
        deliver_to_heirs(config).await;
    }

    Ok(())
}

/// Deliver the descriptor backup to configured heirs.
async fn deliver_to_heirs(config: &ServerConfig) {
    let service_key = match config.notifications.nostr.as_ref() {
        Some(n) => &n.service_key,
        None => {
            log::warn!("Cannot deliver to heirs: no service key configured.");
            return;
        }
    };

    let relays = config
        .notifications
        .nostr
        .as_ref()
        .map(|n| n.relays.clone())
        .unwrap_or_default();

    // Build a simple descriptor backup JSON
    let backup = serde_json::json!({
        "descriptor": config.policy.descriptor,
        "network": config.bitcoin.network,
        "timelock_blocks": config.policy.timelock_blocks,
        "label": config.policy.label,
    });
    let backup_json = serde_json::to_string_pretty(&backup).unwrap_or_default();

    for heir in &config.notifications.heirs {
        let msg =
            nostring_notify::templates::generate_heir_delivery_message(&heir.label, &backup_json);

        // Nostr DM delivery
        if let Some(ref npub) = heir.npub {
            log::info!("Sending descriptor to {} via Nostr DM…", heir.label);
            match nostring_notify::nostr_dm::send_dm_to_recipient(service_key, npub, &relays, &msg)
                .await
            {
                Ok(_event_id) => log::info!("✅ Descriptor delivered to {} via Nostr", heir.label),
                Err(e) => log::error!("❌ Nostr delivery to {} failed: {}", heir.label, e),
            }
        }

        // Email delivery
        if let (Some(ref email_addr), Some(ref email_config)) =
            (&heir.email, &config.notifications.email)
        {
            log::info!("Sending descriptor to {} via email…", heir.label);
            let smtp_config = nostring_notify::EmailConfig {
                enabled: true,
                smtp_host: email_config.smtp_host.clone(),
                smtp_port: email_config.smtp_port,
                smtp_user: email_config.smtp_user.clone(),
                smtp_password: email_config.smtp_password.clone(),
                from_address: email_config.from_address.clone(),
                to_address: email_addr.clone(),
                plaintext: false,
            };
            match nostring_notify::smtp::send_email_to_recipient(&smtp_config, email_addr, &msg)
                .await
            {
                Ok(()) => log::info!("✅ Descriptor delivered to {} via email", heir.label),
                Err(e) => log::error!("❌ Email delivery to {} failed: {}", heir.label, e),
            }
        }
    }
}

/// Convert the largest threshold in days to blocks for warning_threshold_blocks.
fn largest_threshold_blocks(threshold_days: &[u32]) -> i64 {
    let max_days = threshold_days.iter().copied().max().unwrap_or(30);
    (max_days as i64) * 144
}
