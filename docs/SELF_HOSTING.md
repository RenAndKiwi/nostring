# Self-Hosting NoString Server

Run NoString's inheritance monitoring service 24/7 on your own server using Docker.

## Why Self-Host?

The NoString desktop app is great for interactive use, but your computer isn't always on. The server component runs headlessly and:

- **Monitors your inheritance UTXOs** continuously via Electrum
- **Sends check-in reminders** to you via Nostr DM and/or email
- **Delivers descriptor backups to heirs** automatically when the timelock reaches critical status (≤1 day)
- **Runs on any Docker-capable server** — VPS, NAS, Raspberry Pi, etc.

## Prerequisites

- Docker and Docker Compose
- Your inheritance descriptor (export from the NoString desktop app)
- A Nostr service key (generate in the desktop app under Settings)
- (Optional) SMTP credentials for email notifications

## Quick Start

### 1. Clone the repository

```bash
git clone https://github.com/nostring/nostring.git
cd nostring
```

### 2. Create the configuration

```bash
mkdir -p config
cp config/nostring-server.example.toml config/nostring-server.toml
```

### 3. Edit the configuration

Open `config/nostring-server.toml` and fill in:

- **`policy.descriptor`** — Your WSH inheritance descriptor
- **`policy.timelock_blocks`** — Must match your descriptor's timelock
- **`notifications.nostr.service_key`** — Service nsec from the desktop app
- **`notifications.nostr.owner_npub`** — Your npub for receiving reminders

```toml
[policy]
descriptor = "wsh(or_d(pk([aabbccdd/84'/0'/0']xpub6.../0/*),and_v(v:pk([eeff0011/84'/0'/1']xpub6.../0/*),older(26280))))"
timelock_blocks = 26280

[notifications.nostr]
service_key = "nsec1..."
owner_npub = "npub1..."
```

### 4. Start the server

```bash
docker compose up -d
```

### 5. Check the logs

```bash
docker compose logs -f nostring-server
```

You should see:

```
NoString server starting…
  Network:    bitcoin
  Electrum:   ssl://blockstream.info:700
  Interval:   21600 seconds (6.0 hours)
Starting check cycle…
Block height: 935000  |  Events: 0
No owner notification needed — timelock healthy.
Check cycle completed successfully.
Sleeping 21600 seconds until next check…
```

## Configuration Reference

### Server Settings

| Key | Env Var | Default | Description |
|-----|---------|---------|-------------|
| `server.data_dir` | `NOSTRING_DATA_DIR` | `/data` | Persistent data directory |
| `server.check_interval_secs` | `NOSTRING_CHECK_INTERVAL` | `21600` (6h) | Time between checks |
| `server.log_level` | `NOSTRING_LOG_LEVEL` | `info` | Log verbosity |

### Bitcoin Settings

| Key | Env Var | Default | Description |
|-----|---------|---------|-------------|
| `bitcoin.network` | `NOSTRING_NETWORK` | `bitcoin` | Network (bitcoin/testnet/signet/regtest) |
| `bitcoin.electrum_url` | `NOSTRING_ELECTRUM_URL` | `ssl://blockstream.info:700` | Electrum server |

### Notification Thresholds

Default thresholds send notifications when remaining time drops below:
- **30 days** — Gentle reminder
- **7 days** — Warning
- **1 day** — Urgent
- **0 days** — Critical (also triggers heir delivery)

### Heir Delivery

When the timelock reaches critical status (≤144 blocks / ~1 day), the server automatically:

1. Sends the full descriptor backup to all configured heirs
2. Uses their configured npub (Nostr DM) and/or email
3. Includes everything the heir needs to claim the inheritance

Configure heirs in the TOML:

```toml
[[notifications.heirs]]
label = "Spouse"
npub = "npub1..."
email = "spouse@example.com"

[[notifications.heirs]]
label = "Child"
npub = "npub1..."
```

## Environment Variables

For sensitive values (keys, passwords), prefer environment variables over the config file:

```yaml
# docker-compose.yml
services:
  nostring-server:
    environment:
      NOSTRING_SERVICE_KEY: "nsec1..."
      NOSTRING_OWNER_NPUB: "npub1..."
```

Or use a `.env` file:

```bash
NOSTRING_SERVICE_KEY=nsec1...
NOSTRING_OWNER_NPUB=npub1...
```

## Running Without Docker

The server binary can also run directly:

```bash
# Build
cargo build --release -p nostring-server

# Run as daemon
./target/release/nostring-server --config config/nostring-server.toml

# Run a single check (useful for cron)
./target/release/nostring-server --config config/nostring-server.toml --check

# Validate config
./target/release/nostring-server --config config/nostring-server.toml --validate
```

### Systemd Service

```ini
[Unit]
Description=NoString Inheritance Monitor
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=nostring
ExecStart=/usr/local/bin/nostring-server --config /etc/nostring/nostring-server.toml
Restart=on-failure
RestartSec=30
Environment=NOSTRING_LOG_LEVEL=info

[Install]
WantedBy=multi-user.target
```

## Architecture

```
┌─────────────────────────────────────────────┐
│               nostring-server               │
│                                             │
│  ┌─────────────┐    ┌──────────────────┐   │
│  │ WatchService │───►│ NotificationSvc  │   │
│  │ (poll loop)  │    │ (Nostr DM/Email) │   │
│  └──────┬───────┘    └───────┬──────────┘   │
│         │                    │              │
│         ▼                    ▼              │
│  ┌─────────────┐    ┌──────────────────┐   │
│  │  Electrum    │    │  Nostr Relays /  │   │
│  │  Server      │    │  SMTP Server     │   │
│  └─────────────┘    └──────────────────┘   │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │   /data (SQLite + watch_state.json) │   │
│  └─────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

The server reuses the same library crates as the desktop app:
- **nostring-watch** — UTXO monitoring and event detection
- **nostring-notify** — Nostr DM and email notifications
- **nostring-electrum** — Bitcoin network communication
- **nostring-inherit** — Inheritance policy logic

## Security Considerations

1. **Service key ≠ Owner key**: The service key is a separate Nostr keypair used only for sending notifications. It cannot spend your Bitcoin.

2. **Read-only filesystem**: The Docker container runs with a read-only root filesystem. Only `/data` and `/tmp` are writable.

3. **Non-root user**: The container runs as a dedicated `nostring` user, not root.

4. **Outbound-only networking**: No ports are exposed. The server only makes outbound connections to Electrum servers and Nostr relays.

5. **Descriptor exposure**: The descriptor is stored in the config file. The descriptor alone is NOT sufficient to spend — heirs still need their signing device. However, treat the config file as sensitive.

6. **Electrum privacy**: Your server's IP will be visible to the Electrum server. For better privacy, run your own Electrum server (e.g., [Electrs](https://github.com/romanz/electrs)) and point `electrum_url` to it.

## Troubleshooting

### "Failed to connect to Electrum"
- Check your internet connection
- Verify the `electrum_url` is correct
- Try a different Electrum server
- Ensure outbound connections on port 700 (SSL) are allowed

### "No active UTXOs"
- Your inheritance address may not have any funded UTXOs
- Verify the descriptor matches your desktop app configuration
- Check that you're on the correct network (mainnet vs testnet)

### "Owner notification error: No notification channels enabled"
- Configure at least one notification channel (Nostr or email)
- Check that `service_key` and `owner_npub` are set correctly

### Logs not showing?
```bash
docker compose logs --tail 100 nostring-server
```

### Reset state
```bash
docker compose down
docker volume rm nostring_nostring-data
docker compose up -d
```
