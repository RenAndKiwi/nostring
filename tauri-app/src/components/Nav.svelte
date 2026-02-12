<script lang="ts">
  import { currentScreen, navigate, vaultCreated, cosignerRegistered } from '../lib/stores';
  import type { Screen } from '../lib/stores';

  const tabs: { id: Screen; label: string; icon: string }[] = [
    { id: 'setup', label: 'Setup', icon: '⚙️' },
    { id: 'heirs', label: 'Heirs', icon: '👥' },
    { id: 'vault', label: 'Vault', icon: '🔐' },
    { id: 'dashboard', label: 'Status', icon: '📊' },
    { id: 'checkin', label: 'Check-in', icon: '✅' },
    { id: 'deliver', label: 'Deliver', icon: '📨' },
  ];
</script>

<nav>
  <div class="logo">
    <span class="logo-icon">🔑</span>
    <span class="logo-text">NoString</span>
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
</nav>

<style>
  nav {
    display: flex;
    align-items: center;
    gap: 2rem;
    padding: 1rem 0;
    border-bottom: 1px solid #222;
    margin-bottom: 1rem;
  }

  .logo {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .logo-icon {
    font-size: 1.5rem;
  }

  .logo-text {
    font-size: 1.2rem;
    font-weight: 700;
    color: #f7931a;
  }

  .tabs {
    display: flex;
    gap: 0.25rem;
    flex-wrap: wrap;
  }

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

  .tab:hover {
    color: #ccc;
    background: #1a1a1a;
  }

  .tab.active {
    color: #f7931a;
    border-color: #f7931a33;
    background: #f7931a11;
  }

  .tab-icon {
    font-size: 1rem;
  }
</style>
