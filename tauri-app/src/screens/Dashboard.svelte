<script lang="ts">
  import { vaultAddress, navigate, appError } from '../lib/stores';
  import { getHeartbeatStatus, getCcdLoadError } from '../lib/tauri';
  import type { HeartbeatStatus } from '../lib/tauri';

  let heartbeat = $state<HeartbeatStatus | null>(null);
  let loadError = $state<string | null>(null);
  let loading = $state(true);

  function statusLevel(hb: HeartbeatStatus): 'healthy' | 'warning' | 'urgent' {
    if (hb.days_remaining <= 0) return 'urgent';
    if (hb.elapsed_fraction > 0.75) return 'warning';
    return 'healthy';
  }

  async function refresh() {
    loading = true;
    try {
      const errResult = await getCcdLoadError();
      if (errResult.success && errResult.data) {
        loadError = errResult.data;
      }

      const hbResult = await getHeartbeatStatus();
      if (hbResult.success && hbResult.data) {
        heartbeat = hbResult.data;
      } else if (hbResult.error) {
        // Not an app error — might just be no UTXOs yet
        heartbeat = null;
      }
    } catch (e: any) {
      appError.set(e.message || 'Failed to load status');
    }
    loading = false;
  }

  $effect(() => { refresh(); });
</script>

<div class="screen">
  <h1>Dashboard</h1>

  {#if loadError}
    <div class="warning-card">
      <span>⚠️</span>
      <div>
        <strong>Vault reconstruction warning</strong>
        <p>{loadError}</p>
      </div>
    </div>
  {/if}

  {#if $vaultAddress}
    <div class="info-card">
      <span class="label">Vault Address</span>
      <span class="mono">{$vaultAddress}</span>
    </div>
  {/if}

  {#if loading}
    <p class="loading">Loading status...</p>
  {:else if heartbeat}
    {@const level = statusLevel(heartbeat)}
    <div class="status-card" class:urgent={level === 'urgent'} class:warning={level === 'warning'}>
      <div class="status-header">
        <h2>
          {#if level === 'healthy'}
            ✅ Healthy
          {:else if level === 'warning'}
            ⚠️ Check-in Due Soon
          {:else}
            🚨 Timelock Expired
          {/if}
        </h2>
      </div>

      <div class="status-details">
        <div class="stat">
          <span class="stat-label">Days remaining</span>
          <span class="stat-value">{heartbeat.days_remaining.toFixed(1)} days ({heartbeat.blocks_remaining.toLocaleString()} blocks)</span>
        </div>
        <div class="stat">
          <span class="stat-label">Timelock progress</span>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {Math.min(heartbeat.elapsed_fraction * 100, 100)}%"></div>
          </div>
          <span class="stat-value">{(heartbeat.elapsed_fraction * 100).toFixed(1)}%</span>
        </div>
        <div class="stat">
          <span class="stat-label">Current block</span>
          <span class="stat-value mono">{heartbeat.current_block.toLocaleString()}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Expiry block</span>
          <span class="stat-value mono">{heartbeat.expiry_block.toLocaleString()}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Recommended action</span>
          <span class="stat-value">{heartbeat.action}</span>
        </div>
      </div>

      {#if level !== 'healthy'}
        <button class="btn primary" onclick={() => navigate('checkin')}>
          Check In Now →
        </button>
      {/if}
    </div>
  {:else}
    <p class="empty">
      No heartbeat data available. Fund your vault and wait for confirmation.
    </p>
  {/if}

  <div class="actions">
    <button class="btn secondary" onclick={refresh}>🔄 Refresh</button>
    <button class="btn secondary" onclick={() => navigate('checkin')}>Check In</button>
    <button class="btn secondary" onclick={() => navigate('deliver')}>Deliver Backup</button>
  </div>
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 1.5rem; }
  h2 { margin: 0; }

  .loading { color: #888; }
  .empty { color: #888; }
  .mono {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
    word-break: break-all;
    color: #f7931a;
  }

  .info-card {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .label { font-size: 0.8rem; color: #888; }

  .warning-card {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    background: #4a3a1c;
    border: 1px solid #8b7030;
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1rem;
  }

  .warning-card p { margin: 0.25rem 0 0; color: #ccc; font-size: 0.85rem; }

  .status-card {
    background: #0d2818;
    border: 1px solid #1a5c2e;
    border-radius: 8px;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }

  .status-card.warning {
    background: #2d2a0d;
    border-color: #5c5a1a;
  }

  .status-card.urgent {
    background: #2d0d0d;
    border-color: #5c1a1a;
  }

  .status-details {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin: 1rem 0;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat-label { font-size: 0.8rem; color: #888; }
  .stat-value { font-size: 0.95rem; }

  .progress-bar {
    height: 8px;
    background: #333;
    border-radius: 4px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: #f7931a;
    border-radius: 4px;
    transition: width 0.3s;
  }

  .actions {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .btn {
    padding: 0.75rem 1.5rem;
    border: none;
    border-radius: 6px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn.primary { background: #f7931a; color: #000; }
  .btn.primary:hover { background: #f9a84d; }
  .btn.secondary { background: #333; color: #e0e0e0; }
  .btn.secondary:hover { background: #444; }
</style>
