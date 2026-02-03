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

## v0.3 — Planned

- [ ] Frontend: heir setup form with optional npub + email fields
- [ ] Frontend: delivery log viewer (show what was sent to whom and when)
- [ ] NIP-17 (gift-wrapped DMs) support as alternative to NIP-04
- [ ] Encrypted descriptor backup (encrypt with heir's npub before sending)
- [ ] Multi-relay delivery confirmation (require N-of-M relays to accept)
- [ ] Push notification integration (mobile companion app)
- [ ] Automated periodic `check_and_notify` via background scheduler
- [ ] PSBT auto-signing option for check-ins (with hardware wallet bridge)

## Security Model

### Descriptor Delivery
- Descriptor backup contains **locked Shamir shares** — this is intentional (it's the inheritance mechanism)
- Delivery only triggers at **critical** threshold (≤1 day / ≤144 blocks remaining)
- **24-hour cooldown** per heir per channel prevents spam if check runs repeatedly
- All delivery attempts (success + failure) are logged with timestamps
- The descriptor alone is not sufficient to spend — heirs still need their signing device

### Notification Architecture
- **Service key**: a dedicated Nostr keypair (not the owner's keys) sends all notifications
- **Owner notifications**: reminders at 30d, 7d, 1d, 0d thresholds
- **Heir notifications**: only at critical threshold, includes full descriptor backup
- Both Nostr DM (NIP-04 encrypted) and SMTP email channels supported
