import { Platform } from 'react-native';

export const Colors = {
  light: {
    text: '#1C1A18',
    background: '#FFFFFF',
    backgroundElement: '#F6F5F3',
    backgroundSelected: '#EEEAE5',
    textSecondary: '#67615B',
    separator: '#DDD9D4',
    accent: '#D2663E',
    accentSoft: '#F8E7DF',
    positive: '#26734A',
    negative: '#B44336',
    warning: '#966010',
  },
  dark: {
    text: '#F4F0EA',
    background: '#141311',
    backgroundElement: '#1F1D1A',
    backgroundSelected: '#2B2723',
    textSecondary: '#B5ADA4',
    separator: '#3A3530',
    accent: '#E17A51',
    accentSoft: '#3B2921',
    positive: '#63B184',
    negative: '#E47D70',
    warning: '#D4A257',
  },
} as const;

export type ThemeColor = keyof typeof Colors.light & keyof typeof Colors.dark;

export const Fonts = Platform.select({
  ios: {
    sans: 'system-ui',
    serif: 'ui-serif',
    rounded: 'ui-rounded',
    mono: 'ui-monospace',
  },
  default: {
    sans: 'normal',
    serif: 'serif',
    rounded: 'normal',
    mono: 'monospace',
  },
  web: {
    sans: 'system-ui',
    serif: 'ui-serif',
    rounded: 'ui-rounded',
    mono: 'ui-monospace',
  },
});

export const Spacing = {
  half: 2,
  one: 4,
  two: 8,
  three: 16,
  four: 24,
  five: 32,
  six: 64,
} as const;

export const Radii = {
  control: 10,
  panel: 12,
  compact: 7,
} as const;

export const MaxContentWidth = 800;
