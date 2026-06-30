export type Id = string;

export type TrustState = "live" | "estimated" | "reconciled" | "incomplete" | "stale" | "unmapped";

export interface MetricValue<T = number> {
  value: T;
  definition: string;
  estimated: boolean;
  trust_state: TrustState | string;
}

export interface Period {
  from: string;
  to: string;
}

export interface OverviewResponse {
  period: Period;
  currency: string;
  metrics: {
    gross_revenue_minor: MetricValue<number>;
    net_revenue_minor: MetricValue<number>;
    refund_amount_minor: MetricValue<number>;
    active_subscriptions: MetricValue<number>;
    new_subscriptions: MetricValue<number>;
    renewals: MetricValue<number>;
    churned_subscriptions: MetricValue<number>;
    refund_rate: MetricValue<number>;
  };
  warnings: string[];
}

export interface SetupStatus {
  needs_setup: boolean;
  auth_mode: "single_user" | "reverse_proxy" | "disabled";
}

export interface UserSummary {
  id: Id;
  email: string;
  role: string;
}

export interface WorkspaceSummary {
  id: Id;
  name: string;
}

export interface MeResponse {
  user: UserSummary;
  workspace: WorkspaceSummary;
}

export interface AppRecord {
  id: Id;
  name: string;
  platform_bundle_id?: string | null;
  apple_bundle_id?: string | null;
  google_package_name?: string | null;
  default_currency?: string | null;
  created_at: string;
  updated_at: string;
}

export interface DataSourceRecord {
  id: Id;
  workspace_id: Id;
  app_id?: Id | null;
  app_name?: string | null;
  source_type: SourceType;
  name: string;
  status: string;
  has_credentials: boolean;
  credential_keys: string[];
  has_webhook_secret: boolean;
  last_event_at?: string | null;
  last_sync_at?: string | null;
  last_error?: string | null;
  created_at: string;
  updated_at: string;
  webhook_url: string;
  setup_checklist: Array<{ key: string; label: string; done: boolean }>;
}

export type SourceType =
  | "app_store"
  | "google_play"
  | "revenuecat"
  | "stripe"
  | "paddle"
  | "csv"
  | "custom_api";

export interface SourceProductRecord {
  id: Id;
  workspace_id: Id;
  data_source_id: Id;
  data_source_name?: string | null;
  app_id?: Id | null;
  source_type: SourceType;
  platform?: string | null;
  external_product_id?: string | null;
  external_base_plan_id?: string | null;
  external_offer_id?: string | null;
  display_name?: string | null;
  product_kind: ProductKind;
  billing_period: BillingPeriod;
  amount_minor?: number | null;
  currency?: string | null;
  mapping_state: "unmapped" | "mapped" | "ignored" | string;
  logical_product_id?: Id | null;
  logical_product_name?: string | null;
  first_seen_at: string;
  last_seen_at: string;
}

export type ProductKind = "subscription" | "consumable" | "non_consumable" | "lifetime" | "unknown" | string;
export type BillingPeriod = "weekly" | "monthly" | "annual" | "lifetime" | "none" | "unknown" | string;

export interface LogicalProductRecord {
  id: Id;
  workspace_id: Id;
  app_id?: Id | null;
  display_name: string;
  product_kind: ProductKind;
  billing_period: BillingPeriod;
  reporting_category?: string | null;
  active: boolean;
  created_from: string;
  created_at: string;
  source_products: Array<Pick<SourceProductRecord, "id" | "source_type" | "external_product_id" | "display_name" | "platform">>;
}

export interface RawEventRecord {
  id: Id;
  workspace_id: Id;
  data_source_id: Id;
  data_source_name?: string | null;
  source_type: SourceType;
  source_event_id: string;
  source_event_type?: string | null;
  source_app_id?: string | null;
  source_product_id?: Id | null;
  source_product_name?: string | null;
  occurred_at: string;
  received_at: string;
  payload: unknown;
  processing_payload?: unknown | null;
  payload_sha256: string;
  signature_verified: boolean;
  processing_status: string;
  processing_error?: string | null;
}

export interface NormalizedEventRecord {
  id: Id;
  raw_event_id: Id;
  data_source_id: Id;
  app_id?: Id | null;
  source_product_id?: Id | null;
  source_product_name?: string | null;
  logical_product_id?: Id | null;
  logical_product_name?: string | null;
  event_type: string;
  platform?: string | null;
  customer_key?: string | null;
  transaction_key?: string | null;
  original_transaction_key?: string | null;
  subscription_key?: string | null;
  amount_minor?: number | null;
  currency?: string | null;
  country?: string | null;
  occurred_at: string;
  normalization_version: string;
  confidence: number;
  warnings: string[];
}

export interface TransactionRecord {
  id: Id;
  app_id?: Id | null;
  app_name?: string | null;
  source_product_id?: Id | null;
  source_product_name?: string | null;
  logical_product_id?: Id | null;
  logical_product_name?: string | null;
  customer_id?: Id | null;
  platform?: string | null;
  transaction_key: string;
  original_transaction_key?: string | null;
  source_type: SourceType;
  purchase_time: string;
  amount_minor: number;
  currency: string;
  country?: string | null;
  status: string;
  source_status?: string | null;
  status_reason?: string | null;
  status_updated_at: string;
  refunded_at?: string | null;
  refund_amount_minor?: number | null;
  created_from_event_id?: Id | null;
  latest_event_id?: Id | null;
  updated_at: string;
}

export interface SubscriptionRecord {
  id: Id;
  app_id?: Id | null;
  app_name?: string | null;
  source_product_id?: Id | null;
  source_product_name?: string | null;
  logical_product_id?: Id | null;
  logical_product_name?: string | null;
  customer_id?: Id | null;
  platform?: string | null;
  subscription_key: string;
  original_transaction_key?: string | null;
  status: string;
  started_at: string;
  current_period_start?: string | null;
  current_period_end?: string | null;
  cancelled_at?: string | null;
  expired_at?: string | null;
  will_renew: boolean;
  in_grace_period: boolean;
  in_billing_retry: boolean;
  latest_transaction_id?: Id | null;
  updated_at: string;
}

export interface SyncRunRecord {
  id: Id;
  workspace_id: Id;
  data_source_id?: Id | null;
  data_source_name?: string | null;
  sync_type: string;
  status: string;
  cursor?: string | null;
  started_at: string;
  finished_at?: string | null;
  records_seen: number;
  records_inserted: number;
  error?: string | null;
}

export interface JobRecord {
  id: Id;
  queue: string;
  job_type: string;
  payload: unknown;
  status: string;
  run_after: string;
  attempts: number;
  max_attempts: number;
  locked_at?: string | null;
  locked_by?: string | null;
  last_error?: string | null;
  created_at: string;
}

export interface DailyRevenuePoint {
  date: string;
  gross_revenue_minor: number;
  refund_amount_minor: number;
  net_revenue_minor: number;
  purchase_count: number;
  renewal_count: number;
}

export interface DailySubscriptionPoint {
  date: string;
  new_subscription_count: number;
  renewal_count: number;
  cancel_count: number;
  expiration_count: number;
  trial_start_count: number;
  trial_conversion_count: number;
}
