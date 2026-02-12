/**
 * Svelte stores for NoString app state.
 */
import { writable } from 'svelte/store';

/** Wallet initialization state */
export type AppPhase = 'loading' | 'onboarding' | 'unlock' | 'ready';
export const appPhase = writable<AppPhase>('loading');

/** Current screen/route (only relevant when appPhase === 'ready') */
export type Screen = 'setup' | 'heirs' | 'vault' | 'dashboard' | 'checkin' | 'deliver' | 'settings';
export const currentScreen = writable<Screen>('setup');

/** Network state */
export const currentNetwork = writable<string>('bitcoin');

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
