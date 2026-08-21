import type { TrustState } from '@revtern/types';

export function formatMoney(minor: number | null | undefined, currency = 'USD', compact = false) {
  const amount = (minor ?? 0) / 100;
  try {
    return new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
      notation: compact ? 'compact' : 'standard',
      maximumFractionDigits: compact ? 1 : 2,
    }).format(amount);
  } catch {
    return `${currency} ${amount.toFixed(2)}`;
  }
}

export function formatNumber(value: number | null | undefined) {
  return new Intl.NumberFormat().format(value ?? 0);
}

export function formatPercent(value: number | null | undefined) {
  return new Intl.NumberFormat(undefined, {
    style: 'percent',
    maximumFractionDigits: 1,
  }).format(value ?? 0);
}

export function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(new Date(value));
}

export function formatDateTime(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
}

export function formatStatus(value: string) {
  return value.replaceAll('_', ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function trustLabel(value: TrustState | string | undefined) {
  switch (value) {
    case 'live':
      return 'Live data';
    case 'reconciled':
      return 'Reconciled';
    case 'estimated':
      return 'Estimated';
    case 'incomplete':
      return 'Incomplete';
    case 'stale':
      return 'Stale';
    case 'unmapped':
      return 'Unmapped';
    default:
      return value ? formatStatus(value) : 'Unknown';
  }
}

export function last30Days() {
  const to = new Date();
  const from = new Date(to);
  from.setDate(from.getDate() - 29);
  return { from: isoDate(from), to: isoDate(to) };
}

function isoDate(value: Date) {
  return value.toISOString().slice(0, 10);
}
