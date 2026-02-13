<script lang="ts">
  import { appPhase, appError } from '../lib/stores';
  import { unlockSeed } from '../lib/tauri';

  let passwordInput = $state('');
  let loading = $state(false);
  let passwordError = $state('');
  let showPassword = $state(false);

  async function handleUnlock() {
    passwordError = '';
    if (!passwordInput) {
      passwordError = 'Password is required';
      return;
    }

    loading = true;
    appError.set(null);
    try {
      const result = await unlockSeed(passwordInput);
      if (result.success) {
        passwordInput = '';
        appPhase.set('ready');
      } else {
        passwordError = result.error || 'Wrong password';
        passwordInput = '';
      }
    } catch (e: any) {
      passwordError = e.message || 'Unexpected error';
    }
    loading = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleUnlock();
  }
</script>

<div class="screen">
  <div class="logo">🔒</div>
  <h1>NoString</h1>
  <p class="subtitle">Enter your password to unlock</p>

  <div class="unlock-card">
    <label>
      <div class="password-row">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type={showPassword ? 'text' : 'password'}
          bind:value={passwordInput}
          placeholder="Encryption password"
          class:input-error={passwordError}
          onkeydown={handleKeydown}
          autofocus
        />
        <button class="toggle-btn" type="button" onclick={() => showPassword = !showPassword}>
          {showPassword ? '🙈' : '👁️'}
        </button>
      </div>
      {#if passwordError}
        <span class="field-error">{passwordError}</span>
      {/if}
    </label>

    <button class="btn primary" onclick={handleUnlock} disabled={loading}>
      {loading ? 'Unlocking...' : '🔓 Unlock'}
    </button>
  </div>
</div>

<style>
  .screen {
    max-width: 400px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 4rem;
  }

  .logo { font-size: 3rem; margin-bottom: 0.5rem; }
  h1 { font-size: 2rem; margin: 0; color: var(--gold-light); }
  .subtitle { color: var(--text-muted); margin-bottom: 2rem; }

  .unlock-card {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  label { display: flex; flex-direction: column; gap: 0.35rem; }
  .field-error { font-size: 0.8rem; color: var(--error); }

  .password-row { display: flex; gap: 0.5rem; align-items: center; }
  .password-row input { flex: 1; }

  .toggle-btn {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.7rem;
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
  }

  .toggle-btn:hover { border-color: var(--gold-light); }

  input {
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.85rem;
    color: var(--text);
    font-size: 1rem;
    text-align: center;
    box-sizing: border-box;
  }

  input:focus { outline: none; border-color: var(--gold-light); }
  .input-error { border-color: var(--error) !important; }

  .btn {
    padding: 0.85rem 1.5rem;
    border: none;
    border-radius: 6px;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn.primary { background: var(--gold-light); color: #000; }
  .btn.primary:hover { background: var(--gold); }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
