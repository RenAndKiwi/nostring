<script lang="ts">
  import { vaultAddress, vaultCreated, navigate, appError } from '../lib/stores';
  import { createCcdVault } from '../lib/tauri';

  let timelockMonths = $state(6);
  let loading = $state(false);

  // ~4,380 blocks per month (1 block ≈ 10 min)
  let timelockBlocks = $derived(Math.round(timelockMonths * 4380));

  async function handleCreate() {
    loading = true;
    try {
      const result = await createCcdVault(timelockBlocks);
      if (result.success && result.data) {
        vaultAddress.set(result.data);
        vaultCreated.set(true);
        appError.set(null);
      } else {
        appError.set(result.error || 'Vault creation failed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }
</script>

<div class="screen">
  <h1>Create Vault</h1>
  <p class="subtitle">Set the inheritance timelock and create your Taproot vault.</p>

  {#if $vaultCreated && $vaultAddress}
    <div class="vault-card">
      <h2>🔐 Vault Created</h2>
      <p class="address-label">Send Bitcoin to this address:</p>
      <div class="address">{$vaultAddress}</div>
      <p class="note">
        This is a Taproot (P2TR) address with MuSig2 key-path spending
        and heir script-path recovery after {timelockBlocks.toLocaleString()} blocks
        (~{timelockMonths} months).
      </p>
      <button class="btn primary" onclick={() => navigate('dashboard')}>
        View Dashboard →
      </button>
    </div>
  {:else}
    <div class="form">
      <label>
        <span>Inheritance Timelock</span>
        <div class="timelock-input">
          <input
            type="range"
            min="3"
            max="24"
            step="1"
            bind:value={timelockMonths}
          />
          <span class="timelock-value">
            {timelockMonths} months (~{timelockBlocks.toLocaleString()} blocks)
          </span>
        </div>
        <p class="help">
          If you don't check in within this period, your heirs can claim the funds.
          A longer timelock is more secure but requires more frequent check-ins.
        </p>
      </label>

      <button class="btn primary" onclick={handleCreate} disabled={loading}>
        {loading ? 'Creating...' : 'Create Vault'}
      </button>
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 600px; }
  h1 { font-size: 1.8rem; margin-bottom: 0.25rem; }
  h2 { color: #4ade80; margin-bottom: 0.5rem; }
  .subtitle { color: #888; margin-bottom: 2rem; }

  .form {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  label > span {
    font-size: 0.85rem;
    color: #aaa;
    font-weight: 500;
  }

  .timelock-input {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .timelock-input input[type="range"] {
    flex: 1;
    accent-color: #f7931a;
  }

  .timelock-value {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.9rem;
    color: #f7931a;
    white-space: nowrap;
  }

  .help {
    font-size: 0.8rem;
    color: #666;
    line-height: 1.5;
  }

  .vault-card {
    background: #0d2818;
    border: 1px solid #1a5c2e;
    border-radius: 8px;
    padding: 1.5rem;
  }

  .address-label {
    color: #888;
    font-size: 0.85rem;
    margin-bottom: 0.5rem;
  }

  .address {
    background: #0a0a0a;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 0.75rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
    word-break: break-all;
    color: #f7931a;
    margin-bottom: 1rem;
  }

  .note {
    font-size: 0.8rem;
    color: #888;
    line-height: 1.5;
    margin-bottom: 1.5rem;
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
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
