# Phase 11: Testnet Mode Toggle

**Goal:** Let users switch between Bitcoin mainnet and testnet3 from the UI — no config file edits. Visual indicators ensure the user always knows which network they're on. Separate state per network prevents cross-contamination.

**Status:** Planning

**Context:** The testnet wallet already exists (347,970 sats on testnet3). The `nostring-electrum` crate already supports per-network default servers. `AppState` already persists `network` to SQLite. But the UI has no way to change it, and switching networks without restarting would leave stale state in memory.

---

## Research Summary

### What Already Works

| Layer | Testnet Support | Notes |
|-------|----------------|-------|
| `nostring-electrum` | ✅ `default_server(Network::Testnet)` → `ssl://blockstream.info:993` | Full Electrum client per-network |
| `AppState` | ✅ Persists `network` key in SQLite, loads on startup | Also stores `electrum_url` — but only one URL, not per-network |
| `ServerConfig` | ✅ `BitcoinSection.network` field, env override `NOSTRING_NETWORK` | Server daemon already supports testnet |
| Tauri commands | ✅ Read `state.network` for every Electrum call | `refresh_policy_status`, `initiate_checkin`, `broadcast_signed_psbt`, `detect_spend_type` all use `*state.network.lock()` |
| Frontend | ❌ No network selector or indicator | Electrum URL is editable in Settings, but network enum is not |
| DB isolation | ❌ Single `nostring.db` for all networks | Mainnet and testnet share the same heir registry, descriptors, and check-in logs |

### Key Design Decisions

1. **Separate databases per network** — A single DB mixing mainnet and testnet state is a fund-loss vector. The bitcoin ecosystem convention (Electrum, Bitcoin Core, Sparrow) is separate data directories.

2. **Toggle location: Settings page** — Not the setup wizard (wizard runs once; network switching is ongoing). A prominent toggle in Settings with a confirmation dialog.

3. **Restart required on switch** — Changing network means swapping the entire SQLite connection, Electrum server, cached UTXOs, policy status, and heir descriptors. A clean restart (Tauri app relaunch) is safer than hot-swapping all Mutex fields. This matches how Bitcoin Core and Electrum handle it.

4. **Visual indicator: persistent banner + color accent shift** — The gold theme stays for mainnet. Testnet gets a green/teal accent with a persistent top banner reading "⚠️ TESTNET MODE — Not real bitcoin."

---

## Feature 11.1: Per-Network Database Isolation

### Design

```
~/.local/share/com.nostring.app/          # Linux (example)
├── mainnet/
│   └── nostring.db
├── testnet/
│   └── nostring.db
└── network.conf                           # One line: "bitcoin" or "testnet"
```

**`network.conf`** is a tiny file read *before* the DB is opened. It tells the app which subdirectory to load. This avoids chicken-and-egg (can't read network from a DB that hasn't been opened yet).

### Changes Required

| File | Change |
|------|--------|
| `tauri-app/src-tauri/src/main.rs` | Read `network.conf` → resolve DB path → pass to `AppState::from_db_path()` |
| `tauri-app/src-tauri/src/state.rs` | `from_db_path` already works — no change needed |
| `tauri-app/src-tauri/src/commands.rs` | New command `switch_network(target: String)` — writes `network.conf`, returns "restart required" |
| `tauri-app/src-tauri/src/commands.rs` | New command `get_current_network()` — reads `network.conf` |

### Migration

Existing users have a flat `nostring.db`. On first launch after upgrade:
1. Detect flat DB (no `network.conf` exists)
2. Move existing `nostring.db` → `mainnet/nostring.db`
3. Write `network.conf` = `"bitcoin"`
4. Continue boot normally

This is non-destructive and idempotent.

### Security Review

- **Threat: User opens testnet, creates a policy, deposits real BTC thinking they're on mainnet.**
  - Mitigation: Visual banner (Feature 11.3), address prefix validation (`bc1` vs `tb1`), confirmation dialog on switch.

- **Threat: Mainnet descriptor leaks into testnet DB or vice versa.**
  - Mitigation: Complete DB isolation. Each network has its own config table, heir registry, check-in logs, and descriptor. No shared state.

- **Threat: `network.conf` is tampered with to redirect the app.**
  - Mitigation: The file lives in the app data directory (OS-protected). Same trust model as the SQLite DB itself. Additionally, the Electrum URL stored in each DB should be validated against the network (mainnet URL on mainnet DB, testnet URL on testnet DB).

---

## Feature 11.2: Network Toggle in Settings UI

### Design

**Location:** Settings tab, first section — "Network" — above Electrum URL.

```
┌─────────────────────────────────────────────┐
│  ⚙️  Settings                               │
├─────────────────────────────────────────────┤
│                                             │
│  Network                                    │
│  ┌───────────────────────────────────────┐  │
│  │  ● Bitcoin Mainnet     ○ Testnet3     │  │
│  └───────────────────────────────────────┘  │
│  ℹ️  Switching requires app restart.        │
│  Current: Bitcoin Mainnet                   │
│                                             │
│  Electrum Server                            │
│  [ ssl://blockstream.info:700          ]    │
│  (auto-set when switching networks)         │
│                                             │
│  ...                                        │
└─────────────────────────────────────────────┘
```

### Interaction Flow

1. User clicks the other radio button (e.g., switches from Mainnet → Testnet)
2. **Confirmation dialog** appears:
   > ⚠️ **Switch to Testnet?**
   >
   > This will restart the app and load testnet data.
   > Testnet uses fake bitcoin — your mainnet wallet is safe.
   >
   > [Cancel] [Switch & Restart]
3. On confirm: Tauri command `switch_network("testnet")` writes `network.conf`, then `tauri::api::process::restart(&app.env())`
4. App relaunches, reads `network.conf`, opens `testnet/nostring.db`

### Electrum URL Auto-Switch

When network changes, the Electrum URL in the new DB defaults to the correct server:
- Mainnet → `ssl://blockstream.info:700`
- Testnet → `ssl://blockstream.info:993`

If the user previously customized the Electrum URL for that network, it's preserved in that network's DB.

### Changes Required

| File | Change |
|------|--------|
| `frontend/js/app.js` | Network radio buttons in `renderSettings()`, confirmation dialog, invoke `switch_network` + `get_current_network` |
| `commands.rs` | `switch_network` and `get_current_network` commands |
| `main.rs` | Read `network.conf` at startup, use as DB path selector |

---

## Feature 11.3: Visual Network Indicator

### Design

**Mainnet appearance:** Current gold theme, no banner. This is the default — no visual noise.

**Testnet appearance:**
1. **Persistent top banner** (above header): Full-width, green/teal background (#10b981), white text:
   ```
   ⚠️ TESTNET MODE — This wallet uses test bitcoin (no real value)    [Switch to Mainnet]
   ```
2. **Accent color shift**: CSS custom property override — `--accent: #10b981` (emerald) instead of `#FBDC7B` (gold). Affects active tab, buttons, and highlights.
3. **Title bar suffix**: Window title becomes "NoString [TESTNET]"

### Implementation

Frontend receives the network from `get_current_network()` on load:

```javascript
// On app init
const network = await invoke('get_current_network');
if (network === 'testnet' || network === 'testnet3') {
    document.body.classList.add('testnet-mode');
    // Inject banner
    const banner = document.createElement('div');
    banner.id = 'testnet-banner';
    banner.innerHTML = '⚠️ TESTNET MODE — This wallet uses test bitcoin (no real value) <button onclick="switchToMainnet()">Switch to Mainnet</button>';
    document.body.prependChild(banner);
}
```

CSS:
```css
/* Testnet mode overrides */
body.testnet-mode {
    --accent: #10b981;
    --accent-hover: #059669;
    --accent-active: #047857;
}

#testnet-banner {
    background: #10b981;
    color: white;
    text-align: center;
    padding: 0.5rem 1rem;
    font-weight: 600;
    font-size: 0.85rem;
    letter-spacing: 0.02em;
}

#testnet-banner button {
    background: rgba(255,255,255,0.2);
    border: 1px solid rgba(255,255,255,0.4);
    color: white;
    padding: 0.2rem 0.8rem;
    border-radius: 4px;
    margin-left: 1rem;
    cursor: pointer;
}
```

### Security Review

- **Threat: User ignores the banner and transacts on wrong network.**
  - Mitigation: Banner is persistent (not dismissible), accent color changes throughout the UI, and Bitcoin addresses themselves have different prefixes (`bc1` vs `tb1`) which serve as a final safeguard.

- **Threat: Color-blind users can't distinguish mainnet vs testnet.**
  - Mitigation: The text banner is the primary indicator, not just the color. "TESTNET MODE" is always visible in words. Window title also includes `[TESTNET]`.

---

## Feature 11.4: Address Prefix Validation Guard

### Design

As an additional safety layer, validate that addresses match the active network before any transaction:

| Network | Expected prefixes |
|---------|-------------------|
| Bitcoin (mainnet) | `bc1`, `1`, `3` |
| Testnet | `tb1`, `m`, `n`, `2` |

### Changes Required

| File | Change |
|------|--------|
| `commands.rs` → `initiate_checkin` | After deriving the descriptor address, validate its prefix against `state.network`. Reject with clear error if mismatched. |
| `commands.rs` → `broadcast_signed_psbt` | Validate PSBT outputs against expected network before broadcast. |
| `nostring-electrum/src/lib.rs` | Add `fn validate_address_network(address: &str, network: Network) -> bool` utility. |

### Security Review

- This is defense-in-depth. The `bitcoin` crate's `Address` type already encodes network, but an explicit check at the command boundary catches bugs in descriptor derivation early.

---

## Feature 11.5: Server Daemon Testnet Support

### Design

The `nostring-server` daemon already supports testnet via `config.toml`:
```toml
[bitcoin]
network = "testnet"
electrum_url = "ssl://blockstream.info:993"
```

No changes needed for the daemon itself. But for consistency, document how to run mainnet and testnet daemons side-by-side:

```bash
# Mainnet (default)
nostring-server --config /etc/nostring/mainnet.toml

# Testnet
nostring-server --config /etc/nostring/testnet.toml
# Or: NOSTRING_NETWORK=testnet nostring-server --config config.toml
```

The Docker compose file should get a `testnet` profile:
```yaml
services:
  nostring-testnet:
    image: nostring-server
    environment:
      - NOSTRING_NETWORK=testnet
      - NOSTRING_ELECTRUM_URL=ssl://blockstream.info:993
    volumes:
      - ./data/testnet:/data
```

---

## Security Review (Cross-Feature)

### Threat Model

| # | Threat | Severity | Mitigation |
|---|--------|----------|------------|
| 1 | User sends real BTC to testnet address | **Critical** | Address prefix validation (11.4), visual indicators (11.3), confirmation dialog (11.2) |
| 2 | User sends testnet coins thinking they're real | Low | Banner says "no real value", addresses show `tb1` prefix |
| 3 | Mainnet private keys/descriptors used on testnet | Medium | DB isolation (11.1) — each network has its own seed, heirs, and descriptors |
| 4 | Testnet Electrum server connected on mainnet | **High** | On network switch, auto-set Electrum URL to default for that network. On startup, validate stored URL against network. |
| 5 | Attacker modifies `network.conf` to redirect wallet | Medium | Same trust model as DB file — OS file permissions. App could add a checksum. |
| 6 | Heir receives testnet descriptor thinking it's mainnet | Medium | Descriptor backup includes network field (already does). Banner on backup download screen shows network. |
| 7 | Pre-signed check-in PSBTs from mainnet used on testnet | **High** | PSBTs are stored per-network DB. Different DB = different PSBTs. Bitcoin consensus also rejects cross-network txs. |

### Invariants to Enforce

1. **`network.conf` and DB `network` key must agree.** On startup, if they disagree, show an error and refuse to start (don't silently use the wrong network).

2. **Electrum server must match network.** On connect, query a known block hash to confirm the server is on the expected network (e.g., testnet genesis hash differs from mainnet).

3. **No cross-network data sharing.** The only shared file is `network.conf`. Everything else lives inside the network-specific directory.

---

## Implementation Order

```
11.1  Per-Network DB Isolation          ~2 hours
  └─ Migration for existing users
  └─ network.conf read/write
  └─ Startup path resolution

11.2  Network Toggle in Settings        ~1 hour
  └─ Tauri commands (switch + get)
  └─ Frontend radio buttons + dialog
  └─ App restart on switch

11.3  Visual Network Indicator          ~1 hour
  └─ Testnet banner + CSS overrides
  └─ Window title suffix

11.4  Address Prefix Validation         ~30 min
  └─ Guard in checkin + broadcast commands
  └─ Utility function in electrum crate

11.5  Server Daemon Testnet Docs        ~30 min
  └─ Docker compose testnet profile
  └─ Side-by-side docs
```

**Total estimated effort: ~5 hours**

---

## Test Plan

| Test | Type | What it verifies |
|------|------|-----------------|
| `test_network_conf_read_write` | Unit | `network.conf` round-trips correctly |
| `test_db_migration_flat_to_nested` | Unit | Existing flat DB moves to `mainnet/` |
| `test_db_isolation` | Unit | Writing to testnet DB doesn't appear in mainnet DB |
| `test_electrum_url_auto_switch` | Unit | Switching to testnet sets correct default URL |
| `test_address_validation_mainnet` | Unit | `bc1...` accepted, `tb1...` rejected on mainnet |
| `test_address_validation_testnet` | Unit | `tb1...` accepted, `bc1...` rejected on testnet |
| `test_network_mismatch_startup` | Unit | App refuses to start if `network.conf` disagrees with DB |
| `test_switch_network_writes_conf` | Integration | `switch_network("testnet")` updates file |
| `test_testnet_electrum_connection` | Integration (ignored) | Connects to testnet Electrum, gets height |
| `test_visual_indicator_class` | Frontend | `body.testnet-mode` class applied when network=testnet |

---

## Future Enhancements

- **Signet support** — Same architecture, just add `Network::Signet` option. Signet is more reliable than testnet3 and could be a better default test network.
- **Regtest for local dev** — Toggle to regtest with `tcp://127.0.0.1:50001` for developers running local nodes.
- **Network badge on descriptor backups** — Watermark testnet backups so heirs don't mistake them for mainnet.
- **Auto-detect network from xpub prefix** — `xpub` = mainnet, `tpub` = testnet. Warn if xpub network doesn't match current network on import.

---

*Phase 11 plan created: 2026-02-05*
