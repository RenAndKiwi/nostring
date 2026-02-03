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
    tauri::Builder::default()
        .setup(|app| {
            // Resolve the app data directory (platform-specific)
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            // Ensure the directory exists
            fs::create_dir_all(&app_data)
                .expect("Failed to create app data directory");

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
            // Shamir shares
            commands::generate_codex32_shares,
            commands::combine_codex32_shares,
            // Service key (notifications)
            commands::generate_service_key,
            commands::get_service_npub,
            // Notification management
            commands::configure_notifications,
            commands::get_notification_settings,
            commands::send_test_notification,
            commands::check_and_notify,
            // Settings
            commands::get_electrum_url,
            commands::set_electrum_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
