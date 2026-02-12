/**
 * Tauri command invocation wrappers.
 *
 * Each function maps to a #[tauri::command] in the Rust backend.
 * All return CcdResult<T> = { success: boolean, data?: T, error?: string }
 */
import { invoke } from '@tauri-apps/api/core';

// ─── Types ──────────────────────────────────────────────────────────────────

export interface CcdResult<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface HeartbeatStatus {
  current_block: number;
  expiry_block: number;
  blocks_remaining: number;
  days_remaining: number;
  action: string;
  elapsed_fraction: number;
}

export interface DeliveryReport {
  delivered: string[];
  skipped: string[];
  failed: { heir_label: string; error: string }[];
}

// ─── CCD Commands ───────────────────────────────────────────────────────────

export async function registerCosigner(
  pubkeyHex: string,
  chainCodeHex: string,
  label: string
): Promise<CcdResult<any>> {
  return invoke('register_cosigner', {
    pubkey_hex: pubkeyHex,
    chain_code_hex: chainCodeHex,
    label,
  });
}

export async function createCcdVault(
  timelockBlocks: number,
  addressIndex?: number
): Promise<CcdResult<any>> {
  return invoke('create_ccd_vault', {
    timelock_blocks: timelockBlocks,
    address_index: addressIndex ?? null,
  });
}

export async function buildCheckinPsbt(): Promise<CcdResult<string>> {
  return invoke('build_checkin_psbt');
}

export async function getHeartbeatStatus(): Promise<CcdResult<HeartbeatStatus>> {
  return invoke('get_heartbeat_status');
}

export async function getCcdLoadError(): Promise<CcdResult<string | null>> {
  return invoke('get_ccd_load_error');
}

// ─── Signing Ceremony ───────────────────────────────────────────────────────

export async function startSigningSession(): Promise<CcdResult<any>> {
  return invoke('start_signing_session');
}

export async function submitNonces(
  ownerPubnoncesHex: string[],
  cosignerResponseJson: string
): Promise<CcdResult<any>> {
  return invoke('submit_nonces', {
    owner_pubnonces_hex: ownerPubnoncesHex,
    cosigner_response_json: cosignerResponseJson,
  });
}

export async function finalizeAndBroadcast(
  ownerPartialSigsHex: string[],
  cosignerPartialsJson: string
): Promise<CcdResult<string>> {
  return invoke('finalize_and_broadcast', {
    owner_partial_sigs_hex: ownerPartialSigsHex,
    cosigner_partials_json: cosignerPartialsJson,
  });
}

export async function cancelSigningSession(): Promise<CcdResult<boolean>> {
  return invoke('cancel_signing_session');
}

// ─── Heir Commands ──────────────────────────────────────────────────────────

// ─── Heir Management ────────────────────────────────────────────────────────

export interface HeirInfo {
  label: string;
  fingerprint: string;
  xpub: string;
  derivation_path: string;
  npub: string | null;
  email: string | null;
  timelock_months: number | null;
}

export async function addHeir(
  label: string,
  xpubOrDescriptor: string,
  timelockMonths?: number,
  npub?: string
): Promise<CcdResult<HeirInfo>> {
  return invoke('add_heir', {
    label,
    xpub_or_descriptor: xpubOrDescriptor,
    timelock_months: timelockMonths ?? null,
    npub: npub || null,
  });
}

export async function listHeirs(): Promise<HeirInfo[]> {
  return invoke('list_heirs');
}

export async function removeHeir(fingerprint: string): Promise<CcdResult<boolean>> {
  return invoke('remove_heir', { fingerprint });
}

export async function exportVaultBackup(): Promise<CcdResult<string>> {
  return invoke('export_vault_backup');
}

export async function deliverDescriptorToHeirs(
  ownerNsec: string,
  relays: string[]
): Promise<CcdResult<DeliveryReport>> {
  return invoke('deliver_descriptor_to_heirs', {
    owner_nsec: ownerNsec,
    relays,
  });
}
