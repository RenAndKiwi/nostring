/** Bitcoin Butlers brand colors — shared across all NoString apps. */
export const colors = {
  // Backgrounds
  background: '#0A0A0A',
  surface: '#1A1A2E',
  surfaceVariant: '#262834',

  // Gold palette
  goldLight: '#FBDC7B',
  gold: '#FDA24A',
  goldDark: '#FF9125',

  // Text
  textPrimary: '#FFFFFF',
  textMuted: '#9CA3AF',

  // Semantic
  success: '#10B981',
  warning: '#F59E0B',
  error: '#EF4444',
  info: '#3B82F6',

  // Borders
  border: '#333333',
  borderFocus: '#FBDC7B',
} as const;

export const gradients = {
  gold: 'linear-gradient(180deg, #FFD700 0%, #FFC107 50%, #FFA500 100%)',
  goldHorizontal: 'linear-gradient(to right, #FBDC7B, #FDA24A)',
} as const;
