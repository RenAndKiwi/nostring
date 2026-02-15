import { describe, it, expect } from 'vitest';
import { validateXpubInput, xpubPrefixForNetwork, defaultElectrumUrl } from './validation';

describe('validateXpubInput', () => {
  // Empty input
  it('rejects empty input', () => {
    const r = validateXpubInput('', 'testnet');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('paste your xpub');
  });

  it('rejects whitespace-only input', () => {
    const r = validateXpubInput('   ', 'testnet');
    expect(r.valid).toBe(false);
  });

  // Invalid prefix
  it('rejects random string', () => {
    const r = validateXpubInput('not_an_xpub', 'testnet');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('Expected xpub');
  });

  it('rejects partial prefix', () => {
    expect(validateXpubInput('xpu', 'bitcoin').valid).toBe(false);
    expect(validateXpubInput('tpu', 'testnet').valid).toBe(false);
  });

  // Valid raw keys
  it('accepts tpub on testnet', () => {
    const r = validateXpubInput('tpubD6NzVbkrYhZ4XgiXtGrdW5XDZA5gE4REcK', 'testnet');
    expect(r.valid).toBe(true);
    expect(r.error).toBe('');
  });

  it('accepts xpub on mainnet', () => {
    const r = validateXpubInput('xpubD6NzVbkrYhZ4XgiXtGrdW5XDZA5gE4REcK', 'bitcoin');
    expect(r.valid).toBe(true);
  });

  it('accepts tpub on signet', () => {
    expect(validateXpubInput('tpubABC123', 'signet').valid).toBe(true);
  });

  // Descriptors
  it('accepts descriptor with tpub on testnet', () => {
    const r = validateXpubInput("[aabbccdd/86'/1'/0']tpubD6NzVbkrYhZ4", 'testnet');
    expect(r.valid).toBe(true);
  });

  it('accepts descriptor with xpub on mainnet', () => {
    const r = validateXpubInput("[aabbccdd/86'/0'/0']xpubD6NzVbkrYhZ4", 'bitcoin');
    expect(r.valid).toBe(true);
  });

  // Network mismatches
  it('rejects tpub on mainnet', () => {
    const r = validateXpubInput('tpubD6NzVbkrYhZ4', 'bitcoin');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('testnet key');
    expect(r.error).toContain('Mainnet');
  });

  it('rejects xpub on testnet', () => {
    const r = validateXpubInput('xpubD6NzVbkrYhZ4', 'testnet');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('mainnet key');
    expect(r.error).toContain('testnet');
  });

  it('rejects xpub on signet', () => {
    const r = validateXpubInput('xpubD6NzVbkrYhZ4', 'signet');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('mainnet key');
  });

  it('rejects tpub descriptor on mainnet', () => {
    const r = validateXpubInput("[aabbccdd/86'/1'/0']tpubD6NzVbkr", 'bitcoin');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('testnet key');
  });

  it('rejects xpub descriptor on testnet', () => {
    const r = validateXpubInput("[aabbccdd/86'/0'/0']xpubD6NzVbkr", 'testnet');
    expect(r.valid).toBe(false);
    expect(r.error).toContain('mainnet key');
  });

  // ypub/zpub accepted
  it('accepts ypub', () => {
    expect(validateXpubInput('ypubABC123', 'bitcoin').valid).toBe(true);
  });

  it('accepts zpub', () => {
    expect(validateXpubInput('zpubABC123', 'bitcoin').valid).toBe(true);
  });
});

describe('xpubPrefixForNetwork', () => {
  it('returns xpub for mainnet', () => {
    expect(xpubPrefixForNetwork('bitcoin')).toBe('xpub');
  });

  it('returns tpub for testnet', () => {
    expect(xpubPrefixForNetwork('testnet')).toBe('tpub');
  });

  it('returns tpub for signet', () => {
    expect(xpubPrefixForNetwork('signet')).toBe('tpub');
  });
});

describe('defaultElectrumUrl', () => {
  it('returns correct URL for each network', () => {
    expect(defaultElectrumUrl('bitcoin')).toContain('50002');
    expect(defaultElectrumUrl('testnet')).toContain('60002');
    expect(defaultElectrumUrl('signet')).toContain('60602');
  });

  it('returns empty for unknown network', () => {
    expect(defaultElectrumUrl('regtest')).toBe('');
  });
});
