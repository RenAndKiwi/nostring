<script lang="ts">
  import { onMount } from 'svelte';
  import { appError } from '../lib/stores';
  import { deliverDescriptorToHeirs, exportVaultBackup, compressVaultForQr, listHeirs } from '../lib/tauri';
  import type { DeliveryReport, HeirInfo } from '../lib/tauri';
  import CopyToast from '../components/CopyToast.svelte';
  import DeliveryReportCard from '../components/DeliveryReport.svelte';
  import CodeBlock from '../components/CodeBlock.svelte';
  import QrCode from '../components/QrCode.svelte';

  let nsecInput = $state('');
  let relaysInput = $state('wss://relay.damus.io\nwss://relay.primal.net');
  let loading = $state(false);
  let exportLoading = $state(false);
  let report = $state<DeliveryReport | null>(null);
  let backupJson = $state('');
  let qrData = $state('');
  let qrLoading = $state(false);
  let copyFeedback = $state<string | null>(null);
  let nsecError = $state('');
  let heirs = $state<HeirInfo[]>([]);

  const heirsWithNpub = $derived(heirs.filter(h => h.npub));
  const heirsWithoutNpub = $derived(heirs.filter(h => !h.npub));

  onMount(() => { listHeirs().then(h => heirs = h).catch(() => {}); });

  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      copyFeedback = label;
      setTimeout(() => { copyFeedback = null; }, 2000);
    } catch { appError.set('Copy failed.'); }
  }

  function downloadAsFile(content: string, filename: string) {
    const blob = new Blob([content], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = filename; a.click();
    URL.revokeObjectURL(url);
  }

  function validateNsec(): boolean {
    nsecError = '';
    if (!nsecInput.trim()) { nsecError = 'Nostr secret key is required'; return false; }
    if (!nsecInput.trim().startsWith('nsec1')) { nsecError = 'Must start with nsec1'; return false; }
    return true;
  }

  async function handleDeliver() {
    if (!validateNsec()) return;
    loading = true; appError.set(null);
    try {
      const relays = relaysInput.split('\n').map(r => r.trim()).filter(r => r.length > 0);
      const result = await deliverDescriptorToHeirs(nsecInput.trim(), relays);
      if (result.success && result.data) { report = result.data; }
      else appError.set(result.error || 'Delivery failed');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  async function handleShowQr() {
    qrLoading = true; appError.set(null);
    try {
      const result = await compressVaultForQr();
      if (result.success && result.data) qrData = result.data;
      else appError.set(result.error || 'Failed to generate QR');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    qrLoading = false;
  }

  async function handleExport() {
    exportLoading = true;
    try {
      const result = await exportVaultBackup();
      if (result.success && result.data) backupJson = result.data;
      else appError.set(result.error || 'Export failed');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    exportLoading = false;
  }
</script>

<div class="screen">
  <h1>Deliver Backup</h1>
  <p class="subtitle">Send the vault backup to your heirs via encrypted Nostr DM.</p>

  <CopyToast message={copyFeedback} />

  {#if report}
    <DeliveryReportCard {report} />
  {/if}

  <div class="card form">
    <h2>NIP-17 Delivery</h2>

    {#if heirs.length > 0 && !report}
      <div class="pre-summary">
        {#if heirsWithNpub.length > 0}
          <div class="summary-row ok">
            <span>📨 Will deliver to:</span>
            {#each heirsWithNpub as h}<span class="tag">{h.label}</span>{/each}
          </div>
        {/if}
        {#if heirsWithoutNpub.length > 0}
          <div class="summary-row warn">
            <span>⏭️ Will skip (no npub):</span>
            {#each heirsWithoutNpub as h}<span class="tag">{h.label}</span>{/each}
          </div>
        {/if}
        {#if heirsWithNpub.length === 0}
          <div class="summary-row warn">
            <span>⚠️ No heirs have npub set. Use manual export below.</span>
          </div>
        {/if}
      </div>
    {/if}

    <label>
      <span>Your Nostr Secret Key</span>
      <input type="password" bind:value={nsecInput} placeholder="nsec1..." class:input-error={nsecError} />
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

    <button class="btn btn-primary" onclick={handleDeliver} disabled={loading || heirsWithNpub.length === 0}>
      {loading ? 'Delivering...' : `📨 Deliver to ${heirsWithNpub.length} Heir${heirsWithNpub.length !== 1 ? 's' : ''}`}
    </button>
  </div>

  <hr />

  <div class="card qr-section">
    <h2>QR Code</h2>
    <p class="help">Heir scans this QR with the NoString Heir app.</p>

    <button class="btn btn-outline" onclick={handleShowQr} disabled={qrLoading}>
      {qrLoading ? 'Generating...' : '📱 Show QR Code'}
    </button>

    {#if qrData}
      <QrCode data={qrData} size={320} />
      <p class="qr-hint">nostring:v1 compressed format ({qrData.length} chars)</p>
    {/if}
  </div>

  <hr />

  <div class="card manual">
    <h2>Manual Export</h2>
    <p class="help">For heirs without Nostr, export the backup JSON and share it directly.</p>

    <button class="btn btn-outline" onclick={handleExport} disabled={exportLoading}>
      {exportLoading ? 'Exporting...' : 'Export Backup JSON'}
    </button>

    {#if backupJson}
      <div class="export-actions">
        <button class="btn btn-outline" onclick={() => copyToClipboard(backupJson, 'backup')}>📋 Copy</button>
        <button class="btn btn-outline" onclick={() => downloadAsFile(backupJson, 'nostring-vault-backup.json')}>💾 Download</button>
      </div>
      <CodeBlock label="Vault Backup" content={backupJson} onCopy={copyToClipboard} />
    {/if}
  </div>
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.2rem; margin-top: 0; }
  .subtitle { color: var(--text-muted); margin-bottom: 2rem; }

  .form, .manual { display: flex; flex-direction: column; gap: 1rem; }

  .pre-summary {
    background: var(--bg); border: 1px solid var(--border);
    border-radius: var(--radius); padding: 0.75rem;
    display: flex; flex-direction: column; gap: 0.5rem;
  }
  .summary-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; font-size: 0.85rem; }
  .summary-row.ok { color: var(--success); }
  .summary-row.warn { color: var(--warning); }
  .tag {
    display: inline-block; background: var(--surface-variant);
    padding: 0.25rem 0.5rem; border-radius: var(--radius-sm); font-size: 0.85rem;
  }

  label { display: flex; flex-direction: column; gap: 0.35rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .optional { font-weight: 400; }
  .field-error { font-size: 0.8rem; color: var(--error); font-weight: 400; }
  .input-error { border-color: var(--error) !important; }
  .help { font-size: 0.8rem; color: var(--text-muted); margin: 0; }

  .export-actions { display: flex; gap: 0.5rem; margin-top: 0.5rem; }

  hr { border: none; border-top: 1px solid var(--border); margin: 1.5rem 0; }

  .qr-section { display: flex; flex-direction: column; gap: 1rem; }
  .qr-hint { text-align: center; font-size: 0.75rem; color: var(--text-muted); margin: 0; }
</style>
