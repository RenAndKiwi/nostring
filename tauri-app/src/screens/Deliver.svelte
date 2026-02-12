<script lang="ts">
  import { appError } from '../lib/stores';
  import { deliverDescriptorToHeirs, exportVaultBackup } from '../lib/tauri';
  import type { DeliveryReport } from '../lib/tauri';

  let nsecInput = $state('');
  let relaysInput = $state('wss://relay.damus.io\nwss://relay.primal.net');
  let loading = $state(false);
  let report = $state<DeliveryReport | null>(null);
  let backupJson = $state('');

  async function handleDeliver() {
    if (!nsecInput.trim()) {
      appError.set('Enter your Nostr secret key (nsec)');
      return;
    }

    loading = true;
    try {
      const relays = relaysInput
        .split('\n')
        .map(r => r.trim())
        .filter(r => r.length > 0);

      const result = await deliverDescriptorToHeirs(nsecInput.trim(), relays);
      if (result.success && result.data) {
        report = result.data;
        appError.set(null);
      } else {
        appError.set(result.error || 'Delivery failed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  async function handleExport() {
    try {
      const result = await exportVaultBackup();
      if (result.success && result.data) {
        backupJson = result.data;
      } else {
        appError.set(result.error || 'Export failed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
  }

  function copyBackup() {
    navigator.clipboard.writeText(backupJson);
  }
</script>

<div class="screen">
  <h1>Deliver Backup</h1>
  <p class="subtitle">Send the vault backup to your heirs via encrypted Nostr DM.</p>

  {#if report}
    <div class="report">
      {#if report.delivered.length > 0}
        <div class="report-section success">
          <h3>✅ Delivered</h3>
          {#each report.delivered as heir}
            <span class="tag">{heir}</span>
          {/each}
        </div>
      {/if}

      {#if report.skipped.length > 0}
        <div class="report-section warning">
          <h3>⏭️ Skipped (no npub)</h3>
          {#each report.skipped as heir}
            <span class="tag">{heir}</span>
          {/each}
          <p class="help">Export the backup manually for these heirs.</p>
        </div>
      {/if}

      {#if report.failed.length > 0}
        <div class="report-section error">
          <h3>❌ Failed</h3>
          {#each report.failed as f}
            <div class="fail-item">
              <span class="tag">{f.heir_label}</span>
              <span class="fail-reason">{f.error}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <div class="form">
    <h2>NIP-17 Delivery</h2>

    <label>
      <span>Your Nostr Secret Key</span>
      <input
        type="password"
        bind:value={nsecInput}
        placeholder="nsec1..."
      />
      <p class="help">Used to sign the encrypted DM. Not stored.</p>
    </label>

    <label>
      <span>Relays (one per line)</span>
      <textarea bind:value={relaysInput} rows="3"></textarea>
    </label>

    <button class="btn primary" onclick={handleDeliver} disabled={loading}>
      {loading ? 'Delivering...' : '📨 Deliver to Heirs'}
    </button>
  </div>

  <hr />

  <div class="manual">
    <h2>Manual Export</h2>
    <p>For heirs without Nostr, export the backup JSON and share it directly.</p>

    <button class="btn secondary" onclick={handleExport}>
      Export Backup JSON
    </button>

    {#if backupJson}
      <pre class="code-block">{backupJson}</pre>
      <button class="btn secondary" onclick={copyBackup}>📋 Copy to Clipboard</button>
    {/if}
  </div>
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.2rem; margin-top: 1.5rem; }
  .subtitle { color: #888; margin-bottom: 2rem; }

  .form {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  label span { font-size: 0.85rem; color: #aaa; font-weight: 500; }

  input, textarea {
    background: #1a1a1a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    color: #e0e0e0;
    font-size: 0.95rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  input:focus, textarea:focus { outline: none; border-color: #f7931a; }

  .help { font-size: 0.8rem; color: #666; margin: 0; }

  hr { border: none; border-top: 1px solid #333; margin: 2rem 0; }

  .report {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin-bottom: 2rem;
  }

  .report-section {
    border-radius: 8px;
    padding: 1rem;
  }

  .report-section.success { background: #0d2818; border: 1px solid #1a5c2e; }
  .report-section.warning { background: #2d2a0d; border: 1px solid #5c5a1a; }
  .report-section.error { background: #2d0d0d; border: 1px solid #5c1a1a; }

  .report-section h3 { margin: 0 0 0.5rem; font-size: 0.95rem; }

  .tag {
    display: inline-block;
    background: #333;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    font-size: 0.85rem;
    margin: 0.15rem;
  }

  .fail-item { margin: 0.5rem 0; }
  .fail-reason { color: #888; font-size: 0.8rem; margin-left: 0.5rem; }

  .code-block {
    background: #0a0a0a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.8rem;
    overflow-x: auto;
    max-height: 300px;
    overflow-y: auto;
    margin: 1rem 0;
    white-space: pre-wrap;
    word-break: break-all;
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
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
