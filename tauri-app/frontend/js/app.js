// NoString Desktop App - Frontend Logic
// Brand colors aligned with Bitcoin Butlers

// Demo mode when running outside Tauri
const DEMO_MODE = !window.__TAURI__;

const invoke = DEMO_MODE 
    ? async (cmd, args) => {
        console.log('[DEMO] invoke:', cmd, args);
        // Mock responses for demo mode
        const mocks = {
            'has_seed': false, // checks seed OR xpub (i.e., "has wallet")
            'create_seed': { 
                success: true, 
                data: 'zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong' // placeholder 12-word mnemonic for demo
            },
            'store_seed': { success: true },
            'unlock': { success: true },
            'get_policy_status': { 
                isActive: true, 
                lastCheckin: Date.now() - 86400000 * 2,
                nextDue: Date.now() + 86400000 * 28,
                blockHeight: 934812,
                heirsCount: 3
            },
            'list_heirs': [
                { label: 'Spouse', fingerprint: 'a1b2c3d4', timelock: '6 months' },
                { label: 'Children', fingerprint: 'e5f6g7h8', timelock: '12 months' },
            ],
            'initiate_checkin': { success: true, psbt: 'cHNidP8BAH...' },
            'import_seed': { success: true },
            'import_watch_only': { success: true },
            'get_descriptor_backup': {
                success: true,
                data: {
                    descriptor: 'wsh(or_d(pk([owner_fp/84h/0h/0h]xpub6ABC.../0/*),and_v(v:pk([heir_fp/84h/0h/0h]xpub6DEF.../0/*),older(26280))))',
                    network: 'bitcoin',
                    timelock_blocks: 26280,
                    address: 'bc1q_example_inheritance_address',
                    heirs: [
                        { label: 'Spouse', xpub: 'xpub6DEF...', timelock_months: 6 }
                    ]
                }
            },
            'unlock_seed': { success: true },
            'lock_wallet': { success: true },
            'get_electrum_url': 'ssl://blockstream.info:700',
            'set_electrum_url': { success: true },
            'add_heir': { success: true },
            'remove_heir': { success: true },
            'refresh_policy_status': { 
                success: true, 
                data: {
                    urgency: 'ok',
                    days_remaining: 28.5,
                    blocks_remaining: 4104,
                    current_block: 934812
                }
            },
            'generate_codex32_shares': {
                success: true,
                data: [
                    'ms10testsxxxxxxxxxxxxxxxxxxxxxxxxxx',
                    'ms10testayyyyyyyyyyyyyyyyyyyyyyyyyy', 
                    'ms10testbzzzzzzzzzzzzzzzzzzzzzzzzzz'
                ]
            },
            'generate_service_key': {
                success: true,
                data: 'npub1demo0servicekey0placeholder0000000000000000000000000000000'
            },
            'get_service_npub': 'npub1demo0servicekey0placeholder0000000000000000000000000000000',
            'split_nsec': { success: true, data: {
                owner_npub: 'npub1demo_owner_identity_placeholder',
                pre_distributed: [
                    { heir_label: 'Spouse', heir_fingerprint: 'a1b2c3d4', share: 'ms12nsecaxxxxxxxxxxxxxxxxxxxxxxxxxx' },
                ],
                locked_shares: ['ms12nsecbyyyyyyyyyyyyyyyyyyyyyyy', 'ms12nsecczzzzzzzzzzzzzzzzzzzzzzz'],
                threshold: 2,
                total_shares: 3,
                was_resplit: false,
                previous_npub: null,
            }},
            'get_nsec_inheritance_status': { configured: false, owner_npub: null, locked_share_count: 0 },
            'revoke_nsec_inheritance': { success: true },
            'get_locked_shares': null,
            'recover_nsec': { success: true, data: { nsec: 'nsec1demorecovered...', npub: 'npub1demorecovered...' } },
            'configure_notifications': { success: true },
            'get_notification_settings': { owner_npub: null, email_address: null, email_smtp_host: null, service_npub: 'npub1demo...' },
            'send_test_notification': { success: true, data: 'Test DM sent!' },
            'check_and_notify': { success: true, data: 'No notification needed.' },
            'get_spend_events': [
                { id: 1, timestamp: Math.floor(Date.now()/1000) - 86400*2, txid: 'abc123def456abc123def456abc123def456abc123def456abc123def456abcd', spend_type: 'owner_checkin', confidence: 0.95, method: 'witness_analysis', policy_id: null, outpoint: null },
                { id: 2, timestamp: Math.floor(Date.now()/1000) - 86400*15, txid: 'f1e2d3c4b5a6f1e2d3c4b5a6f1e2d3c4b5a6f1e2d3c4b5a6f1e2d3c4b5a6', spend_type: 'heir_claim', confidence: 0.90, method: 'witness_analysis', policy_id: null, outpoint: null },
                { id: 3, timestamp: Math.floor(Date.now()/1000) - 86400*30, txid: '789012ghi789012ghi789012ghi789012ghi789012ghi789012ghi789012ghij', spend_type: 'owner_checkin', confidence: 0.99, method: 'timelock_timing', policy_id: null, outpoint: null },
                { id: 4, timestamp: Math.floor(Date.now()/1000) - 86400*60, txid: 'deadbeef1234deadbeef1234deadbeef1234deadbeef1234deadbeef12345678', spend_type: 'unknown', confidence: 0.30, method: 'indeterminate', policy_id: null, outpoint: null },
                { id: 5, timestamp: Math.floor(Date.now()/1000) - 86400*90, txid: 'cafebabe5678cafebabe5678cafebabe5678cafebabe5678cafebabe56781234', spend_type: 'owner_checkin', confidence: 0.70, method: 'witness_analysis', policy_id: null, outpoint: null },
            ],
            'check_heir_claims': true,
            'detect_spend_type': { id: 0, timestamp: Math.floor(Date.now()/1000), txid: 'test', spend_type: 'owner_checkin', confidence: 0.95, method: 'witness_analysis', policy_id: null, outpoint: null },
        };
        return mocks[cmd] ?? { success: true }
    }
    : window.__TAURI__.core.invoke;

// ============================================================================
// State
// ============================================================================
let currentTab = 'status';
let isUnlocked = false;
let policyStatus = null;
let heirs = [];
let wizardStep = 1;
let wizardHeirs = [];
let spendEvents = [];
let hasHeirClaims = false;

// ============================================================================
// Initialization
// ============================================================================
document.addEventListener('DOMContentLoaded', async () => {
    console.log('NoString initializing...');
    
    // Check if wallet exists (seed OR watch-only xpub)
    const hasWallet = await invoke('has_seed');
    
    if (hasWallet) {
        showLockScreen();
    } else {
        showSetupScreen();
    }
});

// ============================================================================
// Tab Navigation
// ============================================================================
function setupTabs() {
    document.querySelectorAll('[data-tab]').forEach(btn => {
        btn.addEventListener('click', () => {
            const tab = btn.dataset.tab;
            switchTab(tab);
        });
    });
}

function switchTab(tab) {
    currentTab = tab;
    
    // Update tab buttons
    document.querySelectorAll('[data-tab]').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tab);
    });
    
    // Update content
    document.querySelectorAll('.tab-content').forEach(section => {
        section.classList.toggle('active', section.id === `${tab}-tab`);
    });
    
    // Load tab-specific data
    if (tab === 'heirs') {
        loadHeirs();
    }
}

// ============================================================================
// Setup Screen
// ============================================================================
function showSetupScreen() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <div class="setup-screen">
            <h2>Welcome to NoString</h2>
            <p>Sovereign Bitcoin inheritance. No trusted third parties.</p>
            
            <div class="setup-options">
                <div class="setup-card recommended" id="opt-watch-only">
                    <div class="setup-card-badge">Recommended</div>
                    <h3>👁️ Watch-Only Wallet</h3>
                    <p>Import your xpub. Your keys stay on your hardware wallet. NoString just coordinates — you sign check-ins externally.</p>
                </div>
                
                <div class="setup-card" id="opt-import-seed">
                    <h3>📥 Import Seed (Advanced)</h3>
                    <p>Import a recovery phrase. NoString holds your keys. Less secure — only if you know what you're doing.</p>
                </div>
                
                <div class="setup-card" id="opt-create-seed">
                    <h3>✨ Generate New Seed</h3>
                    <p>Create a new wallet for testing or if you don't have one yet.</p>
                </div>
                
                <div class="setup-card" id="opt-heir-recovery" style="border-color: var(--warning, #f59e0b);">
                    <h3>🕊️ Recover a Loved One's Identity</h3>
                    <p>I'm an heir. I have Shamir shares and need to recover a Nostr identity (nsec).</p>
                </div>
            </div>
            
            <!-- Watch-Only Form -->
            <div id="watch-only-form" class="hidden">
                <h3>Import Watch-Only Wallet</h3>
                <p class="text-muted">Export your xpub from your wallet app (Electrum: Wallet → Information, or your hardware wallet companion app).</p>
                <div class="form-row">
                    <label>Your Extended Public Key (xpub)</label>
                    <textarea id="watch-xpub" placeholder="xpub6ABC..."></textarea>
                </div>
                <div class="password-setup">
                    <label>App Password (encrypts local data):</label>
                    <input type="password" id="watch-password" placeholder="Minimum 8 characters">
                    <label>Confirm Password:</label>
                    <input type="password" id="watch-password-confirm" placeholder="Confirm password">
                    <div class="form-actions">
                        <button type="button" id="btn-confirm-watch" class="btn-primary">Continue →</button>
                        <button type="button" id="btn-back-watch" class="btn-secondary">Back</button>
                    </div>
                </div>
            </div>
            
            <!-- Seed Display (after generation) -->
            <div id="seed-display" class="hidden">
                <h3>Your Recovery Phrase</h3>
                <p class="warning">⚠️ Write this down and store it safely. Never share it with anyone.</p>
                <div id="mnemonic-words" class="mnemonic-grid"></div>
                <div class="password-setup">
                    <label>Encryption Password:</label>
                    <input type="password" id="password-input" placeholder="Minimum 8 characters">
                    <label>Confirm Password:</label>
                    <input type="password" id="password-confirm" placeholder="Confirm password">
                    <button type="button" id="btn-confirm-seed" class="btn-primary">Confirm & Encrypt</button>
                </div>
            </div>
            
            <!-- Import Seed Form -->
            <div id="import-form" class="hidden">
                <h3>Import Recovery Phrase</h3>
                <textarea id="import-mnemonic" placeholder="Enter your 12 or 24 word recovery phrase, separated by spaces"></textarea>
                <div class="password-setup">
                    <label>Encryption Password:</label>
                    <input type="password" id="import-password" placeholder="Minimum 8 characters">
                    <label>Confirm Password:</label>
                    <input type="password" id="import-password-confirm" placeholder="Confirm password">
                    <div class="form-actions">
                        <button type="button" id="btn-confirm-import" class="btn-primary">Import & Encrypt</button>
                        <button type="button" id="btn-back-import" class="btn-secondary">Back</button>
                    </div>
                </div>
            </div>
        </div>
    `;
    
    // Setup card clicks
    document.getElementById('opt-watch-only').addEventListener('click', showWatchOnlyForm);
    document.getElementById('opt-import-seed').addEventListener('click', showImportForm);
    document.getElementById('opt-create-seed').addEventListener('click', createNewSeed);
    document.getElementById('opt-heir-recovery').addEventListener('click', showHeirRecoveryScreen);
}

function showWatchOnlyForm() {
    document.getElementById('watch-only-form').classList.remove('hidden');
    document.querySelector('.setup-options').classList.add('hidden');
    
    document.getElementById('btn-confirm-watch').addEventListener('click', confirmWatchOnly);
    document.getElementById('btn-back-watch').addEventListener('click', () => {
        document.getElementById('watch-only-form').classList.add('hidden');
        document.querySelector('.setup-options').classList.remove('hidden');
    });
}

async function confirmWatchOnly() {
    const xpub = document.getElementById('watch-xpub').value.trim();
    const password = document.getElementById('watch-password').value;
    const confirm = document.getElementById('watch-password-confirm').value;
    
    if (!xpub) {
        showError('Please enter your xpub');
        return;
    }
    
    if (!/^(xpub|ypub|zpub|tpub|\[)/i.test(xpub)) {
        showError('Please enter a valid xpub (starts with xpub, ypub, zpub, or tpub)');
        return;
    }
    
    if (!password) {
        showError('Please enter a password');
        return;
    }
    
    if (password !== confirm) {
        showError('Passwords do not match');
        return;
    }
    
    if (password.length < 8) {
        showError('Password must be at least 8 characters');
        return;
    }
    
    try {
        const result = await invoke('import_watch_only', { xpub, password });
        
        if (result.success) {
            isUnlocked = true;
            showSetupWizard();
        } else {
            showError('Error: ' + (result.error || 'Failed to import'));
        }
    } catch (err) {
        console.error('Failed to import watch-only:', err);
        showError('Failed to import watch-only wallet');
    }
}

async function createNewSeed() {
    try {
        const result = await invoke('create_seed', { wordCount: 24 });
        
        if (result.success) {
            const words = result.data.split(' ');
            const wordsContainer = document.getElementById('mnemonic-words');
            wordsContainer.innerHTML = words.map((word, i) => 
                `<span class="word"><span class="num">${i + 1}.</span>${word}</span>`
            ).join('');
            
            document.getElementById('seed-display').classList.remove('hidden');
            document.querySelector('.setup-options').classList.add('hidden');
            
            // Store mnemonic temporarily
            document.getElementById('seed-display').dataset.mnemonic = result.data;
            
            document.getElementById('btn-confirm-seed').addEventListener('click', confirmNewSeed);
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to create seed:', err);
        showError('Failed to create seed');
    }
}

async function confirmNewSeed() {
    const mnemonic = document.getElementById('seed-display').dataset.mnemonic;
    const password = document.getElementById('password-input').value;
    const confirm = document.getElementById('password-confirm').value;
    
    if (password !== confirm) {
        showError('Passwords do not match');
        return;
    }
    
    if (password.length < 8) {
        showError('Password must be at least 8 characters');
        return;
    }
    
    try {
        const result = await invoke('import_seed', { mnemonic, password });
        
        if (result.success) {
            isUnlocked = true;
            showSetupWizard();  // First-time setup → wizard
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to import seed:', err);
        showError('Failed to encrypt seed');
    }
}

function showImportForm() {
    document.getElementById('import-form').classList.remove('hidden');
    document.querySelector('.setup-options').classList.add('hidden');
    
    document.getElementById('btn-confirm-import').addEventListener('click', importExistingSeed);
    document.getElementById('btn-back-import').addEventListener('click', () => {
        document.getElementById('import-form').classList.add('hidden');
        document.querySelector('.setup-options').classList.remove('hidden');
    });
}

async function importExistingSeed() {
    const rawMnemonic = document.getElementById('import-mnemonic').value;
    const password = document.getElementById('import-password').value;
    const confirm = document.getElementById('import-password-confirm').value;
    
    // Validate mnemonic first
    const validation = validateMnemonic(rawMnemonic);
    if (!validation.valid) {
        showError(validation.error);
        return;
    }
    const mnemonic = validation.mnemonic;
    
    if (!password) {
        showError('Please enter a password');
        return;
    }
    
    if (password !== confirm) {
        showError('Passwords do not match');
        return;
    }
    
    if (password.length < 8) {
        showError('Password must be at least 8 characters');
        return;
    }
    
    try {
        const result = await invoke('import_seed', { mnemonic, password });
        
        if (result.success) {
            isUnlocked = true;
            showSetupWizard();  // First-time setup → wizard
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to import seed:', err);
        showError('Failed to import seed');
    }
}

// ============================================================================
// Lock Screen
// ============================================================================
function showLockScreen() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <div class="lock-screen">
            <h2>🔒 Wallet Locked</h2>
            <p>Enter your password to unlock NoString</p>
            <div class="password-setup">
                <input type="password" id="unlock-password" placeholder="Password" autofocus>
                <button type="button" id="btn-unlock" class="btn-primary">Unlock</button>
            </div>
        </div>
    `;
    
    document.getElementById('btn-unlock').addEventListener('click', unlockWallet);
    document.getElementById('unlock-password').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') unlockWallet();
    });
}

async function unlockWallet() {
    const password = document.getElementById('unlock-password').value;
    
    try {
        const result = await invoke('unlock_seed', { password });
        
        if (result.success) {
            isUnlocked = true;
            showMainApp();
        } else {
            showError('Incorrect password');
        }
    } catch (err) {
        console.error('Failed to unlock:', err);
        showError('Failed to unlock wallet');
    }
}

// ============================================================================
// Setup Wizard (first-time setup after seed creation)
// ============================================================================
function showSetupWizard() {
    wizardStep = 1;
    wizardHeirs = [];
    renderWizardStep();
}

function renderWizardStep() {
    const content = document.getElementById('content');
    document.querySelector('#tabs').innerHTML = '';
    
    if (wizardStep === 1) {
        content.innerHTML = `
            <div class="wizard">
                <div class="wizard-progress">
                    <span class="step active">1</span>
                    <span class="step-line"></span>
                    <span class="step">2</span>
                    <span class="step-line"></span>
                    <span class="step">3</span>
                    <span class="step-line"></span>
                    <span class="step">4</span>
                </div>
                
                <h2>👥 Add Your First Heir</h2>
                <p class="text-muted">Who should inherit your Bitcoin if something happens to you?</p>
                
                <div class="wizard-form">
                    <div class="form-row">
                        <label>Label (e.g., "Spouse", "Child")</label>
                        <input type="text" id="wizard-heir-label" placeholder="Spouse">
                    </div>
                    
                    <div class="form-row">
                        <label>Their Extended Public Key (xpub)</label>
                        <textarea id="wizard-heir-address" placeholder="xpub6ABC..."></textarea>
                        <p class="hint">Your heir exports this from their wallet (Electrum: Wallet → Information, or hardware wallet companion app). An xpub lets NoString create the inheritance script.</p>
                    </div>
                    
                    <div class="form-row">
                        <label>Time Before They Can Claim</label>
                        <select id="wizard-heir-timelock">
                            <option value="6">6 months of inactivity</option>
                            <option value="12">12 months of inactivity</option>
                            <option value="18">18 months of inactivity</option>
                            <option value="24">24 months of inactivity</option>
                        </select>
                        <p class="hint">They can only claim if you stop checking in for this long</p>
                    </div>
                    
                    <div class="wizard-actions">
                        <button type="button" id="btn-wizard-skip" class="btn-secondary">Skip for Now</button>
                        <button type="button" id="btn-wizard-next" class="btn-primary">Add Heir & Continue →</button>
                    </div>
                </div>
            </div>
        `;
        
        document.getElementById('btn-wizard-next').addEventListener('click', wizardAddHeir);
        document.getElementById('btn-wizard-skip').addEventListener('click', () => {
            wizardStep = 4; // Skip heirs + nsec → go to completion
            renderWizardStep();
        });
        
    } else if (wizardStep === 2) {
        const heirList = wizardHeirs.map(h => `
            <div class="heir-preview">
                <span class="heir-label">${escapeHtml(h.label)}</span>
                <span class="heir-timelock">can claim after ${h.timelock} months</span>
            </div>
        `).join('');
        
        content.innerHTML = `
            <div class="wizard">
                <div class="wizard-progress">
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step active">2</span>
                    <span class="step-line"></span>
                    <span class="step">3</span>
                    <span class="step-line"></span>
                    <span class="step">4</span>
                </div>
                
                <h2>📋 Review Your Policy</h2>
                <p class="text-muted">Here's how your inheritance will work:</p>
                
                <div class="policy-preview">
                    <div class="policy-rule">
                        <strong>You</strong> can spend anytime (with your password)
                    </div>
                    ${heirList}
                </div>
                
                <div class="wizard-actions">
                    <button type="button" id="btn-wizard-add-more" class="btn-secondary">+ Add Another Heir</button>
                    <button type="button" id="btn-wizard-next" class="btn-primary">Looks Good →</button>
                </div>
            </div>
        `;
        
        document.getElementById('btn-wizard-next').addEventListener('click', () => {
            wizardStep = 3; // nsec inheritance (optional)
            renderWizardStep();
        });
        document.getElementById('btn-wizard-add-more').addEventListener('click', () => {
            wizardStep = 1;
            renderWizardStep();
        });
        
    } else if (wizardStep === 3) {
        // Optional nsec inheritance step
        content.innerHTML = `
            <div class="wizard">
                <div class="wizard-progress">
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step active">3</span>
                    <span class="step-line"></span>
                    <span class="step">4</span>
                </div>
                
                <h2>🔑 Nostr Identity Inheritance (Optional)</h2>
                <p class="text-muted">Want your heirs to inherit your Nostr identity too? Enter your nsec to split it across your heirs using Shamir secret sharing.</p>
                
                <div class="form-row" style="margin-top: 1.5rem;">
                    <label>Your Nostr Secret Key (nsec or hex)</label>
                    <input type="password" id="wizard-nsec-input" placeholder="nsec1... or hex secret key">
                    <p class="hint">Find this in your Nostr client's settings. Your nsec is destroyed from memory immediately after splitting.</p>
                </div>
                
                <div class="wizard-actions">
                    <button type="button" id="btn-wizard-skip-nsec" class="btn-secondary">Skip</button>
                    <button type="button" id="btn-wizard-split-nsec" class="btn-primary">Split nsec →</button>
                </div>
            </div>
        `;
        
        document.getElementById('btn-wizard-skip-nsec').addEventListener('click', () => {
            wizardStep = 4;
            renderWizardStep();
        });
        document.getElementById('btn-wizard-split-nsec').addEventListener('click', async () => {
            const nsecInput = document.getElementById('wizard-nsec-input').value.trim();
            if (!nsecInput) {
                showError('Please enter your nsec or click Skip.');
                return;
            }
            try {
                const result = await invoke('split_nsec', { nsecInput });
                if (result.success) {
                    // Show the shares briefly, then continue
                    showNsecSplitResult(result.data);
                } else {
                    showError(result.error || 'Failed to split nsec.');
                }
            } catch (err) {
                showError('Failed: ' + err.message);
            }
        });
        
    } else if (wizardStep === 4) {
        content.innerHTML = `
            <div class="wizard">
                <div class="wizard-progress">
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step active">4</span>
                </div>
                
                <h2>🎉 You're All Set!</h2>
                <p class="text-muted">${wizardHeirs.length > 0 ? 'Your inheritance policy is ready.' : 'You can add heirs later in the Heirs tab.'}</p>
                
                <div class="wizard-complete">
                    <div id="service-key-section" class="service-key-setup">
                        <h3>🔔 Check-in Reminders</h3>
                        <p class="text-muted">Setting up notification key...</p>
                    </div>
                    
                    <div class="next-steps">
                        <h3>What's Next?</h3>
                        <ul>
                            <li><strong>Check in periodically</strong> — proves you're still in control</li>
                            <li><strong>Add more heirs</strong> — create a cascade (spouse → children → executor)</li>
                            <li><strong>Backup your seed</strong> — use Shamir splits for extra security</li>
                        </ul>
                    </div>
                    
                    <button type="button" id="btn-wizard-finish" class="btn-primary btn-large">Go to Dashboard →</button>
                </div>
            </div>
        `;
        
        // Auto-generate service key for notifications
        generateServiceKeyOnSetup();
        
        document.getElementById('btn-wizard-finish').addEventListener('click', () => {
            showMainApp();
        });
    }
}

async function wizardAddHeir() {
    const label = document.getElementById('wizard-heir-label').value.trim();
    const address = document.getElementById('wizard-heir-address').value.trim();
    const timelock = document.getElementById('wizard-heir-timelock').value;
    
    if (!label) {
        showError('Please enter a label for this heir');
        return;
    }
    
    if (!address) {
        showError('Please enter their xpub');
        return;
    }
    
    // Validate xpub format (xpub, ypub, zpub for mainnet; tpub for testnet; or descriptor with [fingerprint])
    if (!/^(xpub|ypub|zpub|tpub|\[)/i.test(address)) {
        showError('Please enter a valid xpub (starts with xpub, ypub, zpub, or tpub). Your heir can export this from their wallet app.');
        return;
    }
    
    try {
        const result = await invoke('add_heir', { 
            label, 
            xpubOrDescriptor: address,
            timelockMonths: parseInt(timelock)
        });
        
        if (result.success) {
            wizardHeirs.push({ label, address, timelock });
            wizardStep = 2;
            renderWizardStep();
        } else {
            showError('Failed to add heir: ' + (result.error || 'Unknown error'));
        }
    } catch (err) {
        console.error('Failed to add heir:', err);
        showError('Failed to add heir');
    }
}

// ============================================================================
// Main Application
// ============================================================================
function showMainApp() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <section id="status-tab" class="tab-content active">
            <div id="heir-claim-banner"></div>
            
            <div class="status-card">
                <div class="card-header">
                    <h2>📊 Inheritance Status</h2>
                    <button type="button" id="btn-refresh-status" class="btn-secondary btn-icon" title="Refresh">🔄</button>
                </div>
                <div id="status-display">Loading...</div>
            </div>
            
            <div class="checkin-card">
                <h3>✅ Check In</h3>
                <p class="text-muted">Prove you're alive and reset your inheritance timelock.</p>
                <button type="button" id="btn-checkin" class="btn-primary mt-2">Initiate Check-in</button>
            </div>
            
            <div id="activity-log"></div>
            
            <div class="how-it-works">
                <button type="button" id="btn-toggle-how" class="btn-link">ℹ️ How does this work?</button>
                <div id="how-content" class="how-content hidden">
                    <div class="how-step">
                        <strong>1. You set up heirs</strong>
                        <p>Each heir has their own wallet. You add their xpub to create an inheritance script with a timelock.</p>
                    </div>
                    <div class="how-step">
                        <strong>2. You check in periodically</strong>
                        <p>Signing a check-in transaction proves you're alive and resets the timelock countdown.</p>
                    </div>
                    <div class="how-step">
                        <strong>3. If you stop checking in</strong>
                        <p>After the timelock expires (e.g. 6 months), your heirs can claim using their own wallet. No seed sharing needed — it's all in the Bitcoin script.</p>
                    </div>
                    <div class="how-step">
                        <strong>4. Save your descriptor backup</strong>
                        <p>The descriptor is the recipe that combines your xpub + heir xpubs + timelock into the inheritance address. Download it from Settings — you need it to recover if you ever lose access to NoString.</p>
                    </div>
                    <div class="how-step">
                        <strong>5. Shamir backup is for YOUR seed</strong>
                        <p>Codex32/SLIP-39 splits are for backing up your own seed phrase. Your heirs don't need your seed — they use their own keys after the timelock expires.</p>
                    </div>
                </div>
            </div>
        </section>
        
        <section id="heirs-tab" class="tab-content">
            <div class="heir-card">
                <div class="card-header">
                    <h3>👥 Heirs</h3>
                    <button type="button" id="btn-add-heir" class="btn-primary">+ Add Heir</button>
                </div>
                <p class="text-muted mb-2">Manage who can claim your Bitcoin if you stop checking in.</p>
                <div id="heirs-list" class="heir-list">
                    <p class="text-muted">Loading...</p>
                </div>
            </div>
            
            <div id="add-heir-section" class="add-heir-form hidden">
                <h4>Add New Heir</h4>
                <div class="form-row">
                    <label>Label (e.g., "Spouse", "Child 1")</label>
                    <input type="text" id="heir-label" placeholder="Heir name or label">
                </div>
                <div class="form-row">
                    <label>Their Extended Public Key (xpub)</label>
                    <textarea id="heir-xpub" placeholder="xpub6ABC..."></textarea>
                    <p class="hint">Your heir exports this from their wallet (Electrum: Wallet → Information)</p>
                </div>
                <div style="display: flex; gap: 0.75rem; margin-top: 1rem;">
                    <button type="button" id="btn-save-heir" class="btn-primary">Save Heir</button>
                    <button type="button" id="btn-cancel-heir" class="btn-secondary">Cancel</button>
                </div>
            </div>
        </section>
        
        <section id="backup-tab" class="tab-content">
            <div class="backup-card">
                <h3>🔐 Shamir Backup</h3>
                <p class="text-muted mb-2">Split your seed into multiple shares. You'll need a threshold number of shares to recover.</p>
                
                <div class="backup-options">
                    <div class="backup-option" id="backup-codex32">
                        <h4>Codex32 (BIP-93)</h4>
                        <p>Human-readable shares with BCH checksum</p>
                    </div>
                    <div class="backup-option" id="backup-slip39">
                        <h4>SLIP-39</h4>
                        <p>Word-based shares with RS1024 checksum</p>
                    </div>
                </div>
            </div>
            
            <div id="share-generator" class="hidden">
                <div class="backup-card">
                    <h3 id="share-type-title">Generate Shares</h3>
                    <div class="form-row">
                        <label>Threshold (minimum shares needed to recover)</label>
                        <select id="share-threshold">
                            <option value="2">2</option>
                            <option value="3">3</option>
                            <option value="4">4</option>
                            <option value="5">5</option>
                        </select>
                    </div>
                    <div class="form-row">
                        <label>Total Shares</label>
                        <select id="share-total">
                            <option value="3">3</option>
                            <option value="5" selected>5</option>
                            <option value="7">7</option>
                        </select>
                    </div>
                    <div class="form-row">
                        <label>Identifier (4 characters)</label>
                        <input type="text" id="share-identifier" placeholder="TEST" maxlength="4">
                    </div>
                    <button type="button" id="btn-generate-shares" class="btn-primary mt-2">Generate Shares</button>
                    <button type="button" id="btn-cancel-shares" class="btn-secondary mt-2">Cancel</button>
                </div>
            </div>
            
            <div id="shares-display" class="hidden">
                <div class="backup-card">
                    <h3>🔑 Your Shares</h3>
                    <p class="warning">⚠️ Store each share separately. Keep them secure and private.</p>
                    <div id="shares-list" class="share-list"></div>
                    <button type="button" id="btn-done-shares" class="btn-secondary mt-2">Done</button>
                </div>
            </div>
        </section>
        
        <section id="settings-tab" class="tab-content">
            <div class="settings-group">
                <h3>📋 Descriptor Backup</h3>
                <p class="text-muted">Your descriptor is the key to recovery. If you lose access to NoString, import this into any miniscript wallet (Liana, Electrum) to recover your funds.</p>
                <div class="setting">
                    <button type="button" id="btn-download-backup" class="btn-primary">Download Descriptor Backup</button>
                </div>
            </div>
            
            <div class="settings-group">
                <h3>🔔 Notifications</h3>
                <p class="text-muted">NoString sends check-in reminders via Nostr DM and/or email.</p>
                <div class="setting" id="service-npub-container">
                    <p class="text-muted">Loading...</p>
                </div>
                <div class="setting" style="margin-top: 1rem;">
                    <label>Your Nostr npub (receives DM reminders):</label>
                    <input type="text" id="notify-owner-npub" placeholder="npub1...">
                    <p class="hint">Enter your personal npub. The service key above sends encrypted DMs to this address.</p>
                </div>
                <div class="setting" style="margin-top: 0.75rem;">
                    <label>Email (optional backup channel):</label>
                    <input type="email" id="notify-email" placeholder="you@example.com">
                </div>
                <div id="email-smtp-fields" class="hidden" style="margin-top: 0.5rem;">
                    <label>SMTP Host:</label>
                    <input type="text" id="notify-smtp-host" placeholder="smtp.example.com">
                    <label>SMTP User:</label>
                    <input type="text" id="notify-smtp-user" placeholder="user@example.com">
                    <label>SMTP Password:</label>
                    <input type="password" id="notify-smtp-password" placeholder="password">
                </div>
                <div style="display: flex; gap: 0.75rem; margin-top: 1rem;">
                    <button type="button" id="btn-save-notifications" class="btn-primary">Save Notifications</button>
                    <button type="button" id="btn-test-notification" class="btn-secondary">Send Test DM</button>
                </div>
            </div>
            
            <div class="settings-group">
                <h3>🔑 Nostr Identity Inheritance</h3>
                <div id="nsec-inheritance-status">
                    <p class="text-muted">Loading...</p>
                </div>
            </div>
            
            <div class="settings-group">
                <h3>Network</h3>
                <div class="setting">
                    <label>Electrum Server:</label>
                    <input type="text" id="electrum-url" placeholder="ssl://blockstream.info:700">
                    <button type="button" id="btn-save-electrum" class="btn-secondary">Save</button>
                </div>
            </div>
            
            <div class="settings-group">
                <h3>Security</h3>
                <div class="setting">
                    <label>Lock Wallet:</label>
                    <button type="button" id="btn-lock" class="btn-danger">Lock Now</button>
                </div>
            </div>
            
            <div class="settings-group">
                <h3>About</h3>
                <div class="setting">
                    <p class="text-muted">NoString v0.1.0 — Sovereign Bitcoin inheritance</p>
                </div>
            </div>
        </section>
    `;
    
    // Update tabs
    document.querySelector('#tabs').innerHTML = `
        <button type="button" data-tab="status" class="active">Status</button>
        <button type="button" data-tab="heirs">Heirs</button>
        <button type="button" data-tab="backup">Backup</button>
        <button type="button" data-tab="settings">Settings</button>
    `;
    
    setupTabs();
    
    // Setup event handlers
    document.getElementById('btn-refresh-status').addEventListener('click', refreshStatus);
    document.getElementById('btn-checkin').addEventListener('click', initiateCheckin);
    document.getElementById('btn-lock').addEventListener('click', lockWallet);
    document.getElementById('btn-save-electrum').addEventListener('click', saveElectrumUrl);
    document.getElementById('btn-download-backup').addEventListener('click', downloadDescriptorBackup);
    
    // How it works toggle
    document.getElementById('btn-toggle-how').addEventListener('click', () => {
        document.getElementById('how-content').classList.toggle('hidden');
    });
    
    // Heir management
    document.getElementById('btn-add-heir').addEventListener('click', showAddHeirForm);
    document.getElementById('btn-save-heir').addEventListener('click', saveHeir);
    document.getElementById('btn-cancel-heir').addEventListener('click', hideAddHeirForm);
    
    // Backup
    document.getElementById('backup-codex32').addEventListener('click', () => showShareGenerator('codex32'));
    document.getElementById('backup-slip39').addEventListener('click', () => showShareGenerator('slip39'));
    document.getElementById('btn-generate-shares').addEventListener('click', generateShares);
    document.getElementById('btn-cancel-shares').addEventListener('click', hideShareGenerator);
    document.getElementById('btn-done-shares').addEventListener('click', hideSharesDisplay);
    
    // Notification settings handlers
    document.getElementById('btn-save-notifications').addEventListener('click', saveNotificationSettings);
    document.getElementById('btn-test-notification').addEventListener('click', sendTestNotification);
    document.getElementById('notify-email').addEventListener('input', () => {
        const email = document.getElementById('notify-email').value.trim();
        document.getElementById('email-smtp-fields').classList.toggle('hidden', !email);
    });
    
    // Load initial data
    refreshStatus();
    loadElectrumUrl();
    loadServiceNpub();
    loadNotificationSettings();
    loadNsecInheritanceStatus();
}

// ============================================================================
// Status & Check-in
// ============================================================================
async function refreshStatus() {
    const display = document.getElementById('status-display');
    display.innerHTML = '<p class="text-muted">Loading...</p>';
    
    try {
        const result = await invoke('refresh_policy_status');
        
        if (result.success) {
            policyStatus = result.data;
            const urgencyClass = policyStatus.urgency === 'ok' ? 'status-ok' : 
                                 policyStatus.urgency === 'warning' ? 'status-warning' : 'status-critical';
            
            const statusIcon = policyStatus.urgency === 'ok' ? '✅' :
                               policyStatus.urgency === 'warning' ? '⚠️' : '🚨';
            
            const lastCheckin = policyStatus.last_checkin 
                ? new Date(policyStatus.last_checkin * 1000).toLocaleDateString()
                : 'Never';
            
            display.innerHTML = `
                <div class="status-item ${urgencyClass}">
                    <span class="label">Status</span>
                    <span class="value">${statusIcon} ${policyStatus.urgency.toUpperCase()}</span>
                </div>
                <div class="status-item">
                    <span class="label">Days Remaining</span>
                    <span class="value">${policyStatus.days_remaining.toFixed(1)}</span>
                </div>
                <div class="status-item">
                    <span class="label">Blocks Remaining</span>
                    <span class="value">${policyStatus.blocks_remaining.toLocaleString()}</span>
                </div>
                <div class="status-item">
                    <span class="label">Current Block</span>
                    <span class="value">${policyStatus.current_block.toLocaleString()}</span>
                </div>
                <div class="status-item">
                    <span class="label">Last Check-in</span>
                    <span class="value">${lastCheckin}</span>
                </div>
            `;
            
            // Check if notifications should fire
            invoke('check_and_notify').catch(err => {
                console.log('Notification check:', err);
            });

            // Load spend events (activity log + heir claim banner)
            loadSpendEvents();
        } else {
            display.innerHTML = `<p class="error">Error: ${result.error}</p>`;
        }
    } catch (err) {
        console.error('Failed to refresh status:', err);
        display.innerHTML = `<p class="error">Failed to load status</p>`;
    }
}

// Store current PSBT for copy/broadcast
let currentPsbtBase64 = null;

async function initiateCheckin() {
    try {
        const result = await invoke('initiate_checkin');
        
        if (result.success) {
            currentPsbtBase64 = result.data;
            showPsbtQrCode(result.data);
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to initiate check-in:', err);
        showError('Failed to initiate check-in');
    }
}

// ============================================================================
// Heir Management
// ============================================================================
async function loadHeirs() {
    const list = document.getElementById('heirs-list');
    
    try {
        heirs = await invoke('list_heirs');
        
        if (heirs.length === 0) {
            list.innerHTML = '<p class="text-muted">No heirs configured yet. Add an heir to set up your inheritance.</p>';
            return;
        }
        
        list.innerHTML = heirs.map(heir => `
            <div class="heir-item" data-fingerprint="${heir.fingerprint}">
                <div class="heir-info">
                    <span class="heir-label">${escapeHtml(heir.label)}</span>
                    <span class="heir-fingerprint">Fingerprint: ${heir.fingerprint}</span>
                </div>
                <div class="heir-actions">
                    <button type="button" class="btn-icon btn-remove-heir" title="Remove">🗑️</button>
                </div>
            </div>
        `).join('');
        
        // Add remove handlers
        document.querySelectorAll('.btn-remove-heir').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const item = e.target.closest('.heir-item');
                const fp = item.dataset.fingerprint;
                removeHeir(fp);
            });
        });
    } catch (err) {
        console.error('Failed to load heirs:', err);
        list.innerHTML = '<p class="error">Failed to load heirs</p>';
    }
}

function showAddHeirForm() {
    document.getElementById('add-heir-section').classList.remove('hidden');
    document.getElementById('heir-label').value = '';
    document.getElementById('heir-xpub').value = '';
}

function hideAddHeirForm() {
    document.getElementById('add-heir-section').classList.add('hidden');
}

async function saveHeir() {
    const label = document.getElementById('heir-label').value.trim();
    const xpub = document.getElementById('heir-xpub').value.trim();
    
    if (!label) {
        showError('Please enter a label for this heir');
        return;
    }
    
    if (!xpub) {
        showError('Please enter an xpub or descriptor');
        return;
    }
    
    try {
        const result = await invoke('add_heir', { label, xpubOrDescriptor: xpub });
        
        if (result.success) {
            hideAddHeirForm();
            loadHeirs();
            showSuccess(`Heir "${label}" added successfully`);
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to add heir:', err);
        showError('Failed to add heir');
    }
}

async function removeHeir(fingerprint) {
    if (!confirm('Are you sure you want to remove this heir?')) {
        return;
    }
    
    try {
        const result = await invoke('remove_heir', { fingerprint });
        
        if (result.success) {
            loadHeirs();
            showSuccess('Heir removed');
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to remove heir:', err);
        showError('Failed to remove heir');
    }
}

// ============================================================================
// Shamir Backup
// ============================================================================
let currentShareType = 'codex32';

function showShareGenerator(type) {
    currentShareType = type;
    document.getElementById('share-type-title').textContent = 
        type === 'codex32' ? 'Generate Codex32 Shares' : 'Generate SLIP-39 Shares';
    document.getElementById('share-generator').classList.remove('hidden');
    document.querySelector('.backup-options').classList.add('hidden');
}

function hideShareGenerator() {
    document.getElementById('share-generator').classList.add('hidden');
    document.querySelector('.backup-options').classList.remove('hidden');
}

function hideSharesDisplay() {
    document.getElementById('shares-display').classList.add('hidden');
    document.querySelector('.backup-options').classList.remove('hidden');
}

async function generateShares() {
    const threshold = parseInt(document.getElementById('share-threshold').value);
    const total = parseInt(document.getElementById('share-total').value);
    const identifier = document.getElementById('share-identifier').value.toUpperCase() || 'TEST';
    
    if (total < threshold) {
        showError('Total shares must be >= threshold');
        return;
    }
    
    try {
        const result = await invoke('generate_codex32_shares', { 
            threshold, 
            totalShares: total,
            identifier
        });
        
        if (result.success) {
            const shares = result.data;
            const list = document.getElementById('shares-list');
            
            list.innerHTML = shares.map((share, i) => `
                <div class="share-item">
                    <span class="share-index">${i + 1}</span>
                    <span class="share-value">${share}</span>
                    <button type="button" class="btn-icon btn-copy-share" data-share="${share}" title="Copy">📋</button>
                </div>
            `).join('');
            
            // Add copy handlers
            document.querySelectorAll('.btn-copy-share').forEach(btn => {
                btn.addEventListener('click', () => {
                    navigator.clipboard.writeText(btn.dataset.share);
                    showSuccess('Share copied to clipboard');
                });
            });
            
            document.getElementById('share-generator').classList.add('hidden');
            document.getElementById('shares-display').classList.remove('hidden');
        } else {
            showError('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to generate shares:', err);
        showError('Failed to generate shares');
    }
}

// ============================================================================
// QR Code Display & Scanner
// ============================================================================
function showPsbtQrCode(psbtBase64) {
    const modal = document.getElementById('qr-modal');
    const container = document.getElementById('qr-container');
    const instructions = document.getElementById('qr-instructions');
    
    container.innerHTML = '';
    
    try {
        // Use QRious (browser-native, no require)
        const canvas = document.createElement('canvas');
        new QRious({
            element: canvas,
            value: psbtBase64.toUpperCase(),
            size: 350,
            level: 'L'
        });
        container.appendChild(canvas);
    } catch (error) {
        console.error('QR generation error:', error);
        container.innerHTML = `<p style="color: var(--error);">PSBT too large for QR. Use the copy button below.</p>`;
    }
    
    instructions.textContent = 'Scan this QR code with Electrum (watch-only wallet) or your hardware wallet to sign the check-in transaction.';
    
    modal.classList.remove('hidden');
    
    document.getElementById('qr-modal-close').onclick = () => modal.classList.add('hidden');
    document.getElementById('btn-copy-psbt').onclick = () => copyPsbtToClipboard();
    document.getElementById('btn-scan-response').onclick = () => openQrScanner();
}

function copyPsbtToClipboard() {
    if (currentPsbtBase64) {
        navigator.clipboard.writeText(currentPsbtBase64).then(() => {
            showSuccess('PSBT copied! Paste into Electrum: Tools → Load transaction → From text');
        }).catch(err => {
            console.error('Copy failed:', err);
            prompt('Copy this PSBT:', currentPsbtBase64);
        });
    }
}

let scannerStream = null;
let scannerAnimationId = null;

function openQrScanner() {
    const qrModal = document.getElementById('qr-modal');
    const scannerModal = document.getElementById('scanner-modal');
    const video = document.getElementById('scanner-video');
    const status = document.getElementById('scanner-status');
    
    qrModal.classList.add('hidden');
    scannerModal.classList.remove('hidden');
    
    status.textContent = 'Initializing camera...';
    status.classList.remove('scanner-success');
    
    navigator.mediaDevices.getUserMedia({ 
        video: { facingMode: 'environment' } 
    }).then(stream => {
        scannerStream = stream;
        video.srcObject = stream;
        video.play();
        status.textContent = 'Scanning for QR code...';
        startScanning();
    }).catch(err => {
        console.error('Camera error:', err);
        status.textContent = 'Camera access denied. Please allow camera access.';
    });
    
    document.getElementById('scanner-modal-close').onclick = () => closeScanner();
}

function startScanning() {
    const video = document.getElementById('scanner-video');
    const canvas = document.getElementById('scanner-canvas');
    const ctx = canvas.getContext('2d');
    
    function scan() {
        if (video.readyState === video.HAVE_ENOUGH_DATA) {
            canvas.width = video.videoWidth;
            canvas.height = video.videoHeight;
            ctx.drawImage(video, 0, 0);
            
            const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
            const code = jsQR(imageData.data, imageData.width, imageData.height);
            
            if (code) {
                console.log('QR detected:', code.data.substring(0, 50) + '...');
                handleScannedPsbt(code.data);
                return;
            }
        }
        
        scannerAnimationId = requestAnimationFrame(scan);
    }
    
    scan();
}

function closeScanner() {
    const scannerModal = document.getElementById('scanner-modal');
    scannerModal.classList.add('hidden');
    
    if (scannerStream) {
        scannerStream.getTracks().forEach(track => track.stop());
        scannerStream = null;
    }
    
    if (scannerAnimationId) {
        cancelAnimationFrame(scannerAnimationId);
        scannerAnimationId = null;
    }
}

async function handleScannedPsbt(psbtData) {
    const status = document.getElementById('scanner-status');
    
    if (scannerAnimationId) {
        cancelAnimationFrame(scannerAnimationId);
        scannerAnimationId = null;
    }
    
    status.textContent = '✓ PSBT detected! Broadcasting...';
    status.classList.add('scanner-success');
    
    try {
        const result = await invoke('broadcast_signed_psbt', { signedPsbt: psbtData });
        
        if (result.success) {
            closeScanner();
            showSuccess('Check-in broadcast successfully! Txid: ' + result.data);
            refreshStatus();
            // Prompt to download updated descriptor backup
            setTimeout(() => promptDescriptorBackup(), 1500);
        } else {
            status.textContent = 'Error: ' + result.error;
            status.classList.remove('scanner-success');
        }
    } catch (err) {
        console.error('Broadcast error:', err);
        status.textContent = 'Failed to broadcast: ' + err.message;
        status.classList.remove('scanner-success');
    }
}

// ============================================================================
// Descriptor Backup
// ============================================================================
async function downloadDescriptorBackup() {
    try {
        const result = await invoke('get_descriptor_backup');
        
        if (!result.success) {
            showError(result.error || 'No descriptor configured yet. Add heirs first.');
            return;
        }
        
        const backup = result.data;
        const content = `# NoString Descriptor Backup
# Generated: ${new Date().toISOString()}
# 
# ⚠️  KEEP THIS FILE SAFE. You need it to recover your inheritance
#     policy if you lose access to NoString.
#
# To recover: Import this descriptor into any miniscript-compatible
# wallet (Liana, Electrum, etc.) to regain control of your funds.

## Descriptor
${backup.descriptor}

## Details
Network: ${backup.network}
Timelock: ${backup.timelock_blocks} blocks (~${Math.round(backup.timelock_blocks / 144)} days)
Inheritance Address: ${backup.address || 'N/A'}

## Heirs
${(backup.heirs || []).map(h => `- ${h.label}: ${h.xpub} (${h.timelock_months} months)`).join('\n')}

## Recovery Instructions
1. Install a miniscript wallet (e.g., Liana: wizardsardine.com/liana)
2. Import the descriptor above
3. Sign with your hardware wallet to move funds
`;

        // Add nsec inheritance section if configured
        try {
            const nsecStatus = await invoke('get_nsec_inheritance_status');
            if (nsecStatus.configured) {
                const lockedShares = await invoke('get_locked_shares');
                if (lockedShares && lockedShares.length > 0) {
                    content += `
## Nostr Identity Inheritance
Owner npub: ${nsecStatus.owner_npub}

### Locked Shares
These Codex32 shares, combined with the heir's pre-distributed share,
reconstruct the owner's nsec (Nostr secret key).

${lockedShares.map((s, i) => `Share ${i + 1}: ${s}`).join('\n')}

### Heir Recovery Instructions
1. Download NoString from github.com/RenAndKiwi/nostring
2. On the setup screen, choose "Recover a Loved One's Identity"
3. Enter YOUR pre-distributed share (given to you by the owner)
4. Enter ALL locked shares listed above
5. Click "Recover Identity" — your loved one's nsec will be revealed
6. Import the nsec into any Nostr client (Damus, Primal, Amethyst, etc.)
`;
                }
            }
        } catch (err) {
            console.error('Failed to add nsec info to backup:', err);
        }
        
        // Trigger file download
        const blob = new Blob([content], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `nostring-descriptor-backup-${new Date().toISOString().split('T')[0]}.txt`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        
        showSuccess('Descriptor backup downloaded');
    } catch (err) {
        console.error('Failed to get descriptor backup:', err);
        showError('Failed to generate descriptor backup');
    }
}

function promptDescriptorBackup() {
    // Show a modal prompting the user to download after check-in
    const modal = document.createElement('div');
    modal.className = 'modal';
    modal.id = 'descriptor-prompt';
    modal.innerHTML = `
        <div class="modal-content">
            <div class="modal-header">
                <h3>📋 Save Your Descriptor Backup</h3>
            </div>
            <div class="modal-body">
                <p>Your check-in created a new inheritance address. <strong>Download your updated descriptor backup</strong> so you can always recover your funds.</p>
                <p class="text-muted" style="font-size: 0.9rem;">Without this backup, you'd need to manually reconstruct the descriptor from your xpub, heir xpubs, and timelock settings.</p>
                <div class="qr-actions" style="margin-top: 1.5rem;">
                    <button type="button" id="btn-download-descriptor" class="btn-primary">Download Backup</button>
                    <button type="button" id="btn-skip-descriptor" class="btn-secondary">Skip for Now</button>
                </div>
            </div>
        </div>
    `;
    document.body.appendChild(modal);
    
    document.getElementById('btn-download-descriptor').addEventListener('click', async () => {
        await downloadDescriptorBackup();
        modal.remove();
    });
    document.getElementById('btn-skip-descriptor').addEventListener('click', () => {
        modal.remove();
    });
}

// ============================================================================
// Service Key (Notification Identity)
// ============================================================================

async function generateServiceKeyOnSetup() {
    const section = document.getElementById('service-key-section');
    try {
        const result = await invoke('generate_service_key');
        if (result.success && result.data) {
            section.innerHTML = `
                <h3>🔔 Check-in Reminders</h3>
                <p>Follow this npub in your Nostr client to receive check-in reminders:</p>
                <div class="service-npub-display">
                    <code id="wizard-service-npub">${escapeHtml(result.data)}</code>
                    <button type="button" id="btn-copy-wizard-npub" class="btn-icon" title="Copy">📋</button>
                </div>
                <p class="text-muted" style="font-size: 0.85rem;">Use Damus, Primal, Amethyst, or any Nostr client. This is NoString's notification-only identity — not your personal Nostr key.</p>
            `;
            document.getElementById('btn-copy-wizard-npub').addEventListener('click', () => {
                navigator.clipboard.writeText(result.data);
                showSuccess('Service npub copied to clipboard');
            });
        } else {
            section.innerHTML = `
                <h3>🔔 Check-in Reminders</h3>
                <p class="text-muted">Could not generate notification key. You can set this up later in Settings.</p>
            `;
        }
    } catch (err) {
        console.error('Failed to generate service key:', err);
        section.innerHTML = `
            <h3>🔔 Check-in Reminders</h3>
            <p class="text-muted">Could not generate notification key. You can set this up later in Settings.</p>
        `;
    }
}

async function loadServiceNpub() {
    try {
        const npub = await invoke('get_service_npub');
        const container = document.getElementById('service-npub-container');
        if (npub && container) {
            container.innerHTML = `
                <div class="service-npub-display">
                    <code id="settings-service-npub">${escapeHtml(npub)}</code>
                    <button type="button" id="btn-copy-settings-npub" class="btn-icon" title="Copy">📋</button>
                </div>
                <p class="text-muted" style="font-size: 0.85rem;">Follow this npub in your Nostr client (Damus, Primal, etc.) to receive check-in reminders.</p>
            `;
            document.getElementById('btn-copy-settings-npub').addEventListener('click', () => {
                navigator.clipboard.writeText(npub);
                showSuccess('Service npub copied to clipboard');
            });
        } else if (container) {
            container.innerHTML = `
                <p class="text-muted">No service key generated yet.</p>
                <button type="button" id="btn-generate-service-key" class="btn-secondary">Generate Key</button>
            `;
            document.getElementById('btn-generate-service-key').addEventListener('click', async () => {
                const result = await invoke('generate_service_key');
                if (result.success) {
                    loadServiceNpub(); // Reload to show the npub
                }
            });
        }
    } catch (err) {
        console.error('Failed to load service npub:', err);
    }
}

// ============================================================================
// Heir Recovery Screen
// ============================================================================
function showHeirRecoveryScreen() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <div class="setup-screen">
            <h2>🕊️ Recover a Nostr Identity</h2>
            <p class="text-muted">If your loved one set up NoString with Nostr identity inheritance, you can recover their nsec by combining Shamir shares.</p>
            
            <div class="recovery-info" style="margin: 1.5rem 0; padding: 1rem; border-radius: 8px; background: var(--card-bg, #1a1a2e);">
                <h4>What you need:</h4>
                <ol style="margin: 0.5rem 0; padding-left: 1.5rem;">
                    <li><strong>Your pre-distributed share</strong> — given to you during setup (a Codex32 string starting with "ms1...")</li>
                    <li><strong>Locked shares</strong> — from the descriptor backup file (found in a safe deposit box, with a lawyer, etc.)</li>
                </ol>
                <p class="text-muted" style="font-size: 0.85rem; margin-top: 0.75rem;">The descriptor backup file has a section called "Locked Shares" with one or more Codex32 strings. Enter all shares below — yours plus the locked ones.</p>
            </div>
            
            <div id="share-inputs">
                <div class="form-row">
                    <label>Share 1 (your pre-distributed share)</label>
                    <input type="text" class="recovery-share" placeholder="ms12nsec...">
                </div>
                <div class="form-row">
                    <label>Share 2 (from descriptor backup)</label>
                    <input type="text" class="recovery-share" placeholder="ms12nsec...">
                </div>
            </div>
            
            <div style="display: flex; gap: 0.75rem; margin-top: 0.75rem;">
                <button type="button" id="btn-add-share-input" class="btn-secondary">+ Add Another Share</button>
            </div>
            
            <div style="display: flex; gap: 0.75rem; margin-top: 1.5rem;">
                <button type="button" id="btn-recover-nsec" class="btn-primary">Recover Identity</button>
                <button type="button" id="btn-back-recovery" class="btn-secondary">Back</button>
            </div>
            
            <div id="recovery-result" class="hidden" style="margin-top: 1.5rem;">
            </div>
        </div>
    `;
    
    document.getElementById('btn-add-share-input').addEventListener('click', () => {
        const container = document.getElementById('share-inputs');
        const count = container.querySelectorAll('.recovery-share').length + 1;
        const row = document.createElement('div');
        row.className = 'form-row';
        row.innerHTML = '<label>Share ' + count + '</label><input type="text" class="recovery-share" placeholder="ms12nsec...">';
        container.appendChild(row);
    });
    
    document.getElementById('btn-recover-nsec').addEventListener('click', attemptNsecRecovery);
    document.getElementById('btn-back-recovery').addEventListener('click', showSetupScreen);
}

async function attemptNsecRecovery() {
    const inputs = document.querySelectorAll('.recovery-share');
    const shares = [];
    inputs.forEach(input => {
        const val = input.value.trim();
        if (val) shares.push(val);
    });
    
    if (shares.length < 2) {
        showError('Need at least 2 shares to recover.');
        return;
    }
    
    try {
        const result = await invoke('recover_nsec', { shares });
        const resultDiv = document.getElementById('recovery-result');
        
        if (result.success) {
            resultDiv.classList.remove('hidden');
            resultDiv.innerHTML = `
                <div style="padding: 1.5rem; border-radius: 8px; background: var(--card-bg, #1a1a2e); border: 1px solid var(--success, #10b981);">
                    <h3>✅ Identity Recovered</h3>
                    <p>This is the Nostr identity of your loved one.</p>
                    
                    <div class="form-row" style="margin-top: 1rem;">
                        <label>Public Key (npub) — verify this matches</label>
                        <code style="word-break: break-all; display: block; padding: 0.5rem;">${escapeHtml(result.data.npub)}</code>
                    </div>
                    
                    <div class="form-row" style="margin-top: 1rem;">
                        <label>Secret Key (nsec) — import this into your Nostr client</label>
                        <div style="position: relative;">
                            <code id="recovered-nsec" style="word-break: break-all; display: block; padding: 0.5rem; filter: blur(5px); cursor: pointer;" title="Click to reveal">${escapeHtml(result.data.nsec)}</code>
                            <button type="button" id="btn-reveal-nsec" class="btn-secondary" style="position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%);">👁️ Reveal</button>
                        </div>
                    </div>
                    
                    <div style="display: flex; gap: 0.75rem; margin-top: 1rem;">
                        <button type="button" id="btn-copy-nsec" class="btn-primary">📋 Copy nsec</button>
                    </div>
                    
                    <p class="warning" style="margin-top: 1rem;">⚠️ This nsec gives full control of the Nostr identity. Store it as securely as you would a password. Anyone with this key can post as this identity.</p>
                </div>
            `;
            
            document.getElementById('btn-reveal-nsec').addEventListener('click', () => {
                document.getElementById('recovered-nsec').style.filter = 'none';
                document.getElementById('btn-reveal-nsec').remove();
            });
            
            document.getElementById('btn-copy-nsec').addEventListener('click', () => {
                navigator.clipboard.writeText(result.data.nsec);
                showSuccess('nsec copied to clipboard');
            });
        } else {
            resultDiv.classList.remove('hidden');
            resultDiv.innerHTML = `
                <div style="padding: 1rem; border-radius: 8px; background: var(--card-bg, #1a1a2e); border: 1px solid var(--error, #ef4444);">
                    <p class="error">${escapeHtml(result.error)}</p>
                    <p class="text-muted" style="font-size: 0.85rem;">Make sure you have enough shares. Check that all shares are from the same split (same identifier).</p>
                </div>
            `;
        }
    } catch (err) {
        console.error('Recovery failed:', err);
        showError('Recovery failed: ' + err.message);
    }
}

// ============================================================================
// nsec Inheritance Status (Settings tab)
// ============================================================================
async function loadNsecInheritanceStatus() {
    try {
        const status = await invoke('get_nsec_inheritance_status');
        const container = document.getElementById('nsec-inheritance-status');
        if (!container) return;
        
        if (status.configured) {
            container.innerHTML = `
                <p>✅ Identity inheritance configured.</p>
                <p class="text-muted">npub: <code style="font-size: 0.8rem;">${escapeHtml(status.owner_npub || 'unknown')}</code></p>
                <p class="text-muted">${status.locked_share_count} locked shares in descriptor backup.</p>
                <div style="display: flex; gap: 0.75rem; margin-top: 0.75rem;">
                    <button type="button" id="btn-resplit-nsec-settings" class="btn-secondary">🔄 Re-split</button>
                    <button type="button" id="btn-revoke-nsec-settings" class="btn-danger">🗑️ Revoke</button>
                </div>
            `;
            document.getElementById('btn-resplit-nsec-settings').addEventListener('click', showNsecSplitUI);
            document.getElementById('btn-revoke-nsec-settings').addEventListener('click', revokeNsecInheritance);
        } else {
            container.innerHTML = `
                <p class="text-muted">Not configured. Optionally pass down your Nostr identity to heirs.</p>
                <button type="button" id="btn-setup-nsec" class="btn-secondary" style="margin-top: 0.5rem;">Set Up Identity Inheritance</button>
            `;
            document.getElementById('btn-setup-nsec').addEventListener('click', showNsecSplitUI);
        }
    } catch (err) {
        console.error('Failed to load nsec status:', err);
    }
}

// ============================================================================
// nsec Inheritance — Revocation
// ============================================================================
async function revokeNsecInheritance() {
    if (!confirm(
        '⚠️ REVOKE nsec inheritance?\n\n' +
        '• All locked shares will be deleted from your wallet\n' +
        '• Old pre-distributed shares given to heirs become USELESS\n' +
        '• Your descriptor backup becomes stale — you should re-download it\n\n' +
        'This cannot be undone. Continue?'
    )) {
        return;
    }
    
    // Double-confirm
    if (!confirm('Are you absolutely sure? Heirs will NOT be able to recover your Nostr identity with their old shares.')) {
        return;
    }
    
    try {
        const result = await invoke('revoke_nsec_inheritance');
        if (result.success) {
            showSuccess('nsec inheritance revoked. Old shares are now useless.');
            loadNsecInheritanceStatus(); // Refresh the UI
            // Prompt descriptor backup re-download
            setTimeout(() => {
                if (confirm('Your descriptor backup is now stale (locked shares removed). Download an updated backup?')) {
                    downloadDescriptorBackup();
                }
            }, 500);
        } else {
            showError(result.error || 'Failed to revoke');
        }
    } catch (err) {
        console.error('Failed to revoke nsec inheritance:', err);
        showError('Failed to revoke: ' + err.message);
    }
}

// ============================================================================
// nsec Inheritance (Split) — Owner Side
// ============================================================================
async function showNsecSplitUI() {
    const content = document.getElementById('content');
    // Check current status first
    const status = await invoke('get_nsec_inheritance_status');
    
    if (status.configured) {
        content.innerHTML = `
            <div class="setup-screen">
                <h2>🔑 Nostr Identity Inheritance</h2>
                <div style="padding: 1rem; border-radius: 8px; background: var(--card-bg, #1a1a2e); border: 1px solid var(--success, #10b981);">
                    <p>✅ Identity inheritance is configured.</p>
                    <p class="text-muted">npub: <code>${escapeHtml(status.owner_npub || 'unknown')}</code></p>
                    <p class="text-muted">${status.locked_share_count} locked shares stored in descriptor backup.</p>
                </div>
                
                <div class="warning" style="margin-top: 1.5rem;">
                    <strong>⚠️ Re-splitting or revoking invalidates all existing shares.</strong><br>
                    Old pre-distributed shares given to heirs will no longer work. You must give heirs their new shares after re-splitting.
                </div>
                
                <div style="display: flex; gap: 0.75rem; margin-top: 1.5rem;">
                    <button type="button" id="btn-resplit-nsec" class="btn-secondary">🔄 Re-split nsec</button>
                    <button type="button" id="btn-revoke-nsec-full" class="btn-danger">🗑️ Revoke Inheritance</button>
                    <button type="button" id="btn-back-nsec" class="btn-secondary">← Back</button>
                </div>
            </div>
        `;
        document.getElementById('btn-resplit-nsec').addEventListener('click', () => {
            if (confirm('Re-splitting will invalidate ALL existing shares.\\nHeirs must receive new shares after this.\\n\\nYou will need your nsec again. Continue?')) {
                showNsecInputForm();
            }
        });
        document.getElementById('btn-revoke-nsec-full').addEventListener('click', async () => {
            await revokeNsecInheritance();
            showMainApp();
        });
        document.getElementById('btn-back-nsec').addEventListener('click', showMainApp);
        return;
    }
    
    showNsecInputForm();
}

function showNsecInputForm() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <div class="setup-screen">
            <h2>🔑 Nostr Identity Inheritance</h2>
            <p class="text-muted">Optionally pass down your Nostr identity to your heirs using Shamir secret sharing.</p>
            
            <div style="margin: 1.5rem 0; padding: 1rem; border-radius: 8px; background: var(--card-bg, #1a1a2e);">
                <h4>How it works:</h4>
                <ol style="margin: 0.5rem 0; padding-left: 1.5rem; line-height: 1.8;">
                    <li>You enter your nsec (Nostr secret key)</li>
                    <li>NoString splits it into Shamir shares using the (N+1)-of-(2N+1) formula</li>
                    <li>Each heir gets one share — <strong>not enough to reconstruct alone</strong></li>
                    <li>Remaining shares are locked in your descriptor backup</li>
                    <li>After Bitcoin inheritance triggers, heirs combine shares to recover your nsec</li>
                    <li><strong>Your nsec is destroyed from memory immediately after splitting</strong></li>
                </ol>
            </div>
            
            <div class="form-row">
                <label>Your Nostr Secret Key (nsec or hex)</label>
                <input type="password" id="nsec-input" placeholder="nsec1... or hex secret key">
                <p class="hint">Find this in your Nostr client's settings (Damus: Settings → Keys, Primal: Settings → Keys, etc.)</p>
            </div>
            
            <div class="warning" style="margin-top: 1rem;">
                ⚠️ Your nsec will be held in memory ONLY during the split, then immediately zeroed. It is never saved to disk.
            </div>
            
            <div style="display: flex; gap: 0.75rem; margin-top: 1.5rem;">
                <button type="button" id="btn-split-nsec" class="btn-primary">Split nsec</button>
                <button type="button" id="btn-skip-nsec" class="btn-secondary">Skip</button>
            </div>
        </div>
    `;
    
    document.getElementById('btn-split-nsec').addEventListener('click', performNsecSplit);
    document.getElementById('btn-skip-nsec').addEventListener('click', showMainApp);
}

async function performNsecSplit() {
    const nsecInput = document.getElementById('nsec-input').value.trim();
    
    if (!nsecInput) {
        showError('Please enter your nsec.');
        return;
    }
    
    try {
        const result = await invoke('split_nsec', { nsecInput });
        
        if (!result.success) {
            showError(result.error || 'Split failed.');
            return;
        }
        
        const data = result.data;
        showNsecSplitResult(data);
    } catch (err) {
        console.error('nsec split failed:', err);
        showError('Failed to split nsec: ' + err.message);
    }
}

function showNsecSplitResult(data) {
    const content = document.getElementById('content');
    
    const resplitWarning = data.was_resplit ? `
        <div class="warning" style="margin-bottom: 1.5rem; padding: 1rem; border-radius: 8px; border: 1px solid var(--warning, #f59e0b);">
            <strong>⚠️ This was a RE-SPLIT.</strong> Old shares are now INVALID.<br>
            ${data.previous_npub ? `Previous npub: <code style="font-size: 0.8rem;">${escapeHtml(data.previous_npub)}</code><br>` : ''}
            <strong>You MUST give your heirs their new shares below.</strong> The old pre-distributed shares will no longer reconstruct the nsec.
            Also download an updated descriptor backup — the old one has stale locked shares.
        </div>
    ` : '';
    
    const heirShares = data.pre_distributed.map((h, i) => `
        <div class="share-item" style="margin-bottom: 1rem; padding: 1rem; border-radius: 8px; background: var(--card-bg, #1a1a2e);">
            <h4>📨 Share for: ${escapeHtml(h.heir_label)}</h4>
            <p class="text-muted" style="font-size: 0.85rem;">Give this to ${escapeHtml(h.heir_label)}. Tell them to store it securely (paper, steel backup).</p>
            <code style="word-break: break-all; display: block; padding: 0.5rem; margin-top: 0.5rem;">${escapeHtml(h.share)}</code>
            <button type="button" class="btn-secondary btn-copy-heir-share" data-share="${escapeHtml(h.share)}" style="margin-top: 0.5rem;">📋 Copy</button>
        </div>
    `).join('');
    
    const lockedShares = data.locked_shares.map((s, i) => `
        <div style="margin-bottom: 0.5rem;">
            <code style="word-break: break-all; font-size: 0.85rem;">${escapeHtml(s)}</code>
        </div>
    `).join('');
    
    content.innerHTML = `
        <div class="setup-screen">
            <h2>✅ nsec ${data.was_resplit ? 'Re-split' : 'Split'} Complete</h2>
            
            ${resplitWarning}
            
            <div style="padding: 1rem; border-radius: 8px; background: var(--card-bg, #1a1a2e); border: 1px solid var(--success, #10b981); margin-bottom: 1.5rem;">
                <p><strong>Identity:</strong> <code>${escapeHtml(data.owner_npub)}</code></p>
                <p><strong>Scheme:</strong> ${data.threshold}-of-${data.total_shares} Shamir split</p>
                <p><strong>Pre-distributed:</strong> ${data.pre_distributed.length} shares (one per heir)</p>
                <p><strong>Locked:</strong> ${data.locked_shares.length} shares (in descriptor backup)</p>
                <p class="text-muted" style="font-size: 0.85rem; margin-top: 0.5rem;">Your nsec has been destroyed from memory. It cannot be recovered from NoString.</p>
            </div>
            
            <h3>📨 Heir Shares — Distribute These</h3>
            <p class="warning" style="margin-bottom: 1rem;">Write down or print each share and give it to the corresponding heir. Tell them: "This is one piece of a key to my Nostr identity. Keep it safe. You'll get the rest when the time comes."</p>
            ${heirShares}
            
            <h3 style="margin-top: 2rem;">🔒 Locked Shares — Included in Descriptor Backup</h3>
            <p class="text-muted" style="margin-bottom: 1rem;">These are automatically included when you download your descriptor backup. Your heirs will get them from the backup file (safe deposit box, lawyer, etc.).</p>
            ${lockedShares}
            
            <div style="display: flex; gap: 0.75rem; margin-top: 2rem;">
                <button type="button" id="btn-download-backup-after-split" class="btn-primary">📋 Download Descriptor Backup Now</button>
                <button type="button" id="btn-done-nsec-split" class="btn-secondary">Done</button>
            </div>
        </div>
    `;
    
    // Copy handlers
    document.querySelectorAll('.btn-copy-heir-share').forEach(btn => {
        btn.addEventListener('click', () => {
            navigator.clipboard.writeText(btn.dataset.share);
            showSuccess('Share copied to clipboard');
        });
    });
    
    document.getElementById('btn-download-backup-after-split').addEventListener('click', async () => {
        await downloadDescriptorBackup();
    });
    document.getElementById('btn-done-nsec-split').addEventListener('click', showMainApp);
}

// ============================================================================
// Notification Settings
// ============================================================================
async function loadNotificationSettings() {
    try {
        const settings = await invoke('get_notification_settings');
        if (settings.owner_npub) {
            document.getElementById('notify-owner-npub').value = settings.owner_npub;
        }
        if (settings.email_address) {
            document.getElementById('notify-email').value = settings.email_address;
            document.getElementById('email-smtp-fields').classList.remove('hidden');
        }
        if (settings.email_smtp_host) {
            document.getElementById('notify-smtp-host').value = settings.email_smtp_host;
        }
    } catch (err) {
        console.error('Failed to load notification settings:', err);
    }
}

async function saveNotificationSettings() {
    const ownerNpub = document.getElementById('notify-owner-npub').value.trim() || null;
    const emailAddress = document.getElementById('notify-email').value.trim() || null;
    const emailSmtpHost = document.getElementById('notify-smtp-host').value.trim() || null;
    const emailSmtpUser = document.getElementById('notify-smtp-user').value.trim() || null;
    const emailSmtpPassword = document.getElementById('notify-smtp-password').value.trim() || null;

    try {
        const result = await invoke('configure_notifications', {
            ownerNpub,
            emailAddress,
            emailSmtpHost,
            emailSmtpUser,
            emailSmtpPassword,
        });
        if (result.success) {
            showSuccess('Notification settings saved');
        } else {
            showError(result.error || 'Failed to save');
        }
    } catch (err) {
        console.error('Failed to save notification settings:', err);
        showError('Failed to save notification settings');
    }
}

async function sendTestNotification() {
    try {
        showSuccess('Sending test DM...');
        const result = await invoke('send_test_notification');
        if (result.success) {
            showSuccess(result.data);
        } else {
            showError(result.error || 'Failed to send test notification');
        }
    } catch (err) {
        console.error('Failed to send test notification:', err);
        showError('Failed to send test notification');
    }
}

// ============================================================================
// Settings
// ============================================================================
async function lockWallet() {
    await invoke('lock_wallet');
    isUnlocked = false;
    showLockScreen();
}

async function loadElectrumUrl() {
    const url = await invoke('get_electrum_url');
    document.getElementById('electrum-url').value = url;
}

async function saveElectrumUrl() {
    const url = document.getElementById('electrum-url').value;
    await invoke('set_electrum_url', { url });
    showSuccess('Electrum server saved');
}

// ============================================================================
// Spend Events — Activity Log & Heir Claim Banner
// ============================================================================

async function loadSpendEvents() {
    try {
        const [events, claims] = await Promise.all([
            invoke('get_spend_events'),
            invoke('check_heir_claims'),
        ]);
        spendEvents = Array.isArray(events) ? events : [];
        hasHeirClaims = !!claims;

        // Render banner
        const bannerEl = document.getElementById('heir-claim-banner');
        if (bannerEl) {
            bannerEl.innerHTML = renderHeirClaimBanner(spendEvents);
            setupBannerHandlers();
        }

        // Render activity log
        const logEl = document.getElementById('activity-log');
        if (logEl) {
            logEl.innerHTML = renderActivityLog(spendEvents);
            setupActivityLogHandlers();
        }

        // Update heir claims row in status display
        updateHeirClaimsStatusRow();
    } catch (err) {
        console.error('Failed to load spend events:', err);
    }
}

function renderHeirClaimBanner(events) {
    const heirClaims = events.filter(e => e.spend_type === 'heir_claim' && e.confidence >= 0.50);
    if (heirClaims.length === 0) return '';

    // Check if the latest heir claim has been dismissed
    const latestClaim = heirClaims.reduce((a, b) => (a.id > b.id ? a : b));
    const dismissedId = localStorage.getItem('nostring_heir_alert_dismissed');
    if (dismissedId && parseInt(dismissedId) >= latestClaim.id) return '';

    const confidencePct = Math.round(latestClaim.confidence * 100);
    const isLowConfidence = latestClaim.confidence < 0.85;

    const title = isLowConfidence
        ? `⚠️ POSSIBLE HEIR CLAIM (${confidencePct}% confidence — review manually)`
        : `⚠️ HEIR CLAIM DETECTED (${confidencePct}% confidence)`;

    return `
        <div class="heir-claim-banner" data-event-id="${latestClaim.id}">
            <div class="heir-claim-banner-header">
                <span class="heir-claim-banner-title">${title}</span>
                <button type="button" class="heir-claim-banner-dismiss" id="btn-banner-x" title="Dismiss">✕</button>
            </div>
            <p>An heir has claimed funds from your inheritance address. This may indicate an unauthorized spend.</p>
            <p>If this is unexpected, your funds may have been claimed. Contact your heirs or review the transaction on a block explorer.</p>
            <div class="heir-claim-banner-actions">
                <button type="button" class="btn-view-details" id="btn-banner-view">View Details ↓</button>
                <button type="button" class="btn-dismiss-alert" id="btn-banner-dismiss">Dismiss</button>
            </div>
        </div>
    `;
}

function setupBannerHandlers() {
    const banner = document.querySelector('.heir-claim-banner');
    if (!banner) return;
    const eventId = banner.dataset.eventId;

    const dismissFn = () => {
        localStorage.setItem('nostring_heir_alert_dismissed', eventId);
        banner.remove();
    };

    const viewFn = () => {
        const logEl = document.getElementById('activity-log');
        if (logEl) {
            const logContainer = logEl.querySelector('.activity-log');
            if (logContainer && !logContainer.classList.contains('expanded')) {
                logContainer.classList.add('expanded');
            }
            logEl.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }
    };

    const xBtn = document.getElementById('btn-banner-x');
    const dismissBtn = document.getElementById('btn-banner-dismiss');
    const viewBtn = document.getElementById('btn-banner-view');

    if (xBtn) xBtn.addEventListener('click', dismissFn);
    if (dismissBtn) dismissBtn.addEventListener('click', dismissFn);
    if (viewBtn) viewBtn.addEventListener('click', viewFn);
}

function renderActivityLog(events) {
    const limited = events.slice(0, 20);
    const heirClaimsExist = events.some(e => e.spend_type === 'heir_claim');
    const expandedClass = heirClaimsExist ? ' expanded' : '';

    const body = limited.length === 0
        ? '<div class="activity-log-empty">No spend events recorded yet. Events appear when UTXOs are spent.</div>'
        : limited.map(e => renderSpendEventRow(e)).join('');

    return `
        <div class="activity-log${expandedClass}">
            <div class="activity-log-header" id="activity-log-toggle">
                <h3>📋 Activity Log</h3>
                <div class="activity-log-header-actions">
                    <span class="activity-log-toggle">▼</span>
                </div>
            </div>
            <div class="activity-log-body">
                ${body}
            </div>
        </div>
    `;
}

function setupActivityLogHandlers() {
    const toggleBtn = document.getElementById('activity-log-toggle');
    if (toggleBtn) {
        toggleBtn.addEventListener('click', () => {
            const logContainer = toggleBtn.closest('.activity-log');
            if (logContainer) {
                logContainer.classList.toggle('expanded');
            }
        });
    }
}

function renderSpendEventRow(event) {
    const icon = event.spend_type === 'owner_checkin' ? '✅'
        : event.spend_type === 'heir_claim' ? '⚠️'
        : '❓';

    const typeLabel = event.spend_type === 'owner_checkin' ? 'Owner Check-in'
        : event.spend_type === 'heir_claim' ? 'Heir Claim'
        : 'Unknown';

    const typeClass = event.spend_type === 'heir_claim' ? ' heir-claim' : '';

    const date = new Date(event.timestamp * 1000);
    const dateStr = date.toLocaleDateString() + ' ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

    const methodLabel = event.method === 'witness_analysis' ? 'Witness analysis'
        : event.method === 'timelock_timing' ? 'Timelock timing'
        : event.method === 'indeterminate' ? 'Indeterminate'
        : event.method || 'Unknown';

    // Detect network from any available context — default to mainnet
    const network = 'mainnet';

    return `
        <div class="spend-event-row">
            <span class="spend-type-icon">${icon}</span>
            <div class="spend-event-details">
                <div class="spend-event-top">
                    <span class="spend-event-type${typeClass}">${typeLabel}</span>
                    <span class="spend-event-date">${dateStr}</span>
                </div>
                <div class="spend-event-meta">
                    <span class="spend-event-txid">${formatTxidLink(event.txid, network)}</span>
                    ${renderConfidenceIndicator(event.confidence)}
                    <span class="spend-event-method">${escapeHtml(methodLabel)}</span>
                </div>
            </div>
        </div>
    `;
}

function renderConfidenceIndicator(confidence) {
    const pct = Math.round(confidence * 100);
    let filledCount, color, label;

    if (confidence >= 0.90) {
        filledCount = 5; color = '#10b981'; label = 'Very High';
    } else if (confidence >= 0.70) {
        filledCount = 4; color = '#84cc16'; label = 'High';
    } else if (confidence >= 0.50) {
        filledCount = 3; color = '#eab308'; label = 'Medium';
    } else if (confidence >= 0.30) {
        filledCount = 2; color = '#f97316'; label = 'Low';
    } else {
        filledCount = 1; color = '#ef4444'; label = 'Very Low';
    }

    let dots = '';
    for (let i = 0; i < 5; i++) {
        if (i < filledCount) {
            dots += `<span class="confidence-dot filled" style="background:${color}"></span>`;
        } else {
            dots += `<span class="confidence-dot empty"></span>`;
        }
    }

    return `<span class="confidence-indicator"><span class="confidence-dots">${dots}</span><span class="confidence-label">${pct}%</span></span>`;
}

function formatTxidLink(txid, network) {
    if (!txid) return '<span class="text-muted">—</span>';

    const truncated = txid.length > 14
        ? txid.substring(0, 8) + '…' + txid.substring(txid.length - 6)
        : txid;

    const baseUrl = network === 'testnet'
        ? 'https://mempool.space/testnet/tx/'
        : network === 'signet'
        ? 'https://mempool.space/signet/tx/'
        : 'https://mempool.space/tx/';

    return `<a href="${baseUrl}${encodeURIComponent(txid)}" target="_blank" rel="noopener noreferrer" title="${escapeHtml(txid)}">${truncated}</a>`;
}

function updateHeirClaimsStatusRow() {
    const display = document.getElementById('status-display');
    if (!display) return;

    // Remove existing heir claims row if present
    const existing = document.getElementById('status-heir-claims');
    if (existing) existing.remove();

    const heirClaimCount = spendEvents.filter(e => e.spend_type === 'heir_claim').length;
    const row = document.createElement('div');
    row.className = 'status-item';
    row.id = 'status-heir-claims';

    if (heirClaimCount > 0) {
        row.innerHTML = `
            <span class="label">Heir Claims</span>
            <span class="value" style="color: #ef4444;">⚠️ ${heirClaimCount} detected</span>
        `;
    } else {
        row.innerHTML = `
            <span class="label">Heir Claims</span>
            <span class="value" style="color: var(--success);">✅ None</span>
        `;
    }

    display.appendChild(row);
}

// ============================================================================
// Utility Functions
// ============================================================================
function showToast(message, type = 'info') {
    // Remove existing toasts
    document.querySelectorAll('.toast').forEach(t => t.remove());
    
    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.innerHTML = `
        <span class="toast-icon">${type === 'error' ? '❌' : type === 'success' ? '✅' : 'ℹ️'}</span>
        <span class="toast-message">${escapeHtml(message)}</span>
    `;
    document.body.appendChild(toast);
    
    // Trigger animation
    setTimeout(() => toast.classList.add('show'), 10);
    
    // Auto-remove after 4 seconds
    setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 300);
    }, 4000);
}

function showError(message) {
    showToast(message, 'error');
}

function showSuccess(message) {
    showToast(message, 'success');
}

function validateMnemonic(mnemonic) {
    // Basic validation: words and spaces only, 12 or 24 words
    const trimmed = mnemonic.trim().toLowerCase();
    
    if (!trimmed) {
        return { valid: false, error: 'Please enter your recovery phrase' };
    }
    
    // Check for invalid characters (only letters and spaces allowed)
    if (!/^[a-z\s]+$/.test(trimmed)) {
        return { valid: false, error: 'Recovery phrase should contain only words and spaces' };
    }
    
    const words = trimmed.split(/\s+/);
    
    if (words.length !== 12 && words.length !== 24) {
        return { valid: false, error: `Expected 12 or 24 words, got ${words.length}` };
    }
    
    return { valid: true, mnemonic: words.join(' ') };
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
