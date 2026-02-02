# Operations Runbook

Procedures for operating and maintaining NoString infrastructure.

---

## Daily Operations

### Health Check

Run daily to verify system health:

```bash
#!/bin/bash
# nostring-health-check.sh

echo "=== NoString Health Check ==="
echo "Date: $(date)"

# Check NoString config exists
if [ -f ~/.nostring/config.json ]; then
    echo "✓ Config found"
else
    echo "✗ Config missing"
fi

# Check watch state
if [ -f ~/.nostring/watch_state.json ]; then
    LAST_HEIGHT=$(jq -r '.last_height // "null"' ~/.nostring/watch_state.json)
    LAST_POLL=$(jq -r '.last_poll // "null"' ~/.nostring/watch_state.json)
    echo "✓ Watch state: height=$LAST_HEIGHT, last_poll=$LAST_POLL"
else
    echo "⚠ No watch state (first run?)"
fi

# Check Bitcoin node (if running)
if docker ps | grep -q bitcoind; then
    SYNC=$(docker exec bitcoind bitcoin-cli getblockchaininfo 2>/dev/null | jq -r '.verificationprogress')
    echo "✓ Bitcoin node: sync=$SYNC"
else
    echo "○ Bitcoin node not running (using public Electrum)"
fi

# Check Electrum connection
if nc -z localhost 50001 2>/dev/null; then
    echo "✓ Local Electrum available"
else
    echo "○ Using remote Electrum"
fi

echo "=== End Health Check ==="
```

---

## Periodic Tasks

### Weekly

| Task | Procedure |
|------|-----------|
| Verify backups | Check backup files exist and are recent |
| Review notifications | Ensure alerts are being delivered |
| Check disk space | `df -h` — Bitcoin data grows ~10GB/month |

### Monthly

| Task | Procedure |
|------|-----------|
| Test notification delivery | Send test email/DM |
| Review heir documentation | Ensure guides are current |
| Check for updates | `git fetch && git log HEAD..origin/main` |

### Quarterly

| Task | Procedure |
|------|-----------|
| Full recovery test | Practice claim procedure with test wallet |
| Security review | Check for exposed secrets, audit logs |
| Update dependencies | `cargo update && cargo test` |

---

## Incident Response

### Timelock Approaching Expiry

**Severity:** High

**Symptoms:**
- Notification received (30/7/1 day warning)
- Watch state shows low blocks_remaining

**Response:**
1. Open NoString app
2. Initiate check-in transaction
3. Sign with hardware wallet
4. Broadcast
5. Verify confirmation

```bash
# Verify check-in
# New UTXO should appear with fresh timelock
```

### Missed Check-in (Timelock Expired)

**Severity:** Critical

**Symptoms:**
- Timelock has expired (blocks_remaining < 0)
- Heirs can now claim

**Response:**
1. **If owner is alive:**
   - Create new inheritance UTXO immediately
   - Heirs may race to claim — act fast
   
2. **If owner is deceased:**
   - This is expected behavior
   - Direct heirs to CLAIM_GUIDE.md

### Node Sync Failure

**Severity:** Medium

**Symptoms:**
- Electrum queries failing
- Watch state not updating

**Response:**
1. Check node logs: `docker logs bitcoind`
2. Check disk space: `df -h`
3. Restart if needed: `docker-compose restart`
4. Fall back to public Electrum if urgent

### Notification Failure

**Severity:** Medium

**Symptoms:**
- No alerts received despite approaching timelock

**Response:**
1. Check notification config
2. Verify SMTP credentials / Nostr keys
3. Send test notification
4. Check spam folders
5. Verify relay connectivity

---

## Recovery Procedures

### Restore from Backup

```bash
# Stop services
docker-compose down

# Restore config
tar -xzf nostring-backup.tar.gz -C ~/

# Restart
docker-compose up -d

# Verify
./nostring-health-check.sh
```

### Rebuild Watch State

If watch state is corrupted:

```bash
# Remove old state
rm ~/.nostring/watch_state.json

# NoString will rebuild on next poll
# May take a few polls to rediscover UTXOs
```

### Emergency Check-in (No App)

If NoString app is unavailable:

1. Use Sparrow or Electrum to create PSBT
2. Sign with hardware wallet
3. Broadcast via any method

The check-in is just a spend from the inheritance address — any wallet can do it.

---

## Security Procedures

### Rotate Notification Credentials

1. Generate new SMTP password / Nostr key
2. Update NoString config
3. Send test notification
4. Revoke old credentials

### Audit Log Review

Check for:
- Unexpected UTXO changes
- Failed authentication attempts
- Unusual notification patterns

```bash
# Review watch events
jq '.policies | to_entries | .[].value.utxos' ~/.nostring/watch_state.json
```

### Respond to Compromise

If credentials are compromised:

1. **Seed compromised:** Move funds immediately to new wallet
2. **Notification credentials:** Rotate, check for unauthorized alerts
3. **Server access:** Rotate all credentials, audit logs

---

## Maintenance Windows

### Best Practices

1. **Schedule during low-risk periods**
   - Not when timelock is close to expiry
   - Not during high Bitcoin fee periods

2. **Notify stakeholders**
   - Inform heirs of planned downtime
   - Ensure backup contact methods

3. **Have rollback plan**
   - Keep previous version available
   - Test restore procedure first

### Update Procedure

```bash
# 1. Create backup
tar -czf nostring-backup-$(date +%Y%m%d).tar.gz ~/.nostring/

# 2. Pull updates
cd nostring
git pull

# 3. Build and test
cargo build --release
cargo test

# 4. Verify health
./nostring-health-check.sh
```

---

## Contact Escalation

| Severity | Response Time | Action |
|----------|---------------|--------|
| Critical | Immediate | Check-in or claim in progress |
| High | < 24 hours | Timelock warning |
| Medium | < 1 week | Service degradation |
| Low | Next maintenance | Optimization |

---

## Documentation Updates

After any incident:

1. Update this runbook with lessons learned
2. Add new procedures if needed
3. Review and update HEIR_GUIDE.md if applicable

---

*Operate with sovereignty. Document for succession.*
