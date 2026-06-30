export function formatMoney(minor?: number | null, currency = "USD") {
  const value = (minor ?? 0) / 100;
  try {
    return new Intl.NumberFormat(undefined, {
      style: "currency",
      currency,
      maximumFractionDigits: Math.abs(value) >= 1000 ? 0 : 2,
    }).format(value);
  } catch {
    return `${currency} ${value.toFixed(2)}`;
  }
}

export function formatNumber(value?: number | null) {
  return new Intl.NumberFormat().format(value ?? 0);
}

export function formatPercent(value?: number | null) {
  return `${(((value ?? 0) as number) * 100).toFixed(1)}%`;
}

export function formatDate(value?: string | null) {
  if (!value) return "—";
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(new Date(value));
}

export function formatDateTime(value?: string | null) {
  if (!value) return "—";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

export function titleize(value?: string | null) {
  if (!value) return "Unknown";
  return value
    .replaceAll("_", " ")
    .replaceAll("-", " ")
    .split(" ")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function last30Days() {
  const to = new Date();
  const from = new Date();
  from.setDate(to.getDate() - 29);
  return { from: toInputDate(from), to: toInputDate(to) };
}

export function toInputDate(date: Date) {
  return date.toISOString().slice(0, 10);
}
