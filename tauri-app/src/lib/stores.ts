/**
 * Svelte stores for NoString app state.
 */
import { writable } from 'svelte/store';

/** Current screen/route */
export type Screen = 'setup' | 'heirs' | 'vault' | 'dashboard' | 'checkin' | 'deliver';
export const currentScreen = writable<Screen>('setup');

/** Co-signer registration state */
export const cosignerRegistered = writable(false);
export const cosignerLabel = writable('');

/** Vault state */
export const vaultAddress = writable<string | null>(null);
export const vaultCreated = writable(false);

/** Heir list (labels only, for UI display) */
export const heirLabels = writable<string[]>([]);

/** Signing session active */
export const signingSessionActive = writable(false);

/** App error message */
export const appError = writable<string | null>(null);

/** Navigate to a screen */
export function navigate(screen: Screen) {
  appError.set(null);
  currentScreen.set(screen);
}
