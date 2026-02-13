<script lang="ts">
  import { appPhase, appError } from '../lib/stores';
  import { createSeed, importSeed, checkPasswordStrength } from '../lib/tauri';
  import type { PasswordStrength } from '../lib/tauri';

  type Step = 'choose' | 'create-show' | 'create-confirm' | 'create-password' | 'import-enter' | 'import-password';

  let step = $state<Step>('choose');
  let mnemonic = $state('');
  let mnemonicWords = $derived(mnemonic.split(' '));
  let confirmInput = $state('');
  let importInput = $state('');
  let passwordInput = $state('');
  let confirmPasswordInput = $state('');
  let loading = $state(false);

  let confirmError = $state('');
  let passwordError = $state('');
  let importError = $state('');
  let showPassword = $state(false);
  let passwordStrength = $state<PasswordStrength | null>(null);

  async function updatePasswordStrength() {
    if (passwordInput.length < 1) {
      passwordStrength = null;
      return;
    }
    try {
      const result = await checkPasswordStrength(passwordInput);
      if (result.success && result.data) {
        passwordStrength = result.data;
      }
    } catch {
      // Ignore — strength check is informational
    }
  }

  const strengthColor = $derived(
    !passwordStrength ? '#666' :
    passwordStrength.strength === 'VeryStrong' || passwordStrength.strength === 'Strong' ? 'var(--success)' :
    passwordStrength.strength === 'Medium' ? 'var(--gold-light)' : 'var(--error)'
  );

  async function handleCreate() {
    loading = true;
    appError.set(null);
    try {
      const result = await createSeed(24);
      if (result.success && result.data) {
        mnemonic = result.data;
        step = 'create-show';
      } else {
        appError.set(result.error || 'Failed to generate seed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  function handleConfirmMnemonic() {
    confirmError = '';
    // Ask user to type word 1, 6, 12, 24 as verification
    const words = confirmInput.trim().split(/\s+/);
    if (words.length !== 4) {
      confirmError = 'Enter exactly 4 words: word #1, #6, #12, and #24';
      return;
    }
    if (words[0] !== mnemonicWords[0] || words[1] !== mnemonicWords[5] ||
        words[2] !== mnemonicWords[11] || words[3] !== mnemonicWords[23]) {
      confirmError = 'Words don\'t match. Check your backup and try again.';
      return;
    }
    step = 'create-password';
  }

  function validatePassword(): boolean {
    passwordError = '';
    if (passwordInput.length < 8) {
      passwordError = 'Password must be at least 8 characters';
      return false;
    }
    if (passwordInput !== confirmPasswordInput) {
      passwordError = 'Passwords don\'t match';
      return false;
    }
    return true;
  }

  async function handleSetPassword() {
    if (!validatePassword()) return;

    const mnemonicToStore = step === 'import-password' ? importInput.trim() : mnemonic;

    loading = true;
    appError.set(null);
    try {
      const result = await importSeed(mnemonicToStore, passwordInput);
      if (result.success) {
        // Clear sensitive data
        mnemonic = '';
        importInput = '';
        passwordInput = '';
        confirmPasswordInput = '';
        confirmInput = '';
        appPhase.set('ready');
      } else {
        appError.set(result.error || 'Failed to store seed');
      }
    } catch (e: any) {
      appError.set(e.message || 'Unexpected error');
    }
    loading = false;
  }

  function handleImportNext() {
    importError = '';
    const words = importInput.trim().split(/\s+/);
    if (words.length !== 12 && words.length !== 15 && words.length !== 18 &&
        words.length !== 21 && words.length !== 24) {
      importError = `Expected 12, 15, 18, 21, or 24 words. Got ${words.length}.`;
      return;
    }
    step = 'import-password';
  }
</script>

<div class="screen">
  <div class="logo">🔑</div>
  <h1>NoString</h1>
  <p class="subtitle">Sovereign Bitcoin custody with inheritance</p>

  {#if step === 'choose'}
    <div class="choices">
      <button class="choice-card" onclick={handleCreate} disabled={loading}>
        <span class="choice-icon">✨</span>
        <span class="choice-title">{loading ? 'Generating...' : 'Create New Wallet'}</span>
        <span class="choice-desc">Generate a fresh 24-word seed phrase</span>
      </button>

      <button class="choice-card" onclick={() => step = 'import-enter'}>
        <span class="choice-icon">📥</span>
        <span class="choice-title">Import Existing Wallet</span>
        <span class="choice-desc">Restore from a seed phrase you already have</span>
      </button>
    </div>

  {:else if step === 'create-show'}
    <div class="step-card">
      <h2>⚠️ Write Down Your Seed Phrase</h2>
      <p class="warning-text">This is the ONLY time these words will be shown. Write them down on paper. Never store them digitally.</p>

      <div class="word-grid">
        {#each mnemonicWords as word, i}
          <div class="word-item">
            <span class="word-num">{i + 1}</span>
            <span class="word-text">{word}</span>
          </div>
        {/each}
      </div>

      <div class="confirm-notice">
        <p>After writing them down, you'll be asked to verify words <strong>#1, #6, #12, and #24</strong>.</p>
      </div>

      <button class="btn primary" onclick={() => step = 'create-confirm'}>
        I've Written Them Down →
      </button>
    </div>

  {:else if step === 'create-confirm'}
    <div class="step-card">
      <h2>Verify Your Backup</h2>
      <p>Enter words <strong>#1</strong>, <strong>#6</strong>, <strong>#12</strong>, and <strong>#24</strong> separated by spaces.</p>

      <label>
        <textarea
          bind:value={confirmInput}
          rows="2"
          placeholder="word1 word6 word12 word24"
          class:input-error={confirmError}
        ></textarea>
        {#if confirmError}
          <span class="field-error">{confirmError}</span>
        {/if}
      </label>

      <div class="actions">
        <button class="btn primary" onclick={handleConfirmMnemonic}>Verify</button>
        <button class="btn secondary" onclick={() => step = 'create-show'}>← Show Words Again</button>
      </div>
    </div>

  {:else if step === 'create-password' || step === 'import-password'}
    <div class="step-card">
      <h2>Set Encryption Password</h2>
      <p>This password encrypts your seed on disk. You'll need it each time you open the app.</p>

      <label>
        <span>Password</span>
        <div class="password-row">
          <input type={showPassword ? 'text' : 'password'} bind:value={passwordInput} placeholder="At least 8 characters" class:input-error={passwordError} oninput={updatePasswordStrength} />
          <button class="toggle-btn" type="button" onclick={() => showPassword = !showPassword}>
            {showPassword ? '🙈' : '👁️'}
          </button>
        </div>
        {#if passwordStrength}
          <div class="strength-bar">
            <div class="strength-fill" style="width: {Math.min(passwordStrength.entropy_bits / 80 * 100, 100)}%; background: {strengthColor}"></div>
          </div>
          <span class="strength-label" style="color: {strengthColor}">{passwordStrength.description}</span>
          {#if passwordStrength.warnings.length > 0}
            {#each passwordStrength.warnings as warn}
              <span class="strength-warn">⚠️ {warn}</span>
            {/each}
          {/if}
        {/if}
      </label>

      <label>
        <span>Confirm Password</span>
        <input type={showPassword ? 'text' : 'password'} bind:value={confirmPasswordInput} placeholder="Type password again" class:input-error={passwordError} />
        {#if passwordError}
          <span class="field-error">{passwordError}</span>
        {/if}
      </label>

      <button class="btn primary" onclick={handleSetPassword} disabled={loading}>
        {loading ? 'Encrypting...' : 'Create Wallet'}
      </button>
    </div>

  {:else if step === 'import-enter'}
    <div class="step-card">
      <h2>Import Seed Phrase</h2>
      <p>Enter your BIP-39 mnemonic (12, 15, 18, 21, or 24 words).</p>

      <label>
        <textarea
          bind:value={importInput}
          rows="4"
          placeholder="Enter your seed words separated by spaces..."
          class:input-error={importError}
        ></textarea>
        {#if importError}
          <span class="field-error">{importError}</span>
        {/if}
      </label>

      <div class="actions">
        <button class="btn primary" onclick={handleImportNext}>Next →</button>
        <button class="btn secondary" onclick={() => { step = 'choose'; importInput = ''; importError = ''; }}>← Back</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .screen { max-width: 480px; margin: 0 auto; }

  .logo { font-size: 3rem; text-align: center; margin-bottom: 0.5rem; }
  h1 { text-align: center; font-size: 2rem; margin: 0; color: var(--gold-light); }
  h2 { margin-top: 0; }
  .subtitle { text-align: center; color: var(--text-muted); margin-bottom: 2rem; }

  .choices { display: flex; flex-direction: column; gap: 1rem; }

  .choice-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.25rem;
    cursor: pointer;
    transition: all 0.15s;
    text-align: left;
    color: var(--text);
  }

  .choice-card:hover { border-color: var(--gold-light); }
  .choice-card:disabled { opacity: 0.5; cursor: not-allowed; }
  .choice-icon { font-size: 1.5rem; }
  .choice-title { font-size: 1.1rem; font-weight: 600; }
  .choice-desc { font-size: 0.85rem; color: var(--text-muted); }

  .step-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.5rem;
  }

  .step-card p { color: var(--text-muted); line-height: 1.5; font-size: 0.9rem; }

  .warning-text { color: var(--gold-light) !important; font-weight: 500; }

  .word-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.5rem;
    margin: 1rem 0;
  }

  .word-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.4rem 0.6rem;
  }

  .word-num { font-size: 0.7rem; color: var(--text-muted); min-width: 1.2rem; }
  .word-text { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.85rem; }

  .confirm-notice {
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: 6px;
    padding: 0.75rem;
    margin: 1rem 0;
  }

  .confirm-notice p { margin: 0; font-size: 0.85rem; color: var(--text); }

  label { display: flex; flex-direction: column; gap: 0.35rem; margin-bottom: 1rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .field-error { font-size: 0.8rem; color: var(--error); }

  input, textarea {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.75rem;
    color: var(--text);
    font-size: 0.95rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  input:focus, textarea:focus { outline: none; border-color: var(--gold-light); }
  .input-error { border-color: var(--error) !important; }

  .password-row { display: flex; gap: 0.5rem; align-items: center; }
  .password-row input { flex: 1; }

  .toggle-btn {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.6rem;
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
  }

  .toggle-btn:hover { border-color: var(--gold-light); }

  .strength-bar {
    height: 4px;
    background: var(--surface-variant);
    border-radius: 2px;
    overflow: hidden;
    margin-top: 0.25rem;
  }

  .strength-fill { height: 100%; border-radius: 2px; transition: all 0.3s; }

  .strength-label { font-size: 0.75rem; }
  .strength-warn { font-size: 0.75rem; color: var(--gold-light); }

  .actions { display: flex; gap: 1rem; }

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
