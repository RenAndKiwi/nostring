// NoString Desktop App - Frontend Logic
// Brand colors aligned with Bitcoin Butlers

// Demo mode when running outside Tauri
const DEMO_MODE = !window.__TAURI__;

const invoke = DEMO_MODE 
    ? async (cmd, args) => {
        console.log('[DEMO] invoke:', cmd, args);
        // Mock responses for demo mode
        const mocks = {
            'has_seed': false,
            'create_seed': { 
                success: true, 
                data: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art'
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

// ============================================================================
// Initialization
// ============================================================================
document.addEventListener('DOMContentLoaded', async () => {
    console.log('NoString initializing...');
    
    // Check if seed exists
    const hasSeed = await invoke('has_seed');
    
    if (hasSeed) {
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
// Setup Screen (No seed yet)
// ============================================================================
function showSetupScreen() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <div class="setup-screen">
            <h2>Welcome to NoString</h2>
            <p>Sovereign Bitcoin inheritance. No trusted third parties.</p>
            
            <div class="setup-options">
                <button type="button" id="btn-create-seed" class="btn-primary">
                    ✨ Create New Seed
                </button>
                <button type="button" id="btn-import-seed" class="btn-secondary">
                    📥 Import Existing Seed
                </button>
            </div>
            
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
            
            <div id="import-form" class="hidden">
                <h3>Import Recovery Phrase</h3>
                <textarea id="import-mnemonic" placeholder="Enter your 12 or 24 word recovery phrase, separated by spaces"></textarea>
                <div class="password-setup">
                    <label>Encryption Password:</label>
                    <input type="password" id="import-password" placeholder="Minimum 8 characters">
                    <label>Confirm Password:</label>
                    <input type="password" id="import-password-confirm" placeholder="Confirm password">
                    <button type="button" id="btn-confirm-import" class="btn-primary">Import & Encrypt</button>
                    <button type="button" id="btn-back-setup" class="btn-secondary">Back</button>
                </div>
            </div>
        </div>
    `;
    
    document.getElementById('btn-create-seed').addEventListener('click', createNewSeed);
    document.getElementById('btn-import-seed').addEventListener('click', showImportForm);
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
    document.getElementById('btn-back-setup').addEventListener('click', () => {
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
                </div>
                
                <h2>👥 Add Your First Heir</h2>
                <p class="text-muted">Who should inherit your Bitcoin if something happens to you?</p>
                
                <div class="wizard-form">
                    <div class="form-row">
                        <label>Label (e.g., "Spouse", "Child")</label>
                        <input type="text" id="wizard-heir-label" placeholder="Spouse">
                    </div>
                    
                    <div class="form-row">
                        <label>Their Bitcoin Address or xpub</label>
                        <textarea id="wizard-heir-address" placeholder="bc1q... or xpub..."></textarea>
                        <p class="hint">Ask your heir to generate an address in their wallet (Electrum, BlueWallet, hardware wallet, etc.)</p>
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
            wizardStep = 3;
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
            wizardStep = 3;
            renderWizardStep();
        });
        document.getElementById('btn-wizard-add-more').addEventListener('click', () => {
            wizardStep = 1;
            renderWizardStep();
        });
        
    } else if (wizardStep === 3) {
        content.innerHTML = `
            <div class="wizard">
                <div class="wizard-progress">
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step done">✓</span>
                    <span class="step-line done"></span>
                    <span class="step active">3</span>
                </div>
                
                <h2>🎉 You're All Set!</h2>
                <p class="text-muted">${wizardHeirs.length > 0 ? 'Your inheritance policy is ready.' : 'You can add heirs later in the Heirs tab.'}</p>
                
                <div class="wizard-complete">
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
        showError('Please enter their Bitcoin address or xpub');
        return;
    }
    
    // Basic validation - starts with bc1, 1, 3, xpub, or ypub/zpub
    if (!/^(bc1|1|3|xpub|ypub|zpub|tpub)/i.test(address)) {
        showError('Please enter a valid Bitcoin address or xpub');
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
                    <label>Extended Public Key (xpub or full descriptor)</label>
                    <textarea id="heir-xpub" placeholder="xpub... or [fingerprint/path]xpub..."></textarea>
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
    
    // Load initial data
    refreshStatus();
    loadElectrumUrl();
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
            `;
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
            showSuccess('Check-in broadcast successfully!\n\nTxid: ' + result.data);
            refreshStatus();
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
