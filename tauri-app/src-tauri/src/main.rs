//! NoString Desktop Application
//!
//! Tauri-based desktop app for encrypted email with inheritance.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// TODO: Add Tauri commands:
// - Seed management (generate, import, encrypt)
// - Key derivation (Nostr, Bitcoin)
// - Email operations (send, fetch, decrypt)
// - Inheritance setup (policy, heirs, timelock)
// - Check-in (reset timelock)
// - Shamir (split, verify shares)
