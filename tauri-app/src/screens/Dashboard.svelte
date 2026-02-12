<script lang="ts">
  import { vaultAddress, navigate, appError } from '../lib/stores';
  import { getHeartbeatStatus, getCcdLoadError } from '../lib/tauri';
  import type { HeartbeatStatus } from '../lib/tauri';

  let heartbeat = $state<HeartbeatStatus | null>(null);
  let loadError = $state<string | null>(null);
  let loading = $state(true);

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
        appError.set(hbResult.error);
      }
    } catch (e: any) {
      appError.set(e.message || 'Failed to load status');
    }
    loading = false;
  }

  // Load on mount
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
    <div class="status-card" class:urgent={heartbeat.status === 'overdue'} class:warning={heartbeat.status === 'due_soon'}>
      <div class="status-header">
        <h2>
          {#if heartbeat.status === 'healthy'}
            ✅ Healthy
          {:else if heartbeat.status === 'due_soon'}
            ⚠️ Check-in Due Soon
          {:else}
            🚨 Overdue
          {/if}
        </h2>
      </div>

      <div class="status-details">
        <div class="stat">
          <span class="stat-label">Days since check-in</span>
          <span class="stat-value">{heartbeat.days_since_checkin}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Timelock progress</span>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {Math.min(heartbeat.elapsed_fraction * 100, 100)}%"></div>
          </div>
          <span class="stat-value">{(heartbeat.elapsed_fraction * 100).toFixed(1)}%</span>
        </div>
        <div class="stat">
          <span class="stat-label">Recommended action</span>
          <span class="stat-value">{heartbeat.recommended_action}</span>
        </div>
      </div>

      {#if heartbeat.status !== 'healthy'}
        <button class="btn primary" onclick={() => navigate('checkin')}>
          Check In Now →
        </button>
      {/if}
    </div>
  {:else}
    <p class="empty">No vault configured. <button class="link-btn" onclick={() => navigate('vault')}>Create one</button>.</p>
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
  .link-btn {
    background: none;
    border: none;
    color: #f7931a;
    cursor: pointer;
    padding: 0;
    font: inherit;
    text-decoration: underline;
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
  .mono {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
    word-break: break-all;
    color: #f7931a;
  }

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
