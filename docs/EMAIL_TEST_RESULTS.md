# Email Notification Test Results

**Date:** 2026-02-03
**Status:** ✅ All tests passing

## Setup

### Test SMTP Server: MailHog
- **Install:** `brew install mailhog`
- **Run:** `/usr/local/opt/mailhog/bin/MailHog -api-bind-addr 127.0.0.1:8025 -smtp-bind-addr 127.0.0.1:1025 -ui-bind-addr 127.0.0.1:8025`
- **Web UI:** http://127.0.0.1:8025
- **SMTP:** 127.0.0.1:1025 (plaintext, no auth required)
- **API:** http://127.0.0.1:8025/api/v2/messages

### Why MailHog (not Proton Bridge)
Proton Mail doesn't expose standard SMTP. Proton Bridge is required but:
- It's a GUI application requiring interactive login
- Generates its own SMTP password (not the account password)
- Overkill for automated testing

MailHog captures all SMTP traffic locally — perfect for verifying email content and delivery logic without external dependencies.

## Test Results

### Test 5a: Direct `send_email()` 
- Sent a Warning-level notification via `nostring_notify::smtp::send_email()`
- **Result:** ✅ Email delivered to MailHog
- **Subject:** `⚠️ NoString: Check-in WARNING (8 days remaining)`
- **To:** `rensovereign@proton.me`

### Test 5b: `NotificationService.check_and_notify()`
Full notification service flow with threshold detection:

| Blocks Remaining | Days | Expected Level | Result |
|-----------------|------|---------------|--------|
| 720 | ~5 days | Warning | ✅ Warning triggered |
| 100 | ~0.7 days | Urgent | ✅ Urgent triggered |
| -10 | expired | Critical | ✅ Critical triggered |

### Test 5c: MailHog API Verification
Verified all 4 emails captured by MailHog API:

1. `🔴 NoString: CRITICAL - Timelock EXPIRED or expiring NOW!` → rensovereign@proton.me
2. `🚨 NoString: URGENT - Check-in expires in 16.7 hours!` → rensovereign@proton.me
3. `⚠️ NoString: Check-in WARNING (5 days remaining)` → rensovereign@proton.me
4. `⚠️ NoString: Check-in WARNING (8 days remaining)` → rensovereign@proton.me

### Test 5d: Heir Descriptor Delivery
- Sent heir descriptor backup delivery email to `alice_heir@example.com`
- **Result:** ✅ Email sent with full descriptor backup JSON embedded
- **Subject:** `🔑 NoString: Inheritance Descriptor Backup Delivery`

## Code Changes

### `crates/nostring-notify/src/config.rs`
- Added `plaintext: bool` field to `EmailConfig` (default: `false`)
- Allows local test servers without TLS

### `crates/nostring-notify/src/smtp.rs`
- Added `builder_dangerous()` transport path when `config.plaintext == true`
- Production path (`relay()` with TLS) unchanged

### `tests/e2e/live_integration.rs`
- Added `test_email_notification_mailhog` integration test
- Tests: direct send, full service flow, MailHog API verification, heir delivery

## Running the Tests

```bash
# Start MailHog
/usr/local/opt/mailhog/bin/MailHog -api-bind-addr 127.0.0.1:8025 \
  -smtp-bind-addr 127.0.0.1:1025 -ui-bind-addr 127.0.0.1:8025 &

# Run email test
cargo test -p nostring-e2e --test live_integration test_email_notification_mailhog \
  -- --ignored --nocapture
```

## Production Email Setup

For production use with Proton Mail:

1. Install Proton Mail Bridge (`brew install --cask protonmail-bridge`)
2. Log in via Bridge GUI
3. Use Bridge-generated SMTP credentials:
   - Host: `127.0.0.1`
   - Port: `1025` (Bridge SMTP)
   - User: `rensovereign@proton.me`
   - Password: (Bridge-generated, NOT account password)
   - `plaintext: false` (Bridge supports STARTTLS)

For other SMTP providers:
- Gmail: `smtp.gmail.com:587` with app password
- Any standard SMTP: configure host/port/credentials in `EmailConfig`
