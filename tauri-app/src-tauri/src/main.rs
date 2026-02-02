//! NoString Desktop Application
//!
//! Tauri-based desktop app for encrypted communications with inheritance.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // Seed management
            commands::create_seed,
            commands::validate_seed,
            commands::import_seed,
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
            // Settings
            commands::get_electrum_url,
            commands::set_electrum_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
