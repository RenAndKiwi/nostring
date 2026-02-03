# Phase 10: Dashboard UI Polish — Spend Type Icons & Heir Claim Alert Banner

**Status:** PLANNED  
**Depends on:** Phase 9 (spend type detection, witness analysis)  
**Scope:** Frontend-only changes in `tauri-app/frontend/js/app.js` and `tauri-app/frontend/styles/main.css`

---

## 1. Problem Statement

Phase 9 delivered the backend machinery for spend type detection:
- `spend_analysis.rs` — witness analysis distinguishing owner check-in (1 stack item) vs heir claim (2+ items with empty dummy)
- Tauri commands: `detect_spend_type`, `get_spend_events`, `check_heir_claims`
- SQLite tables: `spend_events` (with spend_type, confidence, method columns), `checkin_log` (with spend_type column)

**But the frontend dashboard doesn't surface any of this.** The Status tab shows blocks remaining and last check-in date — it has no awareness of spend types, heir claims, or confidence scores. An heir could be draining funds right now and the owner would see… nothing unusual.

---

## 2. Research Summary

### 2.1 Current Frontend Architecture

- **Stack:** Vanilla JS + HTML + CSS (no framework). Single `app.js` file (~2000 lines). DOM manipulation via `innerHTML` templates.
- **Layout:** Tab-based SPA with 4 tabs: Status, Heirs, Backup, Settings.
- **Status tab** currently shows:
  - Inheritance status card (urgency, days remaining, blocks remaining, current block, last check-in date)
  - Check-in button (creates unsigned PSBT → QR code flow)
  - "How does this work?" collapsible
- **No existing spend event log or activity feed.**

### 2.2 Backend API (Already Complete)

| Command | Returns | Notes |
|---------|---------|-------|
| `get_spend_events` | `Vec<SpendEventInfo>` with `{id, timestamp, txid, spend_type, confidence, method, policy_id, outpoint}` | spend_type: `"owner_checkin"`, `"heir_claim"`, `"unknown"` |
| `check_heir_claims` | `bool` | Simple boolean — any heir_claim rows in spend_events? |
| `detect_spend_type(txid)` | `SpendEventInfo` | On-demand analysis of a specific transaction |

### 2.3 Data Shapes

```typescript
// SpendEventInfo (from commands.rs)
interface SpendEventInfo {
  id: number;
  timestamp: number;       // unix epoch seconds
  txid: string;
  spend_type: string;      // "owner_checkin" | "heir_claim" | "unknown"
  confidence: number;      // 0.0 - 1.0
  method: string;          // "witness_analysis" | "timelock_timing" | "indeterminate"
  policy_id: string | null;
  outpoint: string | null;
}
```

Confidence values from `spend_analysis.rs`:
- Owner (witness, looks like sig): **0.95**
- Owner (witness, unusual sig length): **0.70**
- Owner (timing pre-expiry, definitive): **0.99**
- Owner (witness + timing agree): **0.99**
- Heir (witness, has empty dummy): **0.90**
- Unknown (no empty dummy, multi-item): **0.30**
- Empty witness: **0.0**

---

## 3. Design Specification

### 3.1 Heir Claim Alert Banner

**Location:** Top of the Status tab, above the status card. Rendered only when `check_heir_claims()` returns `true`.

**Visual design:**
```
┌──────────────────────────────────────────────────────────────┐
│ ⚠️ HEIR CLAIM DETECTED                                   ✕ │
│ An heir has claimed funds from your inheritance address.     │
│ This may indicate an unauthorized spend. Review the          │
│ activity log below for details.                              │
│                                                              │
│ [View Details ↓]                          [Dismiss]          │
└──────────────────────────────────────────────────────────────┘
```

**Behavior:**
- **Persistent until dismissed.** Dismissal stores a flag in localStorage (`nostring_heir_alert_dismissed_<latest_event_id>`), so it reappears if a *new* heir claim is detected.
- **Color:** Warning/danger — red-orange border, subtle red background (`rgba(239, 68, 68, 0.1)` with `border-left: 4px solid #ef4444`).
- **"View Details"** scrolls to the activity log section and expands it if collapsed.
- **"Dismiss"** hides the banner and records the most recent heir claim event ID in localStorage.
- The banner re-evaluates on every `refreshStatus()` call.

**When NOT to show:**
- `check_heir_claims()` returns `false`
- All heir claim events have been dismissed (latest event ID matches stored dismissed ID)

### 3.2 Spend Event Activity Log

**Location:** New section on the Status tab, below the check-in card, above the "How does this work?" section.

**Structure:**
```
┌──────────────────────────────────────────────────────────────┐
│ 📋 Activity Log                                    [Refresh]│
├──────────────────────────────────────────────────────────────┤
│ ✅ Owner Check-in         2025-02-01 14:32                   │
│    txid: abc123...def   Confidence: ●●●●○ 95%               │
│    Method: Witness analysis                                  │
│                                                              │
│ ⚠️ Heir Claim             2025-01-28 09:15                   │
│    txid: 789abc...012   Confidence: ●●●●○ 90%               │
│    Method: Witness analysis                                  │
│                                                              │
│ ✅ Owner Check-in         2025-01-15 11:00                   │
│    txid: def456...789   Confidence: ●●●●● 99%               │
│    Method: Witness + Timing                                  │
└──────────────────────────────────────────────────────────────┘
```

**Details:**

- **Icons:**
  - `✅` (green checkmark) for `owner_checkin`
  - `⚠️` (warning) for `heir_claim`
  - `❓` (gray question mark) for `unknown`
- **Confidence indicator:** Visual dot bar (5 dots) + percentage. See §3.3.
- **Truncated txid:** First 8 + last 6 chars, linked to a block explorer (configurable: mempool.space for mainnet, blockstream.info for testnet).
- **Collapsible by default** if there are no heir claims. **Expanded by default** if heir claims exist.
- **Empty state:** "No spend events recorded yet. Events appear when UTXOs are spent."
- **Limit:** Show last 20 events. No pagination needed for v1.

### 3.3 Confidence Indicator

Visual representation of the `confidence` float:

| Range | Dots | Label | Color |
|-------|------|-------|-------|
| 0.90 – 1.00 | ●●●●● | "Very High" | Green (`#10b981`) |
| 0.70 – 0.89 | ●●●●○ | "High" | Yellow-green (`#84cc16`) |
| 0.50 – 0.69 | ●●●○○ | "Medium" | Yellow (`#eab308`) |
| 0.30 – 0.49 | ●●○○○ | "Low" | Orange (`#f97316`) |
| 0.00 – 0.29 | ●○○○○ | "Very Low" | Red (`#ef4444`) |

The confidence indicator serves two purposes:
1. **User education** — helps the owner understand how certain the detection is
2. **Alert gating** — see §5 Security Review for threshold recommendations

### 3.4 Updated Status Card

Add a new row to the existing status display:

```
Heir Claims    ⚠️ 1 detected  (or ✅ None)
```

This is a summary line in the main status card, not a replacement for the alert banner.

### 3.5 Demo Mode Mocks

Add mock data to the `DEMO_MODE` invoke handler:

```javascript
'get_spend_events': [
  { id: 1, timestamp: Date.now()/1000 - 86400*2, txid: 'abc123def456abc123def456abc123def456abc123def456abc123def456abcd', spend_type: 'owner_checkin', confidence: 0.95, method: 'witness_analysis', policy_id: null, outpoint: null },
  { id: 2, timestamp: Date.now()/1000 - 86400*30, txid: '789012ghi789012ghi789012ghi789012ghi789012ghi789012ghi789012ghij', spend_type: 'owner_checkin', confidence: 0.99, method: 'timelock_timing', policy_id: null, outpoint: null },
],
'check_heir_claims': false,
'detect_spend_type': { success: true, data: { id: 0, timestamp: Date.now()/1000, txid: 'test', spend_type: 'owner_checkin', confidence: 0.95, method: 'witness_analysis', policy_id: null, outpoint: null } },
```

---

## 4. Component Structure

Since the app uses vanilla JS with `innerHTML` templates, "components" are functions that return HTML strings and set up event listeners.

### 4.1 New Functions

```
renderHeirClaimBanner(spendEvents)
  → Returns HTML string for the alert banner (or empty string if no heir claims)
  → Checks localStorage for dismissed state

renderActivityLog(spendEvents)
  → Returns HTML string for the collapsible activity log
  → Calls renderSpendEventRow() for each event

renderSpendEventRow(event)
  → Returns HTML for a single spend event (icon, txid, confidence, method)

renderConfidenceIndicator(confidence)
  → Returns HTML for the dot-bar + percentage

formatTxidLink(txid, network)
  → Returns truncated txid wrapped in <a> tag to block explorer

loadSpendEvents()
  → Calls invoke('get_spend_events') and invoke('check_heir_claims')
  → Stores results in module-level state
  → Re-renders banner + activity log

dismissHeirClaimAlert(latestEventId)
  → Stores dismissed event ID in localStorage
  → Hides the banner
```

### 4.2 Integration Points

**In `showMainApp()`:**
1. Add `<div id="heir-claim-banner"></div>` at the top of the status-tab section (before the status-card div)
2. Add `<div id="activity-log"></div>` between the checkin-card and how-it-works sections
3. Call `loadSpendEvents()` during initial data loading

**In `refreshStatus()`:**
1. After refreshing policy status, call `loadSpendEvents()` to refresh the activity log and banner

**New CSS classes** (in `main.css`):
- `.heir-claim-banner` — the alert banner
- `.activity-log` — the log container
- `.spend-event-row` — individual event row
- `.spend-type-icon` — icon styling (size, alignment)
- `.confidence-dots` — the dot bar
- `.confidence-dot-filled` / `.confidence-dot-empty` — dot states

### 4.3 Data Flow

```
User opens app / clicks Refresh
  → refreshStatus()
    → invoke('refresh_policy_status') → update status card
    → invoke('check_and_notify') → (existing) trigger notifications
    → loadSpendEvents()  ← NEW
      → invoke('get_spend_events') → spendEvents[]
      → invoke('check_heir_claims') → hasHeirClaims (bool)
      → renderHeirClaimBanner(spendEvents) → inject into #heir-claim-banner
      → renderActivityLog(spendEvents) → inject into #activity-log
      → if hasHeirClaims && not dismissed → show banner
```

---

## 5. Security Review

### 5.1 Can the UI Be Spoofed?

**Threat:** A compromised frontend could suppress heir claim alerts, making the owner unaware.

**Mitigations:**
- The Tauri IPC bridge is the trust boundary. The frontend can only call registered commands — it can't modify backend state or suppress spend events in SQLite.
- However, if the frontend JS is tampered with (e.g., supply chain attack on CDN-loaded QR libraries), it could hide the banner.
- **Recommendation:** Move QR libraries to local bundles (already a TODO). The current CDN loads (`cdn.jsdelivr.net`) are a supply chain risk.
- **Recommendation:** The backend `check_and_notify` command already handles notifications independently of the UI. Even if the UI is suppressed, Nostr DM / email alerts still fire. This is defense-in-depth.

### 5.2 Should Confidence Thresholds Gate the Alert?

**Analysis:**

| Confidence | Meaning | Should alert? |
|------------|---------|---------------|
| ≥ 0.85 | Witness analysis found empty dummy (heir path) or timing confirms | **YES — full alert** |
| 0.50–0.84 | Ambiguous witness but some indicators | **YES — alert with "low confidence" qualifier** |
| < 0.50 | Basically unknown | **NO alert banner, but show in activity log** |

**Recommendation:**
- **Alert banner** only appears for heir claims with `confidence ≥ 0.50`.
- Activity log shows ALL events regardless of confidence.
- The banner text should include the confidence level: "⚠️ Heir claim detected (90% confidence)" vs "⚠️ Possible heir claim (55% confidence — review manually)".

**Rationale:** False negatives are far worse than false positives in an inheritance tool. Better to show a "possible" alert that the owner can investigate than to silently miss a real claim. The 0.50 threshold filters only the truly indeterminate cases.

### 5.3 Privacy Considerations

- Spend events include txids, which are visible on-chain anyway. No new privacy leak.
- Block explorer links should default to **mempool.space** (no tracking) or be configurable.
- Activity log is local-only (SQLite). Not transmitted anywhere.

### 5.4 Denial of Service via Event Flooding

- `get_spend_events` returns ALL events. A pathological case (many UTXOs cycling) could produce hundreds of rows.
- **Recommendation:** Limit to last 50 events in the query (add `LIMIT 50` to `spend_event_list`), or paginate on the frontend with "Show More".

---

## 6. UX Decisions

### 6.1 What Should Happen When an Heir Claim Is Detected?

**Visual + Notification (belt and suspenders):**

1. **Visual:** Alert banner at top of dashboard (this phase).
2. **Notification:** Already handled by `check_and_notify` → Nostr DM / email to owner. No additional work needed.
3. **Sound:** Not for v1. Desktop notification API could be added later.

The combination ensures the owner is alerted even if they don't open the app for days (via Nostr DM) and immediately sees the issue when they do open it (via banner).

### 6.2 What Can the Owner DO About an Heir Claim?

Once detected, the owner's options are:
1. **If funds are still in the UTXO** (heir claim in mempool, not yet confirmed): Race with a higher-fee check-in transaction. This is an RBF/CPFP scenario — out of scope for Phase 10 but worth noting.
2. **If funds are already spent** by the heir: Contact the heir, investigate, potentially legal action. NoString can't reverse a confirmed transaction.
3. **If it's a false positive** (Unknown spend type with low confidence): Dismiss the alert.

For Phase 10, the banner should include a brief explanation: "If this is unexpected, your funds may have been claimed. Contact your heirs or review the transaction on a block explorer."

### 6.3 Activity Log vs Check-in History

Currently, check-ins are logged in `checkin_log` with a timestamp and txid. Spend events are in `spend_events` with richer data (confidence, method). These are separate tables.

**Decision:** The activity log should query `spend_events` (the richer table). We do NOT merge the tables — they serve different purposes (checkin_log is the owner's action history; spend_events is the monitoring service's detection history). But visually, the activity log replaces the need for a separate "check-in history" view.

---

## 7. Implementation Plan

### Step 1: CSS (main.css additions)

Add styles for:
- `.heir-claim-banner` (alert banner: red border, warning background, dismiss button)
- `.activity-log` (container with header, collapsible body)
- `.spend-event-row` (flexbox row: icon | details | confidence)
- `.confidence-dots` (inline flex, 5 dots)
- Responsive: banner and log should stack nicely on narrow windows

**Estimated effort:** ~60 lines of CSS

### Step 2: Demo Mode Mocks (app.js)

Add `get_spend_events`, `check_heir_claims`, `detect_spend_type` to the mock object so the UI can be developed and tested without a running backend.

**Estimated effort:** ~15 lines

### Step 3: Render Functions (app.js)

Implement the 6 new functions from §4.1. Pure render logic — takes data, returns HTML strings.

**Estimated effort:** ~120 lines

### Step 4: Integration (app.js)

Wire `loadSpendEvents()` into `showMainApp()` and `refreshStatus()`. Add the placeholder `<div>` elements to the Status tab template.

**Estimated effort:** ~30 lines

### Step 5: Status Card Enhancement

Add the "Heir Claims: None / ⚠️ N detected" line to the status display in `refreshStatus()`.

**Estimated effort:** ~10 lines

### Step 6: Testing

- Test with demo mode (no backend): verify layout, icons, confidence dots, banner show/dismiss
- Test with heir_claim mock data: verify banner appears, dismiss persists in localStorage, re-appears on new event
- Test banner dismissal edge cases: dismiss, add new claim, verify re-shown
- Test empty state: no spend events → "No events" message
- Test confidence thresholds: verify banner suppressed for < 0.50

---

## 8. Files Modified

| File | Changes |
|------|---------|
| `tauri-app/frontend/styles/main.css` | New CSS classes for banner, activity log, confidence dots |
| `tauri-app/frontend/js/app.js` | New render functions, demo mocks, integration into showMainApp/refreshStatus |
| `tauri-app/frontend/index.html` | No changes needed (dynamic rendering) |
| `tauri-app/src-tauri/src/commands.rs` | No changes (backend already complete) |
| `tauri-app/src-tauri/src/db.rs` | Optional: add `LIMIT` to `spend_event_list` query |

---

## 9. Future Considerations (Out of Scope)

- **Desktop notifications** (Tauri notification plugin) for heir claims
- **Sound alerts** for critical events
- **RBF response** — automatic fee-bumped check-in to outrace an heir claim
- **Per-policy activity** — when multiple inheritance policies exist, filter events by policy_id
- **Real-time monitoring** — WebSocket-based event stream from the watch service instead of polling on refresh
- **Block explorer preference** in settings (mempool.space vs blockstream.info vs custom)

---

## 10. Roadmap Position

```
Phase 9:  Spend type detection (backend) ........... ✅ COMPLETE
Phase 10: Dashboard UI polish (this document) ...... 📋 PLANNED
Phase 11: Pre-signed check-in UX ................... 📋 PLANNED
Phase 12: Multi-policy support ..................... 📋 FUTURE
```
