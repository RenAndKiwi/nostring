<script lang="ts">
  import { appPhase, appError } from '../lib/stores';
  import { importWatchOnly, createSeed, importSeed, checkPasswordStrength, setNetwork, setElectrumUrl, validateXpub } from '../lib/tauri';
  import type { PasswordStrength } from '../lib/tauri';
  import { validateXpubInput, xpubPrefixForNetwork, defaultElectrumUrl } from '../lib/validation';

  type Step =
    | 'choose'
    | 'xpub-network' | 'xpub-enter' | 'xpub-password'
    | 'create-show' | 'create-confirm' | 'create-password'
    | 'import-enter' | 'import-password';

  let step = $state<Step>('choose');
  let showAdvanced = $state(false);

  // Network
  let selectedNetwork = $state('testnet');

  const xpubPrefix = $derived(xpubPrefixForNetwork(selectedNetwork));

  // Watch-only
  let xpubInput = $state('');
  let xpubError = $state('');

  // Seed create/import
  let mnemonic = $state('');
  let mnemonicWords = $derived(mnemonic.split(' '));
  let confirmInput = $state('');
  let importInput = $state('');
  let confirmError = $state('');
  let importError = $state('');

  // Password (shared)
  let passwordInput = $state('');
  let confirmPasswordInput = $state('');
  let passwordError = $state('');
  let showPassword = $state(false);
  let passwordStrength = $state<PasswordStrength | null>(null);
  let loading = $state(false);

  async function updatePasswordStrength() {
    if (passwordInput.length < 1) { passwordStrength = null; return; }
    try {
      const result = await checkPasswordStrength(passwordInput);
      if (result.success && result.data) passwordStrength = result.data;
    } catch {}
  }

  const strengthColor = $derived(
    !passwordStrength ? '#666' :
    passwordStrength.strength === 'VeryStrong' || passwordStrength.strength === 'Strong' ? 'var(--success)' :
    passwordStrength.strength === 'Medium' ? 'var(--gold-light)' : 'var(--error)'
  );

  function validatePassword(): boolean {
    passwordError = '';
    if (passwordInput.length < 8) { passwordError = 'Password must be at least 8 characters'; return false; }
    if (passwordInput !== confirmPasswordInput) { passwordError = 'Passwords don\'t match'; return false; }
    return true;
  }

  // Watch-only xpub
  function handleNetworkNext() {
    // Don't persist yet — wait until full onboarding completes
    step = 'xpub-enter';
  }

  async function handleXpubNext() {
    const result = validateXpubInput(xpubInput, selectedNetwork);
    xpubError = result.error;
    if (!result.valid) return;

    // Validate xpub cryptographically via backend
    loading = true;
    try {
      const backendCheck = await validateXpub(xpubInput.trim());
      if (!backendCheck.success) {
        xpubError = backendCheck.error || 'Invalid xpub format';
        loading = false;
        return;
      }
    } catch (e: any) {
      xpubError = e.message || 'Failed to validate xpub';
      loading = false;
      return;
    }
    loading = false;
    step = 'xpub-password';
  }

  async function handleXpubFinish() {
    if (!validatePassword()) return;
    loading = true; appError.set(null);
    try {
      // Persist network + Electrum URL first
      await setNetwork(selectedNetwork);
      const url = defaultElectrumUrl(selectedNetwork);
      if (url) await setElectrumUrl(url);

      const result = await importWatchOnly(xpubInput.trim(), passwordInput);
      if (result.success) {
        clearSensitive();
        appPhase.set('ready');
      } else appError.set(result.error || 'Failed to import xpub');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  // Seed create
  async function handleCreate() {
    loading = true; appError.set(null);
    try {
      const result = await createSeed(24);
      if (result.success && result.data) { mnemonic = result.data; step = 'create-show'; }
      else appError.set(result.error || 'Failed to generate seed');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  function handleConfirmMnemonic() {
    confirmError = '';
    const words = confirmInput.trim().split(/\s+/);
    if (words.length !== 4) { confirmError = 'Enter exactly 4 words: #1, #6, #12, #24'; return; }
    if (words[0] !== mnemonicWords[0] || words[1] !== mnemonicWords[5] ||
        words[2] !== mnemonicWords[11] || words[3] !== mnemonicWords[23]) {
      confirmError = 'Words don\'t match. Check your backup.';
      return;
    }
    step = 'create-password';
  }

  // Seed import
  function handleImportNext() {
    importError = '';
    const words = importInput.trim().split(/\s+/);
    if (![12, 15, 18, 21, 24].includes(words.length)) {
      importError = `Expected 12, 15, 18, 21, or 24 words. Got ${words.length}.`;
      return;
    }
    step = 'import-password';
  }

  async function handleSeedPassword() {
    if (!validatePassword()) return;
    const mnemonicToStore = step === 'import-password' ? importInput.trim() : mnemonic;
    loading = true; appError.set(null);
    try {
      const result = await importSeed(mnemonicToStore, passwordInput);
      if (result.success) { clearSensitive(); appPhase.set('ready'); }
      else appError.set(result.error || 'Failed to store seed');
    } catch (e: any) { appError.set(e.message || 'Unexpected error'); }
    loading = false;
  }

  function clearSensitive() {
    mnemonic = ''; importInput = ''; passwordInput = '';
    confirmPasswordInput = ''; confirmInput = ''; xpubInput = '';
  }
</script>

<div class="screen">
  <div class="logo">🔑</div>
  <h1>NoString</h1>
  <p class="subtitle">Sovereign Bitcoin custody with inheritance</p>

  {#if step === 'choose'}
    <div class="choices">
      <button class="choice-card primary-choice" onclick={() => step = 'xpub-network'}>
        <span class="choice-icon">👁️</span>
        <span class="choice-title">Import Watch-Only Wallet</span>
        <span class="choice-desc">Paste your xpub from a hardware wallet. Keys stay on your signing device. <strong>Recommended.</strong></span>
      </button>

      {#if !showAdvanced}
        <button class="advanced-toggle" onclick={() => showAdvanced = true}>
          Advanced options ▾
        </button>
      {:else}
        <div class="advanced-section">
          <p class="advanced-warning">⚠️ These options store key material on this device. Use watch-only above unless you have a specific reason.</p>

          <button class="choice-card" onclick={handleCreate} disabled={loading}>
            <span class="choice-icon">✨</span>
            <span class="choice-title">{loading ? 'Generating...' : 'Create New Seed'}</span>
            <span class="choice-desc">Generate a fresh 24-word seed phrase</span>
          </button>

          <button class="choice-card" onclick={() => step = 'import-enter'}>
            <span class="choice-icon">📥</span>
            <span class="choice-title">Import Seed Phrase</span>
            <span class="choice-desc">Restore from an existing mnemonic</span>
          </button>
        </div>
      {/if}
    </div>

  {:else if step === 'xpub-network'}
    <div class="step-card">
      <h2>Select Network</h2>
      <p>Choose the Bitcoin network before importing your key. Use <strong>Testnet</strong> to try things out with fake coins first.</p>

      <div class="network-options">
        <label class="network-option" class:selected={selectedNetwork === 'testnet'}>
          <input type="radio" bind:group={selectedNetwork} value="testnet" />
          <div>
            <span class="net-title">🧪 Testnet</span>
            <span class="net-desc">Free test coins. Use this to learn the app.</span>
          </div>
        </label>
        <label class="network-option" class:selected={selectedNetwork === 'bitcoin'}>
          <input type="radio" bind:group={selectedNetwork} value="bitcoin" />
          <div>
            <span class="net-title">₿ Mainnet</span>
            <span class="net-desc">Real Bitcoin. Only if you know what you're doing.</span>
          </div>
        </label>
        <label class="network-option" class:selected={selectedNetwork === 'signet'}>
          <input type="radio" bind:group={selectedNetwork} value="signet" />
          <div>
            <span class="net-title">🔬 Signet</span>
            <span class="net-desc">Stable test network for developers.</span>
          </div>
        </label>
      </div>

      {#if selectedNetwork === 'bitcoin'}
        <div class="mainnet-warning">
          ⚠️ This software is in early development. Use testnet first to verify the flow works correctly before putting real funds at risk.
        </div>
      {/if}

      <div class="actions">
        <button class="btn btn-primary" onclick={handleNetworkNext}>Next →</button>
        <button class="btn btn-outline" onclick={() => step = 'choose'}>← Back</button>
      </div>
    </div>

  {:else if step === 'xpub-enter'}
    <div class="step-card">
      <h2>Import Watch-Only Wallet</h2>
      <p>Paste your extended public key from your hardware wallet. Export at derivation path <code>m/86'/0'/0'</code> (mainnet) or <code>m/86'/1'/0'</code> (testnet).</p>
      <p>Your private keys never leave your signing device.</p>

      <label>
        <span>Extended Public Key</span>
        <textarea bind:value={xpubInput} rows="4" placeholder="{xpubPrefix}D6NzVbkrYhZ4... or [fingerprint/86'/{selectedNetwork === 'bitcoin' ? '0' : '1'}'/0']{xpubPrefix}..." class:input-error={xpubError}></textarea>
        {#if xpubError}<span class="field-error">{xpubError}</span>{/if}
      </label>

      <div class="actions">
        <button class="btn btn-primary" onclick={handleXpubNext}>Next →</button>
        <button class="btn btn-outline" onclick={() => { step = 'choose'; xpubInput = ''; xpubError = ''; }}>← Back</button>
      </div>
    </div>

  {:else if step === 'xpub-password' || step === 'create-password' || step === 'import-password'}
    <div class="step-card">
      <h2>Set Encryption Password</h2>
      <p>{step === 'xpub-password' ? 'This password encrypts your local database.' : 'This password encrypts your seed on disk.'} You\'ll need it each time you open the app.</p>

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
        {#if passwordError}<span class="field-error">{passwordError}</span>{/if}
      </label>

      <button class="btn btn-primary" onclick={step === 'xpub-password' ? handleXpubFinish : handleSeedPassword} disabled={loading}>
        {loading ? 'Encrypting...' : step === 'xpub-password' ? 'Import Wallet' : 'Create Wallet'}
      </button>
    </div>

  {:else if step === 'create-show'}
    <div class="step-card">
      <h2>⚠️ Write Down Your Seed Phrase</h2>
      <p class="warning-text">This is the ONLY time these words will be shown. Write them down on paper. Never store digitally.</p>

      <div class="word-grid">
        {#each mnemonicWords as word, i}
          <div class="word-item">
            <span class="word-num">{i + 1}</span>
            <span class="word-text">{word}</span>
          </div>
        {/each}
      </div>

      <div class="confirm-notice">
        <p>You'll verify words <strong>#1, #6, #12, and #24</strong> next.</p>
      </div>

      <button class="btn btn-primary" onclick={() => step = 'create-confirm'}>I've Written Them Down →</button>
    </div>

  {:else if step === 'create-confirm'}
    <div class="step-card">
      <h2>Verify Your Backup</h2>
      <p>Enter words <strong>#1</strong>, <strong>#6</strong>, <strong>#12</strong>, and <strong>#24</strong> separated by spaces.</p>

      <label>
        <textarea bind:value={confirmInput} rows="2" placeholder="word1 word6 word12 word24" class:input-error={confirmError}></textarea>
        {#if confirmError}<span class="field-error">{confirmError}</span>{/if}
      </label>

      <div class="actions">
        <button class="btn btn-primary" onclick={handleConfirmMnemonic}>Verify</button>
        <button class="btn btn-outline" onclick={() => step = 'create-show'}>← Show Words Again</button>
      </div>
    </div>

  {:else if step === 'import-enter'}
    <div class="step-card">
      <h2>Import Seed Phrase</h2>
      <p>Enter your BIP-39 mnemonic, all words separated by spaces.</p>

      <label>
        <textarea bind:value={importInput} rows="4" placeholder="word1 word2 word3 ..." class:input-error={importError}></textarea>
        {#if importError}<span class="field-error">{importError}</span>{/if}
      </label>

      <div class="actions">
        <button class="btn btn-primary" onclick={handleImportNext}>Next →</button>
        <button class="btn btn-outline" onclick={() => { step = 'choose'; importInput = ''; importError = ''; }}>← Back</button>
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
    display: flex; flex-direction: column; align-items: flex-start; gap: 0.25rem;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 1.25rem; cursor: pointer; transition: all 0.15s; text-align: left; color: var(--text);
  }
  .choice-card:hover { border-color: var(--gold-light); }
  .choice-card:disabled { opacity: 0.5; cursor: not-allowed; }
  .primary-choice { border-color: var(--gold-light); background: color-mix(in srgb, var(--gold-light) 5%, var(--surface)); }
  .choice-icon { font-size: 1.5rem; }
  .choice-title { font-size: 1.1rem; font-weight: 600; }
  .choice-desc { font-size: 0.85rem; color: var(--text-muted); }

  .advanced-toggle {
    background: none; border: none; color: var(--text-muted);
    cursor: pointer; font-size: 0.85rem; padding: 0.5rem;
    text-align: center;
  }
  .advanced-toggle:hover { color: var(--text); }
  .advanced-section { display: flex; flex-direction: column; gap: 1rem; }
  .advanced-warning {
    font-size: 0.8rem; color: var(--warning);
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 25%, transparent);
    border-radius: var(--radius-sm); padding: 0.75rem; margin: 0;
  }

  .step-card {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: var(--radius); padding: 1.5rem;
  }
  .step-card p { color: var(--text-muted); line-height: 1.5; font-size: 0.9rem; }
  .warning-text { color: var(--gold-light) !important; font-weight: 500; }
  code {
    background: var(--bg); border: 1px solid var(--border); border-radius: 3px;
    padding: 0.15rem 0.35rem; font-size: 0.8rem;
  }

  .word-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.5rem; margin: 1rem 0; }
  .word-item {
    display: flex; align-items: center; gap: 0.5rem;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px; padding: 0.4rem 0.6rem;
  }
  .word-num { font-size: 0.7rem; color: var(--text-muted); min-width: 1.2rem; }
  .word-text { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.85rem; }

  .confirm-notice {
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: 6px; padding: 0.75rem; margin: 1rem 0;
  }
  .confirm-notice p { margin: 0; font-size: 0.85rem; color: var(--text); }

  label { display: flex; flex-direction: column; gap: 0.35rem; margin-bottom: 1rem; }
  label span { font-size: 0.85rem; color: var(--text-muted); font-weight: 500; }
  .field-error { font-size: 0.8rem; color: var(--error); }

  .password-row { display: flex; gap: 0.5rem; align-items: center; }
  .password-row input { flex: 1; }
  .toggle-btn {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 6px; padding: 0.6rem; cursor: pointer; font-size: 1rem; line-height: 1;
  }
  .toggle-btn:hover { border-color: var(--gold-light); }

  .strength-bar { height: 4px; background: var(--surface-variant); border-radius: 2px; overflow: hidden; margin-top: 0.25rem; }
  .strength-fill { height: 100%; border-radius: 2px; transition: all 0.3s; }
  .strength-label { font-size: 0.75rem; }
  .strength-warn { font-size: 0.75rem; color: var(--gold-light); }

  .actions { display: flex; gap: 1rem; }

  .network-options { display: flex; flex-direction: column; gap: 0.5rem; margin: 1rem 0; }
  .network-option {
    display: flex; align-items: center; gap: 0.75rem;
    background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.75rem 1rem; cursor: pointer; transition: border-color 0.15s;
  }
  .network-option:hover { border-color: var(--gold-light); }
  .network-option.selected { border-color: var(--gold-light); background: color-mix(in srgb, var(--gold-light) 5%, var(--bg)); }
  .network-option input[type="radio"] { accent-color: var(--gold-light); }
  .network-option div { display: flex; flex-direction: column; }
  .net-title { font-weight: 600; font-size: 0.95rem; }
  .net-desc { font-size: 0.8rem; color: var(--text-muted); }

  .mainnet-warning {
    background: color-mix(in srgb, var(--error) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: var(--radius-sm); padding: 0.75rem; margin: 0.5rem 0;
    font-size: 0.85rem; color: var(--error);
  }
</style>
