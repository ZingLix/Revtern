# Revtern Data Model

## Modeling Principles

- Raw source data is immutable.
- Normalized data is derived and rebuildable.
- Metrics are derived from normalized webhook facts.
- Source-specific ids are always preserved.
- Money is stored as integer minor units plus currency.
- Decimal values are used only where source APIs require them.
- Every derived row should link back to source evidence.

## Core Entities

### users

Local or federated Revtern users. `password_hash` is nullable for OIDC-only and
reverse-proxy accounts.

Important fields:

- `id`
- `email`
- `password_hash`
- `display_name`
- `role`
- `status`
- `created_at`
- `last_login_at`

The user-level role describes the account's primary personal workspace. App
authorization is derived separately from ownership and app memberships.

### auth_providers and auth_identities

`auth_providers` stores non-secret OIDC provider metadata. Client secrets remain
deployment configuration. `auth_identities` binds a stable `(provider_id,
subject)` pair to a user and stores the latest verified claims for audit and
display.

### workspaces

A logical ownership container. Each normal user receives a personal workspace;
accepting an app invitation also creates a guest membership in the owner's
workspace without granting access to its other apps.

Important fields:

- `id`
- `name`
- `created_at`

### apps

Developer apps tracked by Revtern.

Important fields:

- `id`
- `workspace_id`
- `owner_user_id`
- `created_by_user_id`
- `name`
- `platform_bundle_id`
- `apple_bundle_id`
- `google_package_name`
- `created_at`

One Revtern app may map to both iOS and Android identifiers when it is the same
product across platforms.

### app_roles, app_role_permissions, and app_memberships

Roles group app capabilities. `app_memberships` grants one role to one user for
one app. Owners and active workspace administrators receive all app
capabilities through the effective permissions view.

### app_invitations

Email-bound, expiring invitations store only a token hash. Acceptance verifies
the account email and creates the explicit app membership atomically.

### audit_events

Append-only security and administration events scoped to a user, workspace,
and app where applicable.

### data_sources

Connected source systems.

Types:

- `app_store`
- `google_play`
- `revenuecat`
- `stripe`
- `paddle`
- `csv`
- `custom_api`

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `source_type`
- `name`
- `status`
- `encrypted_credentials`
- `webhook_secret_hash`
- `last_event_at`
- `last_sync_at` timestamp of the latest source test or webhook catch-up
- `created_at`

### source_apps

Maps external source app identifiers to Revtern apps.

Important fields:

- `id`
- `data_source_id`
- `app_id`
- `external_app_id`
- `external_bundle_id`
- `external_package_name`

### logical_products

User-facing products used for dashboard aggregation.

A logical product is the thing the developer thinks about, such as `Pro
Monthly`, `Pro Annual`, or `Lifetime`. It can include many source-specific
products from App Store, Google Play, RevenueCat, Stripe, Paddle, CSV imports,
or a custom API.

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `display_name`
- `product_kind`
- `billing_period`
- `reporting_category`
- `active`
- `created_from`
- `created_by_user_id`
- `created_at`

`product_kind` examples:

- `subscription`
- `consumable`
- `non_consumable`
- `lifetime`
- `unknown`

`billing_period` examples:

- `weekly`
- `monthly`
- `annual`
- `lifetime`
- `none`

`lifetime` can be represented either as `product_kind=lifetime` or as
`product_kind=non_consumable` with `billing_period=lifetime`. The preferred
Revtern model is `product_kind=lifetime` because lifetime purchases are common
enough in indie apps to deserve first-class reporting.

`logical_products` should be created only through a user-confirmed catalog
draft. Connectors and background jobs should not silently create logical
products.

### source_products

Source-specific sellable products or prices.

Examples:

- App Store product id: `com.example.app.pro.monthly`.
- Google Play product id plus base plan id.
- RevenueCat store product identifier.
- Stripe price id.
- Paddle price id.

Important fields:

- `id`
- `workspace_id`
- `data_source_id`
- `app_id`
- `source_type`
- `platform`
- `external_product_id`
- `external_base_plan_id`
- `external_offer_id`
- `external_price_id`
- `display_name`
- `product_kind`
- `billing_period`
- `amount_minor`
- `currency`
- `raw_metadata`
- `mapping_state`
- `ignored_at`
- `ignored_by_user_id`
- `first_seen_at`
- `last_seen_at`

`mapping_state` examples:

- `unmapped`
- `mapped`
- `ignored`

### product_mappings

Confirmed links from source-specific products to logical products.

Mappings are separate from `source_products` so Revtern can keep mapping audit
history and future effective-date support.

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `source_product_id`
- `logical_product_id`
- `mapping_method`
- `confidence`
- `created_by_user_id`
- `created_at`
- `confirmed_at`
- `active`

`mapping_method` examples:

- `user_confirmed_catalog_draft`
- `user_confirmed_exact_sku`
- `user_confirmed_revenuecat_entitlement`
- `user_confirmed_same_period_price`
- `custom_api_hint`

Only active mappings should be used for dashboard aggregation.

Revtern should not store suggested mappings as durable database rows. Suggested
grouping belongs to the frontend catalog draft. The backend persists the result
only after the user confirms it.

### raw_events

Immutable source records.

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `data_source_id`
- `source_type`
- `source_event_id`
- `source_event_type`
- `environment`
- `source_app_id`
- `source_product_id`
- `occurred_at`
- `received_at`
- `payload`
- `payload_sha256`
- `signature_verified`
- `processing_status`
- `processing_error`

`environment` is one of `production`, `sandbox`, `test`, or `unknown`.
Production metrics count only `production`. `unknown` means Revtern received the
event but could not verify whether the underlying purchase is real or test.

Indexes:

- unique `(data_source_id, source_event_id)` where source ids are stable.
- `(app_id, occurred_at)`.
- `(app_id, source_type, source_event_type)`.
- `(app_id, environment, occurred_at)`.
- `(payload_sha256)` for dedupe support.

### normalized_events

Source-independent purchase lifecycle events.

Important fields:

- `id`
- `workspace_id`
- `raw_event_id`
- `data_source_id`
- `app_id`
- `source_product_id`
- `logical_product_id`
- `event_type`
- `environment`
- `platform`
- `customer_key`
- `transaction_key`
- `original_transaction_key`
- `subscription_key`
- `amount_minor`
- `currency`
- `country`
- `occurred_at`
- `normalization_version`
- `confidence`
- `warnings`

`event_type` examples:

- `purchase`
- `one_time_purchase`
- `trial_started`
- `trial_converted`
- `renewal`
- `cancellation`
- `expiration`
- `refund`
- `partial_refund`
- `revocation`
- `consumption`
- `billing_issue`
- `grace_period_started`
- `grace_period_ended`
- `reactivation`
- `product_change`

### customers

Best-effort customer identities.

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `app_user_id`
- `apple_app_account_token`
- `google_obfuscated_account_id`
- `revenuecat_app_user_id`
- `first_seen_at`
- `last_seen_at`

For the first version, customer records may be incomplete because store data may
not expose stable end-user identity unless the developer configured it.

### transactions

Purchase facts.

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `source_product_id`
- `logical_product_id`
- `customer_id`
- `platform`
- `transaction_key`
- `original_transaction_key`
- `source_type`
- `environment`
- `purchase_time`
- `amount_minor`
- `currency`
- `country`
- `status`
- `source_status`
- `status_reason`
- `status_updated_at`
- `refunded_at`
- `refund_amount_minor`
- `created_from_event_id`
- `latest_event_id`
- `updated_at`

`status` examples:

- `pending`
- `paid`
- `renewed`
- `failed`
- `refunded`
- `partially_refunded`
- `revoked`
- `disputed`
- `charged_back`
- `unknown`

The Transactions screen should expose status, source status, latest event, and
raw evidence links so the user can inspect every order lifecycle.

### subscriptions

Current subscription projection.

Important fields:

- `id`
- `workspace_id`
- `app_id`
- `source_product_id`
- `logical_product_id`
- `customer_id`
- `platform`
- `subscription_key`
- `original_transaction_key`
- `environment`
- `status`
- `started_at`
- `current_period_start`
- `current_period_end`
- `cancelled_at`
- `expired_at`
- `will_renew`
- `in_grace_period`
- `in_billing_retry`
- `latest_transaction_id`
- `status_updated_at` provider event time used to reject stale out-of-order state
  transitions
- `updated_at`

`status` examples:

- `trialing`
- `active`
- `cancelled_active`
- `grace_period`
- `billing_retry`
- `expired`
- `refunded`
- `unknown`

### daily_metrics

Precomputed metrics for dashboard speed.

Normalization updates a daily rollup only when the normalized event is first
inserted. Retrying the same raw-event job therefore cannot increment revenue or
counts twice. User-facing metric queries are derived from the normalized event
ledger so refunds retain their original gross sale and every number can drill
back to source evidence.

Dimensions:

- `workspace_id`
- `date`
- `app_id`
- `platform`
- `logical_product_id`
- `country`
- `currency`
- `source_type`

Measures:

- `gross_revenue_minor`
- `estimated_proceeds_minor`
- `refund_amount_minor`
- `net_revenue_minor`
- `purchase_count`
- `renewal_count`
- `new_subscription_count`
- `active_subscription_count`
- `cancel_count`
- `expiration_count`
- `refund_count`
- `trial_start_count`
- `trial_conversion_count`

### sync_runs

Tracks source test and missed-webhook catch-up history. The table name is kept
from the initial schema, but Revtern does not run report imports, purchase
status pulls, or historical reconstruction outside provider notification
history/backlog.

Important fields:

- `id`
- `workspace_id`
- `data_source_id`
- `sync_type`
- `status`
- `cursor`
- `started_at`
- `finished_at`
- `records_seen`
- `records_inserted`
- `error`

### jobs

Postgres-backed background job queue.

Important fields:

- `id`
- `queue`
- `job_type`
- `payload`
- `status`
- `run_after`
- `attempts`
- `max_attempts`
- `locked_at`
- `locked_by`
- `last_error`
- `created_at`

## Event Idempotency

Different sources provide different guarantees.

Use layered idempotency:

1. Source event id, when stable.
2. Source transaction id and notification type.
3. Payload hash.
4. Source-specific semantic key.

Webhook handlers should never assume exactly-once delivery.

## Currency Handling

MVP:

- Store source currency exactly.
- Aggregate only within the same currency by default.
- Mark cross-currency totals as unavailable unless exchange rates are loaded.

Later:

- Add `exchange_rates`.
- Add reporting currency per workspace.
- Store converted amounts with rate source and rate date.

## Metric Calculation

Metrics should be versioned.

Example:

- `mrr_v1`: sum active subscription monthly normalized amount.
- `gross_revenue_v1`: sum purchase and renewal transaction amounts before
  refunds.
- `net_revenue_v1`: gross revenue minus refunds, not necessarily store payout.

Metric versions let Revtern improve definitions without silently changing old
historical numbers.
