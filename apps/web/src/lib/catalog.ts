import type { SourceProductRecord } from "@revtern/types";

export interface CatalogDraftGroup {
  client_id: string;
  display_name: string;
  product_kind: string;
  billing_period: string;
  reporting_category: string;
  source_product_ids: string[];
  reason: string;
}

export function buildCatalogDraft(products: SourceProductRecord[]) {
  const groups = new Map<string, CatalogDraftGroup>();
  for (const product of products.filter((item) => item.mapping_state === "unmapped")) {
    const key = normalizedKey(product);
    const existing = groups.get(key);
    if (existing) {
      existing.source_product_ids.push(product.id);
      existing.reason = "Grouped by app, billing period, kind, and normalized SKU.";
      continue;
    }
    groups.set(key, {
      client_id: `draft_${key}`,
      display_name: readableProductName(product),
      product_kind: product.product_kind === "unknown" ? inferredKind(product) : product.product_kind,
      billing_period: product.billing_period === "unknown" ? inferredPeriod(product) : product.billing_period,
      reporting_category: product.product_kind === "lifetime" ? "Lifetime" : "Core",
      source_product_ids: [product.id],
      reason: "Suggested from source product identity and purchase metadata.",
    });
  }
  return [...groups.values()];
}

function normalizedKey(product: SourceProductRecord) {
  const raw = product.external_product_id ?? product.display_name ?? product.id;
  const compact = raw
    .toLowerCase()
    .replace(/^com\.[a-z0-9_.-]+\./, "")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/(^_|_$)/g, "");
  const period = product.billing_period === "unknown" ? inferredPeriod(product) : product.billing_period;
  const kind = product.product_kind === "unknown" ? inferredKind(product) : product.product_kind;
  return `${compact}_${kind}_${period}`;
}

function readableProductName(product: SourceProductRecord) {
  const raw = product.display_name ?? product.external_product_id ?? "Product";
  const last = raw.split(".").at(-1) ?? raw;
  return last
    .replace(/[_-]+/g, " ")
    .split(" ")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function inferredKind(product: SourceProductRecord) {
  const raw = `${product.external_product_id ?? ""} ${product.display_name ?? ""}`.toLowerCase();
  if (raw.includes("life")) return "lifetime";
  if (raw.includes("month") || raw.includes("annual") || raw.includes("year") || raw.includes("week")) return "subscription";
  if (raw.includes("coin") || raw.includes("credit") || raw.includes("token") || raw.includes("pack")) return "consumable";
  return "non_consumable";
}

function inferredPeriod(product: SourceProductRecord) {
  const raw = `${product.external_product_id ?? ""} ${product.display_name ?? ""}`.toLowerCase();
  if (raw.includes("week")) return "weekly";
  if (raw.includes("month")) return "monthly";
  if (raw.includes("annual") || raw.includes("year")) return "annual";
  if (raw.includes("life")) return "lifetime";
  return "none";
}
