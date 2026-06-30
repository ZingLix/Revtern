create table if not exists workspaces (
  id text primary key,
  name text not null,
  created_at timestamptz not null default now()
);

create table if not exists users (
  id text primary key,
  email text not null unique,
  password_hash text not null,
  display_name text,
  role text not null default 'owner',
  created_at timestamptz not null default now(),
  last_login_at timestamptz
);

create table if not exists workspace_users (
  workspace_id text not null references workspaces(id) on delete cascade,
  user_id text not null references users(id) on delete cascade,
  role text not null default 'owner',
  primary key (workspace_id, user_id)
);

create table if not exists sessions (
  id text primary key,
  user_id text not null references users(id) on delete cascade,
  session_hash text not null unique,
  expires_at timestamptz not null,
  created_at timestamptz not null default now(),
  last_seen_at timestamptz not null default now()
);

create table if not exists apps (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  name text not null,
  platform_bundle_id text,
  apple_bundle_id text,
  google_package_name text,
  default_currency text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists data_sources (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  app_id text references apps(id) on delete set null,
  source_type text not null,
  name text not null,
  status text not null default 'waiting_for_events',
  encrypted_credentials text,
  webhook_secret_hash text,
  last_event_at timestamptz,
  last_sync_at timestamptz,
  last_error text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists source_apps (
  id text primary key,
  data_source_id text not null references data_sources(id) on delete cascade,
  app_id text references apps(id) on delete cascade,
  external_app_id text,
  external_bundle_id text,
  external_package_name text,
  unique (data_source_id, external_app_id)
);

create table if not exists logical_products (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  app_id text references apps(id) on delete cascade,
  display_name text not null,
  product_kind text not null default 'unknown',
  billing_period text not null default 'unknown',
  reporting_category text,
  active boolean not null default true,
  created_from text not null default 'catalog_confirmation',
  created_by_user_id text references users(id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists source_products (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  data_source_id text not null references data_sources(id) on delete cascade,
  app_id text references apps(id) on delete cascade,
  source_type text not null,
  platform text,
  external_product_id text,
  external_base_plan_id text,
  external_offer_id text,
  external_price_id text,
  display_name text,
  product_kind text not null default 'unknown',
  billing_period text not null default 'unknown',
  amount_minor bigint,
  currency text,
  raw_metadata jsonb not null default '{}'::jsonb,
  mapping_state text not null default 'unmapped',
  ignored_at timestamptz,
  ignored_by_user_id text references users(id) on delete set null,
  source_product_key text not null,
  first_seen_at timestamptz not null default now(),
  last_seen_at timestamptz not null default now(),
  unique (workspace_id, data_source_id, source_product_key)
);

create table if not exists product_mappings (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  source_product_id text not null references source_products(id) on delete cascade,
  logical_product_id text not null references logical_products(id) on delete cascade,
  mapping_method text not null,
  confidence double precision not null default 1,
  created_by_user_id text references users(id) on delete set null,
  created_at timestamptz not null default now(),
  confirmed_at timestamptz not null default now(),
  active boolean not null default true
);

create unique index if not exists product_mappings_active_source_idx
on product_mappings (workspace_id, source_product_id)
where active = true;

create table if not exists raw_events (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  data_source_id text not null references data_sources(id) on delete cascade,
  source_type text not null,
  source_event_id text not null,
  source_event_type text,
  source_app_id text,
  source_product_id text references source_products(id) on delete set null,
  occurred_at timestamptz not null,
  received_at timestamptz not null default now(),
  payload jsonb not null,
  processing_payload jsonb,
  payload_sha256 text not null,
  signature_verified boolean not null default false,
  processing_status text not null default 'stored',
  processing_error text,
  sync_run_id text,
  unique (data_source_id, source_event_id)
);

create index if not exists raw_events_workspace_occurred_idx on raw_events (workspace_id, occurred_at desc);
create index if not exists raw_events_type_idx on raw_events (workspace_id, source_type, source_event_type);
create index if not exists raw_events_payload_sha_idx on raw_events (payload_sha256);

create table if not exists normalized_events (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  raw_event_id text not null references raw_events(id) on delete cascade,
  data_source_id text not null references data_sources(id) on delete cascade,
  app_id text references apps(id) on delete set null,
  source_product_id text references source_products(id) on delete set null,
  logical_product_id text references logical_products(id) on delete set null,
  event_type text not null,
  platform text,
  customer_key text,
  transaction_key text,
  original_transaction_key text,
  subscription_key text,
  amount_minor bigint,
  currency text,
  country text,
  occurred_at timestamptz not null,
  normalization_version text not null,
  confidence double precision not null default 0.9,
  warnings jsonb not null default '[]'::jsonb,
  created_at timestamptz not null default now(),
  unique (raw_event_id, event_type)
);

create index if not exists normalized_events_workspace_occurred_idx on normalized_events (workspace_id, occurred_at desc);
create index if not exists normalized_events_event_type_idx on normalized_events (workspace_id, event_type);

create table if not exists customers (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  app_user_id text,
  apple_app_account_token text,
  google_obfuscated_account_id text,
  revenuecat_app_user_id text,
  customer_identity_key text generated always as (
    coalesce(app_user_id, apple_app_account_token, google_obfuscated_account_id, revenuecat_app_user_id)
  ) stored,
  first_seen_at timestamptz not null,
  last_seen_at timestamptz not null,
  unique (workspace_id, customer_identity_key)
);

create table if not exists transactions (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  app_id text references apps(id) on delete set null,
  source_product_id text references source_products(id) on delete set null,
  logical_product_id text references logical_products(id) on delete set null,
  customer_id text references customers(id) on delete set null,
  platform text,
  transaction_key text not null,
  original_transaction_key text,
  source_type text not null,
  purchase_time timestamptz not null,
  amount_minor bigint not null default 0,
  currency text not null default 'UNKNOWN',
  country text,
  status text not null,
  source_status text,
  status_reason text,
  status_updated_at timestamptz not null default now(),
  refunded_at timestamptz,
  refund_amount_minor bigint,
  created_from_event_id text references normalized_events(id) on delete set null,
  latest_event_id text references normalized_events(id) on delete set null,
  updated_at timestamptz not null default now(),
  unique (workspace_id, source_type, transaction_key)
);

create index if not exists transactions_workspace_time_idx on transactions (workspace_id, purchase_time desc);
create index if not exists transactions_status_idx on transactions (workspace_id, status);

create table if not exists subscriptions (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  app_id text references apps(id) on delete set null,
  source_product_id text references source_products(id) on delete set null,
  logical_product_id text references logical_products(id) on delete set null,
  customer_id text references customers(id) on delete set null,
  platform text,
  subscription_key text not null,
  original_transaction_key text,
  status text not null,
  started_at timestamptz not null,
  current_period_start timestamptz,
  current_period_end timestamptz,
  cancelled_at timestamptz,
  expired_at timestamptz,
  will_renew boolean not null default true,
  in_grace_period boolean not null default false,
  in_billing_retry boolean not null default false,
  latest_transaction_id text references transactions(id) on delete set null,
  updated_at timestamptz not null default now(),
  unique (workspace_id, subscription_key)
);

create index if not exists subscriptions_status_idx on subscriptions (workspace_id, status);

create table if not exists daily_metrics (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  date date not null,
  app_id text references apps(id) on delete set null,
  app_id_key text generated always as (coalesce(app_id, '')) stored,
  platform text,
  platform_key text generated always as (coalesce(platform, '')) stored,
  logical_product_id text references logical_products(id) on delete set null,
  logical_product_id_key text generated always as (coalesce(logical_product_id, '')) stored,
  country text,
  country_key text generated always as (coalesce(country, '')) stored,
  currency text not null default 'UNKNOWN',
  source_type text not null,
  gross_revenue_minor bigint not null default 0,
  estimated_proceeds_minor bigint not null default 0,
  refund_amount_minor bigint not null default 0,
  net_revenue_minor bigint not null default 0,
  purchase_count bigint not null default 0,
  renewal_count bigint not null default 0,
  new_subscription_count bigint not null default 0,
  active_subscription_count bigint not null default 0,
  cancel_count bigint not null default 0,
  expiration_count bigint not null default 0,
  refund_count bigint not null default 0,
  trial_start_count bigint not null default 0,
  trial_conversion_count bigint not null default 0,
  unique (workspace_id, date, app_id_key, platform_key, logical_product_id_key, country_key, currency, source_type)
);

create table if not exists sync_runs (
  id text primary key,
  workspace_id text not null references workspaces(id) on delete cascade,
  data_source_id text references data_sources(id) on delete set null,
  sync_type text not null,
  status text not null,
  cursor text,
  started_at timestamptz not null default now(),
  finished_at timestamptz,
  records_seen bigint not null default 0,
  records_inserted bigint not null default 0,
  error text
);

create table if not exists jobs (
  id text primary key,
  queue text not null default 'default',
  job_type text not null,
  payload jsonb not null,
  status text not null default 'queued',
  run_after timestamptz not null default now(),
  attempts int not null default 0,
  max_attempts int not null default 5,
  locked_at timestamptz,
  locked_by text,
  last_error text,
  created_at timestamptz not null default now()
);

create index if not exists jobs_claim_idx on jobs (queue, status, run_after);
