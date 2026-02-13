<script lang="ts">
  import { onMount } from 'svelte';
  import { appError } from '../lib/stores';
  import { deliverDescriptorToHeirs, exportVaultBackup, listHeirs } from '../lib/tauri';
  import type { DeliveryReport, HeirInfo } from '../lib/tauri';

  let nsecInput = $state('');
  let relaysInput = $state('wss://relay.damus.io\nwss://relay.primal.net');
  let loading = $state(false);
  let exportLoading = $state(false);
  let report = $state<DeliveryReport | null>(null);
  let backupJson = $state('');
  let copyFeedback = $state<string | null>(null);

  // Validation
  let nsecError = $state('');

  // Pre-delivery summary
  let heirs = $state<HeirInfo[]>([]);

  const heirsWithNpub = $derived(heirs.filter(h => h.npub));
  const heirsWithoutNpub = $derived(heirs.filter(h => !h.npub));

  onMount(() => {
    listHeirs().then(h => heirs = h).catch(() => {});
  });

  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      copyFeedback = label;
      setTimeout(() => { copyFeedback = null; }, 2000);
    } catch {
      appError.set('Copy failed. Please select and copy manually.');
    }
  }

  function downloadAsFile(content: string, filename: string) {
    const blob = new Blob([content], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  }

  function validateNsec(): boolean {
    nsecError = '';
    if (!nsecInput.trim()) {
      nsecError = 'Nostr secret key is required';
      return false;
    }
    if (!nsecInput.trim().startsWith('nsec1')) {
      nsecError = 'Must start with nsec1';
      return false;
    }
    return true;
  }

  async function handleDeliver() {
    if (!validateNsec()) return;

    loading = true;
    appError.set(null);
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
    exportLoading = true;
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
    exportLoading = false;
  }
</script>

<div class="screen">
  <h1>Deliver Backup</h1>
  <p class="subtitle">Send the vault backup to your heirs via encrypted Nostr DM.</p>

  <!-- Copy toast -->
  {#if copyFeedback}
    <div class="copy-toast">Copied {copyFeedback}!</div>
  {/if}

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

    <!-- Pre-delivery summary -->
    {#if heirs.length > 0 && !report}
      <div class="pre-summary">
        {#if heirsWithNpub.length > 0}
          <div class="summary-row ok">
            <span>📨 Will deliver to:</span>
            {#each heirsWithNpub as h}
              <span class="tag">{h.label}</span>
            {/each}
          </div>
        {/if}
        {#if heirsWithoutNpub.length > 0}
          <div class="summary-row warn">
            <span>⏭️ Will skip (no npub):</span>
            {#each heirsWithoutNpub as h}
              <span class="tag">{h.label}</span>
            {/each}
          </div>
        {/if}
        {#if heirsWithNpub.length === 0}
          <div class="summary-row warn">
            <span>⚠️ No heirs have npub set. NIP-17 delivery won't send to anyone. Use manual export below.</span>
          </div>
        {/if}
      </div>
    {/if}

    <label>
      <span>Your Nostr Secret Key</span>
      <input
        type="password"
        bind:value={nsecInput}
        placeholder="nsec1..."
        class:input-error={nsecError}
      />
      {#if nsecError}
        <span class="field-error">{nsecError}</span>
      {:else}
        <p class="help">Used to sign the encrypted DM. Not stored.</p>
      {/if}
    </label>

    <label>
      <span>Relays <span class="optional">(one per line)</span></span>
      <textarea bind:value={relaysInput} rows="3"></textarea>
    </label>

    <button class="btn primary" onclick={handleDeliver} disabled={loading || heirsWithNpub.length === 0}>
      {loading ? 'Delivering...' : `📨 Deliver to ${heirsWithNpub.length} Heir${heirsWithNpub.length !== 1 ? 's' : ''}`}
    </button>
  </div>

  <hr />

  <div class="manual">
    <h2>Manual Export</h2>
    <p>For heirs without Nostr, export the backup JSON and share it directly (USB, print, etc.).</p>

    <button class="btn secondary" onclick={handleExport} disabled={exportLoading}>
      {exportLoading ? 'Exporting...' : 'Export Backup JSON'}
    </button>

    {#if backupJson}
      <div class="export-result">
        <div class="code-header">
          <span class="code-label">Vault Backup</span>
          <div class="code-actions">
            <button class="copy-btn" onclick={() => copyToClipboard(backupJson, 'backup')}>Copy</button>
            <button class="copy-btn" onclick={() => downloadAsFile(backupJson, 'nostring-vault-backup.json')}>Download</button>
          </div>
        </div>
        <pre class="code-block">{backupJson}</pre>
      </div>
    {/if}
  </div>
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.2rem; margin-top: 1.5rem; }
  .subtitle { color: var(--text-muted); margin-bottom: 2rem; }

  .copy-toast {
    position: fixed;
    top: 1rem;
    right: 1rem;
    background: #1a5c2e;
    border: 1px solid #2a8c4e;
    color: var(--text);
    padding: 0.5rem 1rem;
    border-radius: 6px;
    font-size: 0.85rem;
    z-index: 100;
  }

  .form { display: flex; flex-direction: column; gap: 1.25rem; }

  .pre-summary {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .summary-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; font-size: 0.85rem; }
  .summary-row.ok { color: var(--success); }
  .summary-row.warn { color: var(--gold-light); }

  label { display: flex; flex-direction: column; gap: 0.35rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .optional { font-weight: 400; color: var(--text-muted); }

  .field-error { font-size: 0.8rem; color: var(--error); font-weight: 400; }

  input, textarea {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.75rem;
    color: var(--text);
    font-size: 0.95rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  input:focus, textarea:focus { outline: none; border-color: var(--gold-light); }
  .input-error { border-color: var(--error) !important; }

  .help { font-size: 0.8rem; color: var(--text-muted); margin: 0; }

  hr { border: none; border-top: 1px solid #333; margin: 2rem 0; }

  .report { display: flex; flex-direction: column; gap: 1rem; margin-bottom: 2rem; }

  .report-section { border-radius: 8px; padding: 1rem; }
  .report-section.success { background: #0d2818; border: 1px solid #1a5c2e; }
  .report-section.warning { background: #2d2a0d; border: 1px solid #5c5a1a; }
  .report-section.error { background: #2d0d0d; border: 1px solid #5c1a1a; }
  .report-section h3 { margin: 0 0 0.5rem; font-size: 0.95rem; }

  .tag {
    display: inline-block;
    background: var(--surface-variant);
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    font-size: 0.85rem;
    margin: 0.15rem;
  }

  .fail-item { margin: 0.5rem 0; }
  .fail-reason { color: var(--text-muted); font-size: 0.8rem; margin-left: 0.5rem; }

  .export-result { margin-top: 1rem; }

  .code-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .code-label { font-size: 0.8rem; color: var(--text-muted); font-weight: 500; }

  .code-actions { display: flex; gap: 0.5rem; }

  .copy-btn {
    background: #252525;
    border: 1px solid #444;
    border-radius: 4px;
    padding: 0.25rem 0.5rem;
    color: var(--text);
    font-size: 0.75rem;
    cursor: pointer;
    transition: all 0.15s;
  }

  .copy-btn:hover { background: var(--surface-variant); border-color: var(--gold-light); }

  .code-block {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.75rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.8rem;
    overflow-x: auto;
    max-height: 300px;
    overflow-y: auto;
    margin: 0.25rem 0 1rem;
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

  .btn.primary { background: var(--gold-light); color: #000; }
  .btn.primary:hover { background: var(--gold); }
  .btn.secondary { background: var(--surface-variant); color: var(--text); }
  .btn.secondary:hover { background: var(--border); }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
