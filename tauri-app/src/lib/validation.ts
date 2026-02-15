/**
 * Pure validation functions — testable without Svelte/Tauri.
 */

export interface XpubValidation {
  valid: boolean;
  error: string;
}

/**
 * Validate an xpub/tpub/descriptor string and check network consistency.
 */
export function validateXpubInput(input: string, network: string): XpubValidation {
  const v = input.trim();

  if (!v) {
    return { valid: false, error: 'Please paste your xpub' };
  }

  // Must start with known prefix or be a descriptor
  if (!v.startsWith('xpub') && !v.startsWith('tpub') &&
      !v.startsWith('ypub') && !v.startsWith('zpub') &&
      !v.startsWith('[')) {
    return { valid: false, error: 'Expected xpub, tpub, or descriptor (e.g. [fingerprint/path]xpub...)' };
  }

  // Network mismatch: testnet key on mainnet
  const hasTestnetKey = v.includes('tpub');
  const hasMainnetKey = v.includes('xpub') && !hasTestnetKey;

  if (network === 'bitcoin' && hasTestnetKey) {
    return {
      valid: false,
      error: 'This looks like a testnet key (tpub) but you selected Mainnet. Go back and change the network, or paste a mainnet xpub.',
    };
  }

  if (network !== 'bitcoin' && hasMainnetKey) {
    return {
      valid: false,
      error: `This looks like a mainnet key (xpub) but you selected ${network}. Go back and change the network, or paste a testnet tpub.`,
    };
  }

  return { valid: true, error: '' };
}

/**
 * Derive expected xpub prefix for a given network.
 */
export function xpubPrefixForNetwork(network: string): string {
  return network === 'bitcoin' ? 'xpub' : 'tpub';
}

/**
 * Default Electrum server URL for a network.
 */
export function defaultElectrumUrl(network: string): string {
  const urls: Record<string, string> = {
    bitcoin: 'ssl://electrum.blockstream.info:50002',
    testnet: 'ssl://electrum.blockstream.info:60002',
    signet: 'ssl://mempool.space:60602',
  };
  return urls[network] || '';
}
