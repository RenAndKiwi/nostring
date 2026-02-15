<script lang="ts">
  import { onMount } from 'svelte';
  import { currentScreen, appError, appPhase } from './lib/stores';
  import { hasSeed, isUnlocked } from './lib/tauri';
  import Onboarding from './screens/Onboarding.svelte';
  import Unlock from './screens/Unlock.svelte';
  import Setup from './screens/Setup.svelte';
  import Heirs from './screens/Heirs.svelte';
  import Vault from './screens/Vault.svelte';
  import Dashboard from './screens/Dashboard.svelte';
  import CheckIn from './screens/CheckIn.svelte';
  import Deliver from './screens/Deliver.svelte';
  import Settings from './screens/Settings.svelte';
  import Nav from './components/Nav.svelte';

  onMount(async () => {
    try {
      const walletExists = await hasSeed();
      if (!walletExists) {
        appPhase.set('onboarding');
        return;
      }
      // Check if already unlocked (watch-only without password hash)
      const alreadyUnlocked = await isUnlocked();
      appPhase.set(alreadyUnlocked ? 'ready' : 'unlock');
    } catch {
      appPhase.set('onboarding');
    }
  });
</script>

<main>
  {#if $appPhase === 'loading'}
    <div class="loading-screen">
      <div class="logo">🔑</div>
      <p>Loading...</p>
    </div>

  {:else if $appPhase === 'onboarding'}
    {#if $appError}
      <div class="error-banner">
        <span>{$appError}</span>
        <button onclick={() => appError.set(null)}>✕</button>
      </div>
    {/if}
    <Onboarding />

  {:else if $appPhase === 'unlock'}
    {#if $appError}
      <div class="error-banner">
        <span>{$appError}</span>
        <button onclick={() => appError.set(null)}>✕</button>
      </div>
    {/if}
    <Unlock />

  {:else if $appPhase === 'ready'}
    <Nav />

    {#if $appError}
      <div class="error-banner">
        <span>{$appError}</span>
        <button onclick={() => appError.set(null)}>✕</button>
      </div>
    {/if}

    <div class="content">
      {#if $currentScreen === 'setup'}
        <Setup />
      {:else if $currentScreen === 'heirs'}
        <Heirs />
      {:else if $currentScreen === 'vault'}
        <Vault />
      {:else if $currentScreen === 'dashboard'}
        <Dashboard />
      {:else if $currentScreen === 'checkin'}
        <CheckIn />
      {:else if $currentScreen === 'deliver'}
        <Deliver />
      {:else if $currentScreen === 'settings'}
        <Settings />
      {/if}
    </div>
  {/if}
</main>

<style>
  main {
    max-width: 800px;
    margin: 0 auto;
    padding: 0 1rem;
  }

  .loading-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    color: var(--text-muted);
  }

  .loading-screen .logo { font-size: 3rem; }

  .error-banner {
    background: color-mix(in srgb, var(--error) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: var(--radius);
    padding: 0.75rem 1rem;
    margin: 1rem 0;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .error-banner button {
    background: none;
    border: none;
    color: var(--text);
    cursor: pointer;
    font-size: 1.1rem;
  }

  .content {
    padding: 1rem 0;
  }
</style>
