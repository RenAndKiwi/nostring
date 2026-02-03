# NoString Roadmap

## v0.1 — Foundation ✅

- [x] BIP-39 seed generation, import, encryption
- [x] Watch-only wallet (xpub import)
- [x] Heir management (add/remove/list by xpub + fingerprint)
- [x] Inheritance policy with miniscript descriptors (`or_d(pk(owner), and_v(v:pk(heir), older(N)))`)
- [x] Cascade policies (multiple heirs with staggered timelocks)
- [x] Check-in via PSBT (create → sign externally → broadcast)
- [x] Electrum backend for blockchain queries
- [x] Codex32 Shamir secret sharing (seed backup)
- [x] nsec Shamir inheritance (split nsec for Nostr identity inheritance)
- [x] Notification service: owner reminders via Nostr DM and email
- [x] Service key generation (Nostr keypair for sending notifications)
- [x] Descriptor backup generation (JSON export with locked shares)
- [x] SQLite persistence (config, heirs, check-in log)
- [x] Tauri desktop app with full command set

## v0.2 — Automated Heir Delivery & Spend Detection 🚧

### Automated Descriptor Delivery to Heirs ✅
- [x] Heir contact fields: optional `npub` and `email` on heirs table (SQLite migration)
- [x] `set_heir_contact` / `get_heir_contact` Tauri commands
- [x] `list_heirs` now returns contact info from DB
- [x] Heir delivery template (`generate_heir_delivery_message`) in nostring-notify
- [x] `send_dm_to_recipient` — send Nostr DM to arbitrary npub (for heir delivery)
- [x] `send_email_to_recipient` — send email to arbitrary address (for heir delivery)
- [x] Escalation in `check_and_notify`: when timelock is critical (≤144 blocks / ≤1 day),
      automatically deliver the full descriptor backup to all heirs with configured contacts
- [x] Delivery log table + rate limiting (24h cooldown per heir per channel)
- [x] `log_delivery` / `can_deliver_to_heir` state helpers
- [x] Comprehensive tests: delivery log CRUD, heir contact update, cross-connection persistence,
      heir delivery template generation

### Spend Type Detection (WIP)
- [x] Witness analysis module (`spend_analysis.rs`) — detect owner check-in vs heir claim
- [x] Timing analysis fallback (pre-expiry spend = definitively owner)
- [x] Combined analysis with confidence scoring
- [x] Spend events table in SQLite
- [ ] Integrate spend detection commands into Tauri handler
- [ ] Frontend spend event display

### nsec Inheritance Improvements ✅
- [x] `revoke_nsec_inheritance` command
- [x] Re-split detection (`was_resplit` / `previous_npub` in split result)

## v0.3 — Self-Hosting & Server 🚧

### Docker Self-Hosting ✅
- [x] `nostring-server` headless binary crate — monitors inheritance UTXOs 24/7
- [x] Reuses all library crates (watch, notify, electrum, inherit) without Tauri
- [x] TOML configuration with environment variable overrides
- [x] Daemon mode: periodic blockchain polling on configurable interval
- [x] Single-check mode (`--check`) for cron-job usage
- [x] Config validation mode (`--validate`)
- [x] Owner notifications via Nostr DM and/or email at configurable thresholds
- [x] Automatic heir descriptor delivery when timelock critical (≤144 blocks)
- [x] Multi-stage Dockerfile (Rust builder → Debian slim runtime)
- [x] `docker-compose.yml` with volume mounts, resource limits, read-only filesystem
- [x] Non-root container user, outbound-only networking
- [x] Example config (`nostring-server.example.toml`)
- [x] Self-hosting documentation (`docs/SELF_HOSTING.md`)
- [x] Config parsing tests (10 tests)
- [x] Graceful shutdown via Ctrl-C / SIGTERM

### Nostr Relay Storage for Locked Shares (Phase 8.3) ✅
- [x] `nostr_relay.rs` module in nostring-notify: publish/fetch encrypted shares
- [x] NIP-44 encryption (modern, with padding) with NIP-04 fallback
- [x] Multi-relay redundancy: publish to damus, nostr.band, nos.lol
- [x] `publish_locked_shares_to_relays` Tauri command: encrypts each locked share to each heir's npub
- [x] `fetch_locked_shares_from_relays` Tauri command: heir recovery from relays
- [x] `get_relay_publication_status` Tauri command: view last publication info
- [x] `relay_publications` SQLite table for tracking publication status
- [x] Split ID generation for grouping share publications
- [x] Comprehensive tests: NIP-44/NIP-04 roundtrip, payload serialization, multi-share encrypt/decrypt, DB CRUD (14 new tests)

### Pre-signed Check-in Stack (Auto Check-in) ✅
- [x] SQLite table: `presigned_checkins` (id, psbt_base64, sequence_index, spending info, broadcast/invalidation tracking)
- [x] `add_presigned_checkin` — import signed PSBTs from hardware wallet
- [x] `get_presigned_checkin_status` — view stack status (active count, low warning)
- [x] `auto_broadcast_checkin` — automatically broadcast next PSBT when timelock approaches threshold
- [x] `invalidate_presigned_checkins` — mark all active PSBTs as stale (e.g., after manual check-in)
- [x] `delete_presigned_checkin` — remove a specific unbroadcast PSBT
- [x] `generate_checkin_psbt_chain` — create N sequential unsigned PSBTs for batch signing on hardware wallet
- [x] Manual check-in (`broadcast_signed_psbt`) auto-invalidates the pre-signed stack
- [x] Sequential chain: each PSBT spends the output of the previous one
- [x] Low-stack warning when < 2 active PSBTs remain
- [x] Comprehensive tests: add/list, next selection, broadcast lifecycle, invalidation (all/after),
      delete, clear, mixed states, persistence across connections (10 tests)

### Planned
- [ ] Frontend: heir setup form with optional npub + email fields
- [ ] Frontend: delivery log viewer (show what was sent to whom and when)
- [ ] Frontend: "Pre-sign Check-ins" section with PSBT generation + import workflow
- [ ] Frontend: button in Settings to publish locked shares to relays
- [ ] NIP-17 (gift-wrapped DMs) support as alternative to NIP-04
- [ ] Encrypted descriptor backup (encrypt with heir's npub before sending)
- [ ] Push notification integration (mobile companion app)
- [ ] Option A: Hot key mode for fully automated check-in (advanced users — encrypted seed on device)

## Security Model

### Descriptor Delivery
- Descriptor backup contains **locked Shamir shares** — this is intentional (it's the inheritance mechanism)
- Delivery only triggers at **critical** threshold (≤1 day / ≤144 blocks remaining)
- **24-hour cooldown** per heir per channel prevents spam if check runs repeatedly
- All delivery attempts (success + failure) are logged with timestamps
- The descriptor alone is not sufficient to spend — heirs still need their signing device

### Pre-signed Check-in Stack
- Pre-signed PSBTs contain **fully signed transactions** — treat as sensitive data at rest
- Each PSBT in the chain spends the output of the previous one (sequential dependency)
- If the owner checks in manually, **all remaining pre-signed PSBTs become invalid** (the UTXO they spend no longer exists)
- The app automatically invalidates the stack when a manual check-in is broadcast
- Stack depth is limited to 12 PSBTs — practical limit for sequential hardware wallet signing
- Low-stack warning at < 2 remaining PSBTs prompts the user to generate more

### Notification Architecture
- **Service key**: a dedicated Nostr keypair (not the owner's keys) sends all notifications
- **Owner notifications**: reminders at 30d, 7d, 1d, 0d thresholds
- **Heir notifications**: only at critical threshold, includes full descriptor backup
- Both Nostr DM (NIP-04 encrypted) and SMTP email channels supported

### Relay Storage (Phase 8.3)
- Locked shares published to relays are **NIP-44 encrypted** to each heir's npub
- Even if relays are scraped, encrypted blobs are useless without the heir's nsec
- Even with decrypted locked shares, reconstruction requires meeting the **Shamir threshold**
  (locked shares alone can't reconstruct without the heir's pre-distributed share)
- Multi-relay redundancy: published to 3+ relays, succeeds if any 1 accepts
- Relay downtime is expected — this is defense-in-depth alongside the descriptor backup file
- Split IDs group related publications for easy tracking and de-duplication
