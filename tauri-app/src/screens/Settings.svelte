<script lang="ts">
  import { currentNetwork, appError } from '../lib/stores';
  import { getNetwork, setNetwork, getElectrumUrl, setElectrumUrl, refreshPolicyStatus, isWatchOnly } from '../lib/tauri';
  import { onMount } from 'svelte';
  import StatusBadge from '../components/StatusBadge.svelte';

  let network = $state('bitcoin');
  let watchOnly = $state(false);
  let electrumUrl = $state('');
  let saving = $state(false);
  let saved = $state(false);
  let testing = $state(false);
  let connectionResult = $state<'success' | 'fail' | null>(null);
  let connectionDetail = $state('');

  const defaultElectrumUrls: Record<string, string> = {
    bitcoin: 'ssl://electrum.blockstream.info:50002',
    testnet: 'ssl://electrum.blockstream.info:60002',
    signet: 'ssl://mempool.space:60602',
  };

  onMount(async () => {
    try {
      network = await getNetwork();
      electrumUrl = await getElectrumUrl();
      watchOnly = await isWatchOnly();
      currentNetwork.set(network);
    } catch (e: any) {
      appError.set(`Failed to load settings: ${e}`);
    }
  });

  async function handleNetworkChange() {
    const defaultUrl = defaultElectrumUrls[network] || '';
    if (!electrumUrl || Object.values(defaultElectrumUrls).includes(electrumUrl)) {
      electrumUrl = defaultUrl;
    }
  }

  async function handleSave() {
    saving = true; saved = false; appError.set(null);
    try {
      await setNetwork(network);
      if (electrumUrl.trim()) await setElectrumUrl(electrumUrl.trim());
      currentNetwork.set(network);
      saved = true;
      setTimeout(() => { saved = false; }, 2000);
    } catch (e: any) {
      appError.set(`Failed to save settings: ${e}`);
    } finally { saving = false; }
  }

  async function handleTestConnection() {
    testing = true; connectionResult = null; connectionDetail = '';
    try {
      const result = await refreshPolicyStatus();
      if (result.success) {
        connectionResult = 'success';
        connectionDetail = result.data?.current_block
          ? `Block height: ${result.data.current_block.toLocaleString()}`
          : 'Connected';
      } else {
        connectionResult = 'fail';
        connectionDetail = result.error || 'Connection failed';
      }
    } catch (e: any) {
      connectionResult = 'fail';
      connectionDetail = e.message || 'Connection failed';
    } finally { testing = false; }
  }
</script>

<div class="screen">
  <h1>Settings</h1>

  <div class="card wallet-mode">
    {#if watchOnly}
      <div class="mode-row">
        <StatusBadge label="Watch-Only" type="success" />
        <span class="mode-icon">👁️</span>
      </div>
      <p class="mode-desc">Keys stay on your hardware wallet. This app only builds unsigned transactions.</p>
    {:else}
      <div class="mode-row">
        <StatusBadge label="Seed-Based" type="warning" />
        <span class="mode-icon">🔑</span>
      </div>
      <p class="mode-desc">This device holds key material. Consider switching to watch-only for better security.</p>
    {/if}
  </div>

  <div class="card">
    <label>
      <span>Network</span>
      <select bind:value={network} onchange={handleNetworkChange}>
        <option value="bitcoin">Mainnet</option>
        <option value="testnet">Testnet</option>
        <option value="signet">Signet</option>
      </select>
    </label>

    <label>
      <span>Electrum Server</span>
      <input type="text" bind:value={electrumUrl} placeholder={defaultElectrumUrls[network] || 'ssl://host:port'} />
      <span class="hint">Default: {defaultElectrumUrls[network] || 'none'}</span>
    </label>

    <div class="actions">
      <button class="btn btn-primary" onclick={handleSave} disabled={saving}>
        {#if saving}Saving...{:else if saved}✅ Saved{:else}Save Settings{/if}
      </button>

      <button class="btn btn-outline" onclick={handleTestConnection} disabled={testing || !electrumUrl.trim()}>
        {testing ? 'Testing...' : '🔌 Test Connection'}
      </button>
    </div>

    {#if connectionResult === 'success'}
      <div class="success-box">✅ {connectionDetail}</div>
    {:else if connectionResult === 'fail'}
      <div class="error-box">❌ {connectionDetail}</div>
    {/if}
  </div>

  {#if network !== 'bitcoin'}
    <div class="warning-box">
      ⚠️ You are on <strong>{network}</strong>. Do not use real funds.
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 500px; }
  h1 { font-size: 1.8rem; margin-bottom: 1.5rem; }

  .wallet-mode { margin-bottom: 1rem; }
  .mode-row { display: flex; align-items: center; gap: 0.5rem; }
  .mode-icon { font-size: 1.2rem; }
  .mode-desc { font-size: 0.8rem; color: var(--text-muted); margin: 0.5rem 0 0; }

  label { display: flex; flex-direction: column; gap: 0.35rem; margin-bottom: 1rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .hint { font-weight: 400 !important; font-size: 0.75rem !important; }

  select {
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.5rem; color: var(--text); font-size: 0.95rem;
  }
  select:focus { outline: none; border-color: var(--gold-light); }

  .actions { display: flex; gap: 0.75rem; margin-top: 0.5rem; }
</style>
