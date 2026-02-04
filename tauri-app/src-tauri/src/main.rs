//! NoString Desktop Application
//!
//! Tauri-based desktop app for encrypted communications with inheritance.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod state;

use state::AppState;
use std::fs;
use tauri::Manager;

fn main() {
    // Security hardening: disable core dumps to prevent seed material leaking to disk
    nostring_core::memory::disable_core_dumps();

    // Initialize rustls CryptoProvider before any Nostr/TLS operations.
    // Without this, WebSocket connections via nostr-sdk will panic.
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    tauri::Builder::default()
        .setup(|app| {
            // Resolve the app data directory (platform-specific)
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            // Ensure the directory exists
            fs::create_dir_all(&app_data).expect("Failed to create app data directory");

            let db_path = app_data.join("nostring.db");
            log::info!("Database path: {}", db_path.display());

            // Create state from the database (loads persisted data)
            let state = AppState::from_db_path(db_path);
            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Seed / wallet management
            commands::create_seed,
            commands::validate_seed,
            commands::check_password_strength,
            commands::import_seed,
            commands::import_watch_only,
            commands::has_seed,
            commands::unlock_seed,
            commands::lock_wallet,
            // Policy status
            commands::get_policy_status,
            commands::refresh_policy_status,
            // Check-in
            commands::initiate_checkin,
            commands::complete_checkin,
            commands::broadcast_signed_psbt,
            // Heir management
            commands::add_heir,
            commands::list_heirs,
            commands::remove_heir,
            commands::get_heir,
            commands::validate_xpub,
            // Heir contact info (v0.2 - descriptor delivery)
            commands::set_heir_contact,
            commands::get_heir_contact,
            // Shamir shares
            commands::generate_codex32_shares,
            commands::combine_codex32_shares,
            // nsec inheritance (Shamir split + recovery)
            commands::split_nsec,
            commands::get_nsec_inheritance_status,
            commands::get_locked_shares,
            commands::recover_nsec,
            commands::revoke_nsec_inheritance,
            // Service key (notifications)
            commands::generate_service_key,
            commands::get_service_npub,
            // Notification management
            commands::configure_notifications,
            commands::get_notification_settings,
            commands::send_test_notification,
            commands::check_and_notify,
            // Descriptor backup
            commands::get_descriptor_backup,
            // Spend type detection
            commands::detect_spend_type,
            commands::get_spend_events,
            commands::check_heir_claims,
            // Pre-signed check-in stack (v0.3 auto check-in)
            commands::add_presigned_checkin,
            commands::get_presigned_checkin_status,
            commands::auto_broadcast_checkin,
            commands::invalidate_presigned_checkins,
            commands::delete_presigned_checkin,
            commands::generate_checkin_psbt_chain,
            // Relay storage (v0.3.1 — locked share relay backup)
            commands::publish_locked_shares_to_relays,
            commands::fetch_locked_shares_from_relays,
            commands::get_relay_publication_status,
            // Settings
            commands::get_network,
            commands::set_network,
            commands::get_electrum_url,
            commands::set_electrum_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
