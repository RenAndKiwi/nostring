<script lang="ts">
  import { currentScreen, navigate, appPhase, currentNetwork } from '../lib/stores';
  import { lockWallet } from '../lib/tauri';
  import type { Screen } from '../lib/stores';

  const tabs: { id: Screen; label: string; icon: string }[] = [
    { id: 'setup', label: 'Setup', icon: '⚙️' },
    { id: 'heirs', label: 'Heirs', icon: '👥' },
    { id: 'vault', label: 'Vault', icon: '🔐' },
    { id: 'dashboard', label: 'Status', icon: '📊' },
    { id: 'checkin', label: 'Check-in', icon: '✅' },
    { id: 'deliver', label: 'Deliver', icon: '📨' },
    { id: 'settings', label: 'Settings', icon: '🛠️' },
  ];

  async function handleLock() {
    await lockWallet();
    appPhase.set('unlock');
  }
</script>

<nav>
  <div class="logo">
    <span class="logo-icon">🔑</span>
    <span class="logo-text">NoString</span>
    {#if $currentNetwork !== 'bitcoin'}
      <span class="network-badge">{$currentNetwork}</span>
    {/if}
  </div>
  <div class="tabs">
    {#each tabs as tab}
      <button
        class="tab"
        class:active={$currentScreen === tab.id}
        onclick={() => navigate(tab.id)}
      >
        <span class="tab-icon">{tab.icon}</span>
        <span class="tab-label">{tab.label}</span>
      </button>
    {/each}
  </div>
  <button class="lock-btn" onclick={handleLock} title="Lock wallet">
    🔒
  </button>
</nav>

<style>
  nav {
    display: flex;
    align-items: center;
    gap: 2rem;
    padding: 1rem 0;
    border-bottom: 1px solid var(--border);
    margin-bottom: 1rem;
  }

  .logo { display: flex; align-items: center; gap: 0.5rem; }
  .logo-icon { font-size: 1.5rem; }
  .logo-text { font-size: 1.2rem; font-weight: 700; color: var(--gold-light); }
  .network-badge {
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    background: var(--warning)33;
    color: var(--warning);
    padding: 0.15rem 0.4rem;
    border-radius: 3px;
    letter-spacing: 0.05em;
  }

  .tabs { display: flex; gap: 0.25rem; flex-wrap: wrap; flex: 1; }

  .tab {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.5rem 0.75rem;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    color: #888;
    cursor: pointer;
    font-size: 0.85rem;
    transition: all 0.15s;
  }

  .tab:hover { color: #ccc; background: var(--surface); }
  .tab.active { color: var(--gold-light); border-color: var(--gold-light)33; background: var(--gold-light)11; }
  .tab-icon { font-size: 1rem; }

  .lock-btn {
    background: none;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    cursor: pointer;
    font-size: 1rem;
    transition: all 0.15s;
  }

  .lock-btn:hover { border-color: var(--gold-light); background: var(--surface); }
</style>
