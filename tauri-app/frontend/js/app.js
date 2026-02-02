// NoString Desktop App - Frontend Logic

const { invoke } = window.__TAURI__.core;

// ============================================================================
// State
// ============================================================================
let currentTab = 'status';
let isUnlocked = false;
let policyStatus = null;

// ============================================================================
// Initialization
// ============================================================================
document.addEventListener('DOMContentLoaded', async () => {
    console.log('NoString initializing...');
    
    // Setup tab navigation
    setupTabs();
    
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
}

// ============================================================================
// Setup Screen (No seed yet)
// ============================================================================
function showSetupScreen() {
    const content = document.getElementById('content');
    content.innerHTML = `
        <div class="setup-screen">
            <h2>Welcome to NoString</h2>
            <p>Sovereign communications for life — and beyond.</p>
            
            <div class="setup-options">
                <button id="btn-create-seed" class="btn-primary">
                    Create New Seed
                </button>
                <button id="btn-import-seed" class="btn-secondary">
                    Import Existing Seed
                </button>
            </div>
            
            <div id="seed-display" class="hidden">
                <h3>Your Recovery Phrase</h3>
                <p class="warning">Write this down and store it safely. Never share it.</p>
                <div id="mnemonic-words" class="mnemonic-grid"></div>
                <div class="password-setup">
                    <label>Encryption Password:</label>
                    <input type="password" id="password-input" placeholder="Enter password">
                    <input type="password" id="password-confirm" placeholder="Confirm password">
                    <button id="btn-confirm-seed" class="btn-primary">Confirm & Encrypt</button>
                </div>
            </div>
            
            <div id="import-form" class="hidden">
                <h3>Import Recovery Phrase</h3>
                <textarea id="import-mnemonic" placeholder="Enter your 12 or 24 word recovery phrase"></textarea>
                <div class="password-setup">
                    <label>Encryption Password:</label>
                    <input type="password" id="import-password" placeholder="Enter password">
                    <input type="password" id="import-password-confirm" placeholder="Confirm password">
                    <button id="btn-confirm-import" class="btn-primary">Import & Encrypt</button>
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
                `<span class="word"><span class="num">${i + 1}.</span> ${word}</span>`
            ).join('');
            
            document.getElementById('seed-display').classList.remove('hidden');
            document.querySelector('.setup-options').classList.add('hidden');
            
            // Store mnemonic temporarily
            document.getElementById('seed-display').dataset.mnemonic = result.data;
            
            document.getElementById('btn-confirm-seed').addEventListener('click', confirmNewSeed);
        } else {
            alert('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to create seed:', err);
        alert('Failed to create seed');
    }
}

async function confirmNewSeed() {
    const mnemonic = document.getElementById('seed-display').dataset.mnemonic;
    const password = document.getElementById('password-input').value;
    const confirm = document.getElementById('password-confirm').value;
    
    if (password !== confirm) {
        alert('Passwords do not match');
        return;
    }
    
    if (password.length < 8) {
        alert('Password must be at least 8 characters');
        return;
    }
    
    try {
        const result = await invoke('import_seed', { mnemonic, password });
        
        if (result.success) {
            isUnlocked = true;
            showMainApp();
        } else {
            alert('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to import seed:', err);
        alert('Failed to encrypt seed');
    }
}

function showImportForm() {
    document.getElementById('import-form').classList.remove('hidden');
    document.querySelector('.setup-options').classList.add('hidden');
    
    document.getElementById('btn-confirm-import').addEventListener('click', importExistingSeed);
}

async function importExistingSeed() {
    const mnemonic = document.getElementById('import-mnemonic').value.trim();
    const password = document.getElementById('import-password').value;
    const confirm = document.getElementById('import-password-confirm').value;
    
    if (password !== confirm) {
        alert('Passwords do not match');
        return;
    }
    
    if (password.length < 8) {
        alert('Password must be at least 8 characters');
        return;
    }
    
    try {
        const result = await invoke('import_seed', { mnemonic, password });
        
        if (result.success) {
            isUnlocked = true;
            showMainApp();
        } else {
            alert('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to import seed:', err);
        alert('Failed to import seed');
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
            <p>Enter your password to unlock</p>
            <input type="password" id="unlock-password" placeholder="Password">
            <button id="btn-unlock" class="btn-primary">Unlock</button>
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
            alert('Incorrect password');
        }
    } catch (err) {
        console.error('Failed to unlock:', err);
        alert('Failed to unlock wallet');
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
                <h2>Inheritance Status</h2>
                <div id="status-display">Loading...</div>
                <button id="btn-refresh-status" class="btn-secondary">Refresh</button>
            </div>
            
            <div class="checkin-card">
                <h3>Check In</h3>
                <p>Prove you're alive and reset your timelock.</p>
                <button id="btn-checkin" class="btn-primary">Initiate Check-in</button>
            </div>
        </section>
        
        <section id="heirs-tab" class="tab-content">
            <h2>Heirs</h2>
            <p>Manage who inherits if you don't check in.</p>
            <div id="heirs-list">Coming soon...</div>
        </section>
        
        <section id="backup-tab" class="tab-content">
            <h2>Backup</h2>
            <p>Split your seed with Shamir's Secret Sharing.</p>
            <div class="backup-options">
                <button class="btn-secondary">Generate SLIP-39 Shares</button>
                <button class="btn-secondary">Generate Codex32 Shares</button>
            </div>
        </section>
        
        <section id="settings-tab" class="tab-content">
            <h2>Settings</h2>
            <div class="setting">
                <label>Electrum Server:</label>
                <input type="text" id="electrum-url" placeholder="ssl://electrum.blockstream.info:60002">
                <button id="btn-save-electrum" class="btn-secondary">Save</button>
            </div>
            <div class="setting">
                <button id="btn-lock" class="btn-secondary">Lock Wallet</button>
            </div>
        </section>
    `;
    
    // Update tabs
    document.querySelector('#tabs').innerHTML = `
        <button data-tab="status" class="active">Status</button>
        <button data-tab="heirs">Heirs</button>
        <button data-tab="backup">Backup</button>
        <button data-tab="settings">Settings</button>
    `;
    
    setupTabs();
    
    // Setup event handlers
    document.getElementById('btn-refresh-status').addEventListener('click', refreshStatus);
    document.getElementById('btn-checkin').addEventListener('click', initiateCheckin);
    document.getElementById('btn-lock').addEventListener('click', lockWallet);
    document.getElementById('btn-save-electrum').addEventListener('click', saveElectrumUrl);
    
    // Load initial data
    refreshStatus();
    loadElectrumUrl();
}

async function refreshStatus() {
    const display = document.getElementById('status-display');
    display.innerHTML = 'Loading...';
    
    try {
        const result = await invoke('refresh_policy_status');
        
        if (result.success) {
            policyStatus = result.data;
            const urgencyClass = policyStatus.urgency === 'ok' ? 'status-ok' : 
                                 policyStatus.urgency === 'warning' ? 'status-warning' : 'status-critical';
            
            display.innerHTML = `
                <div class="status-item ${urgencyClass}">
                    <span class="label">Status:</span>
                    <span class="value">${policyStatus.urgency.toUpperCase()}</span>
                </div>
                <div class="status-item">
                    <span class="label">Days Remaining:</span>
                    <span class="value">${policyStatus.days_remaining.toFixed(1)}</span>
                </div>
                <div class="status-item">
                    <span class="label">Blocks Remaining:</span>
                    <span class="value">${policyStatus.blocks_remaining.toLocaleString()}</span>
                </div>
                <div class="status-item">
                    <span class="label">Current Block:</span>
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
            alert('Error: ' + result.error);
        }
    } catch (err) {
        console.error('Failed to initiate check-in:', err);
        alert('Failed to initiate check-in');
    }
}

// ============================================================================
// QR Code Display (for unsigned PSBT)
// ============================================================================
function showPsbtQrCode(psbtBase64) {
    const modal = document.getElementById('qr-modal');
    const container = document.getElementById('qr-container');
    const instructions = document.getElementById('qr-instructions');
    
    // Clear previous QR
    container.innerHTML = '';
    
    // Generate QR code
    // For large PSBTs, we use alphanumeric mode which is more efficient
    QRCode.toCanvas(psbtBase64.toUpperCase(), {
        width: 350,
        margin: 2,
        errorCorrectionLevel: 'L' // Low EC for larger data capacity
    }, (error, canvas) => {
        if (error) {
            console.error('QR generation error:', error);
            // Fallback: show text for manual copy
            container.innerHTML = `<p style="color: var(--error);">PSBT too large for QR. Use copy button.</p>`;
        } else {
            container.appendChild(canvas);
        }
    });
    
    instructions.textContent = 'Scan this QR with Electrum (watch-only wallet) to sign the check-in transaction.';
    
    // Show modal
    modal.classList.remove('hidden');
    
    // Setup event handlers
    document.getElementById('qr-modal-close').onclick = () => modal.classList.add('hidden');
    document.getElementById('btn-copy-psbt').onclick = () => copyPsbtToClipboard();
    document.getElementById('btn-scan-response').onclick = () => openQrScanner();
}

function copyPsbtToClipboard() {
    if (currentPsbtBase64) {
        navigator.clipboard.writeText(currentPsbtBase64).then(() => {
            alert('PSBT copied to clipboard.\n\nPaste into Electrum: Tools → Load transaction → From text');
        }).catch(err => {
            console.error('Copy failed:', err);
            // Fallback: show in prompt
            prompt('Copy this PSBT:', currentPsbtBase64);
        });
    }
}

// ============================================================================
// QR Code Scanner (for signed PSBT response)
// ============================================================================
let scannerStream = null;
let scannerAnimationId = null;

function openQrScanner() {
    const qrModal = document.getElementById('qr-modal');
    const scannerModal = document.getElementById('scanner-modal');
    const video = document.getElementById('scanner-video');
    const status = document.getElementById('scanner-status');
    
    // Hide QR modal, show scanner
    qrModal.classList.add('hidden');
    scannerModal.classList.remove('hidden');
    
    status.textContent = 'Initializing camera...';
    status.classList.remove('scanner-success');
    
    // Request camera access
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
    
    // Setup close handler
    document.getElementById('scanner-modal-close').onclick = () => closeScanner();
}

function startScanning() {
    const video = document.getElementById('scanner-video');
    const canvas = document.getElementById('scanner-canvas');
    const ctx = canvas.getContext('2d');
    const status = document.getElementById('scanner-status');
    
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
                return; // Stop scanning
            }
        }
        
        scannerAnimationId = requestAnimationFrame(scan);
    }
    
    scan();
}

function closeScanner() {
    const scannerModal = document.getElementById('scanner-modal');
    scannerModal.classList.add('hidden');
    
    // Stop camera
    if (scannerStream) {
        scannerStream.getTracks().forEach(track => track.stop());
        scannerStream = null;
    }
    
    // Stop scanning loop
    if (scannerAnimationId) {
        cancelAnimationFrame(scannerAnimationId);
        scannerAnimationId = null;
    }
}

async function handleScannedPsbt(psbtData) {
    const status = document.getElementById('scanner-status');
    
    // Stop scanning
    if (scannerAnimationId) {
        cancelAnimationFrame(scannerAnimationId);
        scannerAnimationId = null;
    }
    
    status.textContent = '✓ PSBT detected! Processing...';
    status.classList.add('scanner-success');
    
    try {
        // Send signed PSBT to backend for broadcast
        const result = await invoke('broadcast_signed_psbt', { signedPsbt: psbtData });
        
        if (result.success) {
            closeScanner();
            alert('✓ Check-in broadcast successfully!\n\nTxid: ' + result.data);
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
    alert('Electrum server saved');
}
