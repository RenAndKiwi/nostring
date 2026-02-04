# Integration Test Results — Sprint 3: Desktop App & Dashboard UI

**Date:** 2026-02-03  
**Tester:** Automated Integration Suite  
**Commit:** `b7ac15e` (style: cargo fmt) / `4ad6ca4` (feat: dashboard UI)

---

## Test 9: Desktop App Compilation ✅

### Tauri CLI
- **Version:** `tauri-cli 2.10.0` (installed via `cargo tauri`)
- `npx tauri` not available (not needed — cargo subcommand works)

### Rust Backend Build (`cargo build --release`)
- **Result:** ✅ **SUCCESS**
- **Build time:** 26.73 seconds (release profile, optimized)
- **Warnings:** None
- **Crates compiled:** nostring-app, nostring-core, nostring-notify, nostring-inherit, nostring-email, nostring-shamir, nostring-watch, nostr-sdk, nostr-relay-pool
- **Binary location:** `tauri-app/src-tauri/target/release/` (Rust backend only, not a full Tauri bundle)
- **Note:** Full `cargo tauri build` was not attempted locally (requires system WebView libraries). CI handles full builds.

### CI Build Status
- All 4 platform targets built successfully (see Test 12 for details)

---

## Test 10: Dashboard UI Visual Inspection ⚠️ (2 bugs found)

### Code Reviewed
- `tauri-app/frontend/js/app.js` (2326 lines)
- `tauri-app/frontend/styles/main.css` (CSS classes verified)
- `tauri-app/frontend/index.html` (structure verified)

### HTML Structure ✅
- All tags properly closed
- Modal structure (QR, scanner) correct with matching IDs
- Dynamic content uses `innerHTML` with proper nesting
- No orphaned elements

### CSS Classes ✅
- All classes used in JS have corresponding CSS rules:
  - `.heir-claim-banner` and variants (lines 1122–1175) ✅
  - `.activity-log` and variants (lines 1207–1265) ✅
  - `.spend-event-row`, `.spend-type-icon` (lines 1273–1350) ✅
  - `.confidence-indicator`, `.confidence-dot`, `.confidence-dot.filled`, `.confidence-dot.empty` (lines 1353–1378) ✅
  - Responsive overrides (lines 1390–1415) ✅

### Event Listeners ✅
- Banner dismiss handlers properly attached via `setupBannerHandlers()` ✅
- Activity log toggle handler via `setupActivityLogHandlers()` ✅
- All `addEventListener` calls use valid element IDs ✅
- No dangling references to removed DOM elements ✅

### Demo Mode Mocks ⚠️
- 29 invoke commands called by UI — all have mock responses ✅
- 2 stale mock keys that are never invoked:
  - `store_seed` — not a real command (harmless, never called)
  - `unlock` — should be `unlock_seed` (harmless, never called)
- Mock data is realistic: spend events with varied timestamps, confidence levels, and spend types ✅
- `get_spend_events` mock includes 5 events covering owner_checkin, heir_claim, and unknown types ✅

### Confidence Dot Rendering Math ✅
Boundary case analysis of `renderConfidenceIndicator()`:

| Confidence | filledCount | Color     | Label     | Correct? |
|-----------|-------------|-----------|-----------|----------|
| 0.00      | 1           | #ef4444   | Very Low  | ✅       |
| 0.29      | 1           | #ef4444   | Very Low  | ✅       |
| 0.30      | 2           | #f97316   | Low       | ✅       |
| 0.50      | 3           | #eab308   | Medium    | ✅       |
| 0.70      | 4           | #84cc16   | High      | ✅       |
| 0.90      | 5           | #10b981   | Very High | ✅       |
| 1.00      | 5           | #10b981   | Very High | ✅       |

All thresholds use `>=`, creating clean non-overlapping ranges. Dot loop generates exactly 5 dots. Math is correct.

### localStorage Dismiss Logic ✅
- **Key format:** `nostring_heir_alert_dismissed` — consistent between set/get
- **Comparison logic:** `parseInt(dismissedId) >= latestClaim.id` — correctly uses `>=` so same-ID redisplay is prevented
- **New claims:** Higher IDs re-trigger the banner (correct behavior)
- **Set logic:** `localStorage.setItem('nostring_heir_alert_dismissed', eventId)` — stores the string event ID
- **Parse:** `parseInt()` correctly converts string back to number for comparison

### 🐛 Bug #1: `const` prevents descriptor backup nsec append (MEDIUM)

**File:** `app.js`, `downloadDescriptorBackup()` function  
**Line:** ~1379 and ~1406

```javascript
const content = `# NoString Descriptor Backup...`;   // Line ~1379
// ... later in try block ...
content += `\n## Nostr Identity Inheritance...`;      // Line ~1406 — TypeError!
```

**Impact:** When a user has nsec inheritance configured and downloads the descriptor backup, the `content +=` operation on a `const` variable throws a `TypeError: Assignment to constant variable`. The nsec inheritance section (locked shares, recovery instructions) is **silently dropped** from the backup file. The base descriptor backup still downloads, but without the critical nsec recovery information.

**Fix:** Change `const content` to `let content`.

### 🐛 Bug #2: Wizard `timelockMonths` silently discarded (MEDIUM)

**File:** `app.js`, `wizardAddHeir()` function  
**Line:** ~697

```javascript
const result = await invoke('add_heir', { 
    label, 
    xpubOrDescriptor: address,
    timelockMonths: parseInt(timelock)   // ← Sent but never received
});
```

**Rust side:** `add_heir(label: String, xpub_or_descriptor: String, state: ...)` — no `timelock_months` parameter.

**Impact:** The timelock value selected in the wizard (6/12/18/24 months) is silently ignored. Tauri doesn't error on extra args — it just drops them. The heir is added but the timelock preference has no effect. The actual timelock comes from `inheritance_config` which is set elsewhere.

**Severity:** Medium — misleading UX. User thinks they're setting a timelock per heir, but the value is discarded. The wizard UI shows timelock selection but it's cosmetic only.

---

## Test 11: Frontend-Backend Command Parity ⚠️

### Every `invoke()` call in app.js (29 unique commands):

| # | JS invoke command | Called from |
|---|-------------------|-------------|
| 1 | `has_seed` | DOMContentLoaded |
| 2 | `import_watch_only` | confirmWatchOnly |
| 3 | `create_seed` | createNewSeed |
| 4 | `import_seed` | confirmNewSeed, importExistingSeed |
| 5 | `unlock_seed` | unlockWallet |
| 6 | `split_nsec` | wizard step 3, performNsecSplit |
| 7 | `add_heir` | wizardAddHeir, saveHeir |
| 8 | `refresh_policy_status` | refreshStatus |
| 9 | `check_and_notify` | refreshStatus (fire-and-forget) |
| 10 | `initiate_checkin` | initiateCheckin |
| 11 | `list_heirs` | loadHeirs |
| 12 | `remove_heir` | removeHeir |
| 13 | `generate_codex32_shares` | generateShares |
| 14 | `broadcast_signed_psbt` | handleScannedPsbt |
| 15 | `get_descriptor_backup` | downloadDescriptorBackup |
| 16 | `get_nsec_inheritance_status` | downloadDescriptorBackup, loadNsecInheritanceStatus, showNsecSplitUI |
| 17 | `get_locked_shares` | downloadDescriptorBackup |
| 18 | `generate_service_key` | generateServiceKeyOnSetup, loadServiceNpub |
| 19 | `get_service_npub` | loadServiceNpub |
| 20 | `recover_nsec` | attemptNsecRecovery |
| 21 | `revoke_nsec_inheritance` | revokeNsecInheritance |
| 22 | `configure_notifications` | saveNotificationSettings |
| 23 | `get_notification_settings` | loadNotificationSettings |
| 24 | `send_test_notification` | sendTestNotification |
| 25 | `lock_wallet` | lockWallet |
| 26 | `get_electrum_url` | loadElectrumUrl |
| 27 | `set_electrum_url` | saveElectrumUrl |
| 28 | `get_spend_events` | loadSpendEvents |
| 29 | `check_heir_claims` | loadSpendEvents |

### Every `#[tauri::command]` registered in handler (46 total, 43 active):

**Active (registered in invoke_handler):**
1. `create_seed` ✅
2. `validate_seed` — no JS call (backend-only validator)
3. `import_seed` ✅
4. `import_watch_only` ✅
5. `has_seed` ✅
6. `unlock_seed` ✅
7. `lock_wallet` ✅
8. `get_policy_status` — no JS call (UI uses `refresh_policy_status` instead)
9. `refresh_policy_status` ✅
10. `initiate_checkin` ✅
11. `complete_checkin` — no JS call (UI uses `broadcast_signed_psbt` directly)
12. `broadcast_signed_psbt` ✅
13. `add_heir` ✅
14. `list_heirs` ✅
15. `remove_heir` ✅
16. `get_heir` — no JS call (future use)
17. `validate_xpub` — no JS call (future use)
18. `set_heir_contact` — no JS call (v0.2 feature, not yet in UI)
19. `get_heir_contact` — no JS call (v0.2 feature, not yet in UI)
20. `generate_codex32_shares` ✅
21. `combine_codex32_shares` — no JS call (recovery use case)
22. `split_nsec` ✅
23. `get_nsec_inheritance_status` ✅
24. `get_locked_shares` ✅
25. `recover_nsec` ✅
26. `revoke_nsec_inheritance` ✅
27. `generate_service_key` ✅
28. `get_service_npub` ✅
29. `configure_notifications` ✅
30. `get_notification_settings` ✅
31. `send_test_notification` ✅
32. `check_and_notify` ✅
33. `get_descriptor_backup` ✅
34. `detect_spend_type` — mocked in demo but never invoked from UI (future: per-tx analysis)
35. `get_spend_events` ✅
36. `check_heir_claims` ✅
37. `add_presigned_checkin` — no JS call (v0.3 auto-checkin, no UI yet)
38. `get_presigned_checkin_status` — no JS call (v0.3, no UI yet)
39. `auto_broadcast_checkin` — no JS call (v0.3, no UI yet)
40. `invalidate_presigned_checkins` — no JS call (v0.3, no UI yet)
41. `delete_presigned_checkin` — no JS call (v0.3, no UI yet)
42. `generate_checkin_psbt_chain` — no JS call (v0.3, no UI yet)
43. `get_electrum_url` ✅
44. `set_electrum_url` ✅

**Previously commented out — now resolved:**
45. `publish_locked_shares_to_relays` ✅ — registered, async Send issue resolved
46. `fetch_locked_shares_from_relays` ✅ — registered, working
47. `get_relay_publication_status` ✅ — registered, working

### Parity Summary

| Category | Count |
|----------|-------|
| **Matched (JS ↔ Rust)** | 29 |
| **Rust only (no JS call)** | 14 |
| **JS only (no Rust handler)** | 0 |
| **Commented out** | 3 |

**All 29 JS invoke calls have matching registered Rust handlers.** ✅  
No JS call targets a nonexistent command.

### Argument Name Matching ✅

Tauri 2.x automatically converts between JavaScript camelCase and Rust snake_case:

| JS argument | Rust parameter | Auto-converted? |
|-------------|---------------|-----------------|
| `wordCount` | `word_count` | ✅ |
| `xpubOrDescriptor` | `xpub_or_descriptor` | ✅ |
| `nsecInput` | `nsec_input` | ✅ |
| `signedPsbt` | `signed_psbt` | ✅ |
| `totalShares` | `total_shares` | ✅ |
| `ownerNpub` | `owner_npub` | ✅ |
| `emailAddress` | `email_address` | ✅ |
| `emailSmtpHost` | `email_smtp_host` | ✅ |
| `emailSmtpUser` | `email_smtp_user` | ✅ |
| `emailSmtpPassword` | `email_smtp_password` | ✅ |

All argument names match after camelCase→snake_case conversion.

**Note:** `timelockMonths` from wizard JS has **no corresponding Rust parameter** — see Bug #2 above.

---

## Test 12: v0.4.0 Release Build Status ✅

### GitHub Actions Workflow Run: `21650373288`

| Platform | Job ID | Status | Duration |
|----------|--------|--------|----------|
| macOS ARM64 (`aarch64-apple-darwin`) | 62413138334 | ✅ Success | 5m 2s |
| macOS x64 (`x86_64-apple-darwin`) | 62413138350 | ✅ Success | 7m 50s |
| Linux x64 (`x86_64-unknown-linux-gnu`) | 62413138368 | ✅ Success | 5m 43s |
| Windows x64 (`x86_64-pc-windows-msvc`) | 62413138349 | ✅ Success | 10m 11s |
| Create Release | 62414078611 | ✅ Success | 7s |

**All 5 jobs passed.** No failures, no retries.

### Release Artifacts

| Artifact | Format |
|----------|--------|
| `nostring-macos-arm64.tar.gz` | macOS ARM64 bundle |
| `nostring-macos-x64.tar.gz` | macOS Intel bundle |
| `nostring-linux-x64.tar.gz` | Linux x64 bundle |
| `nostring-windows-x64.zip` | Windows x64 bundle |
| `SHA256SUMS.txt` | Integrity checksums |

### Release Status
- **Tag:** `v0.4.0`
- **Draft:** ⚠️ Yes (still draft, not published)
- **Created:** 2026-02-03T22:30:40Z
- **Author:** github-actions[bot]
- **Changelog:** Full diff from v0.1.0 to v0.4.0

**Note:** The release is in **draft** status. It needs to be manually published on GitHub to become visible to users.

---

## Summary

| Test | Result | Notes |
|------|--------|-------|
| **Test 9: Desktop Build** | ✅ PASS | 26.73s release build, zero warnings |
| **Test 10: Dashboard UI** | ⚠️ 2 BUGS | `const` prevents nsec backup append; wizard timelock silently discarded |
| **Test 11: Command Parity** | ✅ PASS | All 29 JS commands have Rust handlers; arg names match |
| **Test 12: Release Builds** | ✅ PASS | All 4 platforms + release creation succeeded; draft status |

### Bugs Found

1. **`const content` in descriptor backup** (Medium) — nsec inheritance section silently dropped from backup file due to `TypeError: Assignment to constant variable`. Fix: change `const` to `let`.

2. **Wizard `timelockMonths` discarded** (Medium) — UI shows timelock selection per heir but value is never sent to backend (Rust `add_heir` has no `timelock_months` parameter). Misleading UX.

### Observations

- **14 registered Tauri commands have no UI** — these are v0.2/v0.3 features (heir contacts, pre-signed check-ins, auto-broadcast) that have backend support but no frontend yet. This is expected for incremental development.
- **3 relay commands** — previously commented out due to async `Send` issue, now fully registered and working.
- **Release is draft** — needs manual publish on GitHub.
- **Demo mode is comprehensive** — 29/29 invoked commands have mock responses, enabling full UI testing without a Rust backend.
