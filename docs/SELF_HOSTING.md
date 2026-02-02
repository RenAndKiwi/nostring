# Self-Hosting Guide

This guide covers deploying NoString infrastructure for maximum sovereignty.

---

## Overview

NoString is primarily a **desktop application**. However, you may want to self-host:

1. **Electrum Server** — Private blockchain queries
2. **Bitcoin Node** — Full sovereignty
3. **Notification Relay** — Custom alert delivery

---

## Quick Start (Desktop Only)

For most users, the desktop app is sufficient:

```bash
# Build from source
git clone https://github.com/nostring/nostring
cd nostring
cargo build --release

# Or download pre-built binaries
# https://github.com/nostring/nostring/releases
```

No server required. The app connects to public Electrum servers by default.

---

## Privacy-Enhanced Setup

### Run Your Own Electrum Server

For private blockchain queries, run [electrs](https://github.com/romanz/electrs):

```bash
cd nostring/docker

# Create electrs config
cat > electrs.toml << 'EOF'
network = "bitcoin"
daemon_rpc_addr = "bitcoind:8332"
daemon_p2p_addr = "bitcoind:8333"
electrum_rpc_addr = "0.0.0.0:50001"
log_filters = "INFO"
EOF

# Start with Bitcoin Core
docker-compose --profile full-node up -d
```

Configure NoString to use your server:
```
Electrum URL: tcp://localhost:50001
```

### Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| Bitcoin Core | 500GB SSD, 4GB RAM | 1TB NVMe, 8GB RAM |
| Electrs | +50GB, 4GB RAM | +100GB, 8GB RAM |
| Total | 600GB, 8GB RAM | 1.2TB, 16GB RAM |

Initial sync takes 1-3 days depending on hardware.

---

## Full Sovereignty Stack

### docker-compose.yml

```yaml
version: "3.8"

services:
  bitcoind:
    image: lncm/bitcoind:v27.0
    restart: unless-stopped
    volumes:
      - bitcoind-data:/data
    ports:
      - "8332:8332"
      - "8333:8333"

  electrs:
    image: getumbrel/electrs:v0.10.5
    restart: unless-stopped
    depends_on:
      - bitcoind
    volumes:
      - electrs-data:/data
    ports:
      - "50001:50001"
    environment:
      - ELECTRS_DAEMON_RPC_ADDR=bitcoind:8332

volumes:
  bitcoind-data:
  electrs-data:
```

### Bitcoin Configuration

Create `bitcoin.conf`:

```ini
# Network
server=1
txindex=1
rpcallowip=172.16.0.0/12
rpcbind=0.0.0.0

# Performance
dbcache=4096
maxmempool=300

# Security
rpcuser=nostring
rpcpassword=CHANGE_THIS_PASSWORD
```

---

## Network Security

### Firewall Rules

```bash
# Allow Bitcoin P2P
ufw allow 8333/tcp

# Block Electrum from public (internal only)
ufw deny 50001/tcp
ufw deny 50002/tcp

# Or allow from specific IPs
ufw allow from 192.168.1.0/24 to any port 50001
```

### TLS for Electrum

For remote access, enable TLS:

1. Generate certificate:
   ```bash
   openssl req -x509 -newkey rsa:4096 \
     -keyout electrs.key -out electrs.crt \
     -days 365 -nodes
   ```

2. Configure electrs:
   ```toml
   electrum_rpc_addr = "0.0.0.0:50002"
   electrum_rpc_cert = "/path/to/electrs.crt"
   electrum_rpc_key = "/path/to/electrs.key"
   ```

3. Use SSL in NoString:
   ```
   Electrum URL: ssl://your-server:50002
   ```

---

## Backup Strategy

### What to Backup

| Data | Location | Frequency |
|------|----------|-----------|
| Seed (encrypted) | Local app data | Once (at creation) |
| Policy config | `~/.nostring/` | After changes |
| Watch state | `~/.nostring/watch_state.json` | Daily |
| Bitcoin data | Docker volume | Optional (can resync) |

### Backup Commands

```bash
# Backup NoString config
tar -czf nostring-backup.tar.gz ~/.nostring/

# Backup Bitcoin data (optional, large)
docker run --rm -v bitcoind-data:/data -v $(pwd):/backup \
  alpine tar -czf /backup/bitcoind-backup.tar.gz /data
```

---

## Monitoring

### Health Checks

```bash
# Check Bitcoin sync status
docker exec bitcoind bitcoin-cli getblockchaininfo

# Check Electrum index status
curl -s http://localhost:50001 | jq .

# Check NoString watch state
cat ~/.nostring/watch_state.json | jq .last_height
```

### Alerts

Configure NoString notifications to alert on:
- Timelock approaching expiry
- UTXO state changes
- Connection failures

See [OPERATIONS.md](OPERATIONS.md) for operational procedures.

---

## Troubleshooting

### Bitcoin won't sync

```bash
# Check logs
docker logs bitcoind

# Check disk space
df -h

# Check connections
docker exec bitcoind bitcoin-cli getpeerinfo | jq length
```

### Electrs won't connect

```bash
# Check Bitcoin RPC
docker exec bitcoind bitcoin-cli -rpcuser=nostring -rpcpassword=... getblockchaininfo

# Check electrs logs
docker logs electrs
```

### NoString can't connect

1. Verify Electrum URL is correct
2. Check firewall rules
3. Test with `telnet`:
   ```bash
   telnet localhost 50001
   ```

---

## Upgrades

### NoString

```bash
cd nostring
git pull
cargo build --release
```

### Docker Services

```bash
docker-compose pull
docker-compose up -d
```

---

## Heir Infrastructure Transfer

When setting up inheritance:

1. Document your server configuration
2. Include access credentials (encrypted) with heir materials
3. Consider: heirs may not be technical — document everything

See [HEIR_GUIDE.md](HEIR_GUIDE.md) and [CLAIM_GUIDE.md](CLAIM_GUIDE.md).

---

*Self-host for sovereignty. Document for succession.*
