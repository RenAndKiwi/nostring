//! Integration test: NIP-17 DM via local test relay.
//!
//! Requires: `node /Users/ai_sandbox/clawd/tools/nostr-test-relay.js 19867`
//! Run with: cargo test -p nostring-notify --test nip17_relay -- --ignored

use nostr_sdk::prelude::*;
use nostring_notify::nostr_dm::send_dm_to_recipient;
use nostring_notify::templates::{NotificationLevel, NotificationMessage};
use std::time::Duration;

#[tokio::test]
#[ignore] // requires local relay
async fn test_nip17_dm_roundtrip() {
    // Start relay (caller must run: node tools/nostr-test-relay.js 19867)
    let relay_url = "ws://127.0.0.1:19867";

    // Generate sender and recipient keys
    let sender_keys = Keys::generate();
    let recipient_keys = Keys::generate();

    let sender_secret = sender_keys.secret_key().to_bech32().unwrap();
    let recipient_npub = recipient_keys.public_key().to_bech32().unwrap();

    let notification = NotificationMessage {
        subject: "Test Notification".into(),
        body: "Your vault timelock is approaching.".into(),
        level: NotificationLevel::Warning,
    };

    // Send the DM
    let _event_id = send_dm_to_recipient(
        &sender_secret,
        &recipient_npub,
        &[relay_url.to_string()],
        &notification,
    )
    .await
    .expect("DM send failed");

    // Verify: recipient fetches gift-wrapped events
    let recipient_client = Client::new(recipient_keys.clone());
    recipient_client.add_relay(relay_url).await.unwrap();
    recipient_client.connect().await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Query for gift-wrap events (kind 1059)
    // NIP-59 gift wraps have a `p` tag with the recipient's pubkey
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .limit(10);

    let events = recipient_client
        .fetch_events(filter, Duration::from_secs(3))
        .await
        .expect("fetch failed");

    assert!(
        !events.is_empty(),
        "recipient should have received at least one gift-wrapped event"
    );

    // Verify we can unwrap and decrypt
    // NIP-59 gift wrap: outer event (kind 1059) contains a sealed rumor
    let gift_wrap = events.first().unwrap();
    assert_eq!(gift_wrap.kind, Kind::GiftWrap);

    // The gift wrap should be decryptable by the recipient
    let unwrapped = nip59::extract_rumor(&recipient_keys, gift_wrap).await;
    assert!(
        unwrapped.is_ok(),
        "recipient should be able to unwrap gift wrap: {:?}",
        unwrapped.as_ref().err()
    );

    let rumor = unwrapped.unwrap();
    assert!(
        rumor.rumor.content.contains("Test Notification"),
        "decrypted content should contain our message, got: {}",
        rumor.rumor.content
    );

    recipient_client.disconnect().await;
}
