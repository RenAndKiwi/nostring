<script lang="ts">
  import { currentNetwork, appError } from '../lib/stores';
  import { getNetwork, setNetwork, getElectrumUrl, setElectrumUrl, refreshPolicyStatus } from '../lib/tauri';
  import { onMount } from 'svelte';

  let network = $state('bitcoin');
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
      currentNetwork.set(network);
    } catch (e: any) {
      appError.set(`Failed to load settings: ${e}`);
    }
  });

  async function handleNetworkChange() {
    // When network changes, suggest default Electrum URL
    const defaultUrl = defaultElectrumUrls[network] || '';
    if (!electrumUrl || Object.values(defaultElectrumUrls).includes(electrumUrl)) {
      electrumUrl = defaultUrl;
    }
  }

  async function handleSave() {
    saving = true;
    saved = false;
    appError.set(null);

    try {
      await setNetwork(network);
      if (electrumUrl.trim()) {
        await setElectrumUrl(electrumUrl.trim());
      }
      currentNetwork.set(network);
      saved = true;
      setTimeout(() => { saved = false; }, 2000);
    } catch (e: any) {
      appError.set(`Failed to save settings: ${e}`);
    } finally {
      saving = false;
    }
  }

  async function handleTestConnection() {
    testing = true;
    connectionResult = null;
    connectionDetail = '';
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
    } finally {
      testing = false;
    }
  }
</script>

<div class="screen">
  <h2>⚙️ Settings</h2>

  <div class="field">
    <label for="network">Network</label>
    <select id="network" bind:value={network} onchange={handleNetworkChange}>
      <option value="bitcoin">Mainnet</option>
      <option value="testnet">Testnet</option>
      <option value="signet">Signet</option>
    </select>
  </div>

  <div class="field">
    <label for="electrum">Electrum Server</label>
    <input
      id="electrum"
      type="text"
      bind:value={electrumUrl}
      placeholder={defaultElectrumUrls[network] || 'ssl://host:port'}
    />
    <small>Default: {defaultElectrumUrls[network] || 'none'}</small>
  </div>

  <button onclick={handleSave} disabled={saving}>
    {#if saving}
      Saving...
    {:else if saved}
      ✅ Saved
    {:else}
      Save Settings
    {/if}
  </button>

  <button class="test-btn" onclick={handleTestConnection} disabled={testing || !electrumUrl.trim()}>
    {#if testing}
      Testing...
    {:else}
      🔌 Test Connection
    {/if}
  </button>

  {#if connectionResult === 'success'}
    <div class="connection-ok">✅ {connectionDetail}</div>
  {:else if connectionResult === 'fail'}
    <div class="connection-fail">❌ {connectionDetail}</div>
  {/if}

  {#if network !== 'bitcoin'}
    <div class="warning">
      ⚠️ You are on <strong>{network}</strong>. Do not use real funds.
    </div>
  {/if}
</div>

<style>
  .screen {
    max-width: 500px;
    margin: 0 auto;
    padding: 1.5rem;
  }
  h2 { margin-bottom: 1.5rem; }
  .field {
    margin-bottom: 1.25rem;
  }
  .field label {
    display: block;
    font-weight: 600;
    margin-bottom: 0.35rem;
  }
  .field input, .field select {
    width: 100%;
    padding: 0.5rem;
    border: 1px solid #444;
    border-radius: 4px;
    background: #1a1a1a;
    color: #eee;
    font-size: 0.95rem;
  }
  .field small {
    color: #888;
    font-size: 0.8rem;
  }
  button {
    width: 100%;
    padding: 0.7rem;
    border: none;
    border-radius: 4px;
    background: #f7931a;
    color: #fff;
    font-weight: 600;
    font-size: 1rem;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .test-btn {
    width: 100%;
    padding: 0.6rem;
    margin-top: 0.5rem;
    border: 1px solid #444;
    border-radius: 4px;
    background: #1a1a1a;
    color: #ccc;
    font-size: 0.9rem;
    cursor: pointer;
  }
  .test-btn:hover { background: #222; border-color: #666; }
  .test-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .connection-ok {
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: #0d2818;
    border: 1px solid #1a5c2e;
    border-radius: 4px;
    color: #4ade80;
    font-size: 0.85rem;
  }
  .connection-fail {
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: #2d0d0d;
    border: 1px solid #5c1a1a;
    border-radius: 4px;
    color: #f87171;
    font-size: 0.85rem;
  }
  .warning {
    margin-top: 1rem;
    padding: 0.75rem;
    background: #3b2f00;
    border: 1px solid #f7931a;
    border-radius: 4px;
    color: #f7931a;
    font-size: 0.9rem;
  }
</style>
