# API Design

## API Principles

- JSON over HTTP for the MVP.
- Stable resource-oriented endpoints.
- Webhook endpoints are separated from authenticated app API endpoints.
- Every dashboard number should be drillable to source data.
- API responses should expose metric metadata, not just values.

## Route Groups

```text
/api/setup
/api/session
/api/me
/api/apps
/api/products
/api/data-sources
/api/events
/api/transactions
/api/subscriptions
/api/metrics
/api/sync-runs
/api/jobs
/webhooks
```

## Setup

### GET /api/setup/status

Returns whether first-run setup is needed.

Response:

```json
{
  "needs_setup": true,
  "auth_mode": "single_user"
}
```

### POST /api/setup/owner

Creates the first owner user and workspace.

Request:

```json
{
  "email": "dev@example.com",
  "password": "correct horse battery staple",
  "workspace_name": "Personal Apps"
}
```

## Session

### POST /api/session

Creates a session.

Request:

```json
{
  "email": "dev@example.com",
  "password": "password"
}
```

### DELETE /api/session

Logs out the current session.

### GET /api/me

Returns current user and workspace.

Response:

```json
{
  "user": {
    "id": "usr_...",
    "email": "dev@example.com",
    "role": "owner"
  },
  "workspace": {
    "id": "wsp_...",
    "name": "Personal Apps"
  }
}
```

## Apps

### GET /api/apps

Lists apps.

### POST /api/apps

Creates an app.

Request:

```json
{
  "name": "My App",
  "apple_bundle_id": "com.example.ios",
  "google_package_name": "com.example.android"
}
```

### PATCH /api/apps/{app_id}

Updates app metadata.

## Data Sources

### GET /api/data-sources

Lists connected sources and health.

### POST /api/data-sources

Creates a source.

Request:

```json
{
  "source_type": "revenuecat",
  "name": "RevenueCat Production",
  "credentials": {
    "webhook_secret": "secret"
  }
}
```

Webhook secrets are stored as hashes. Non-secret credentials are only used for
missed-webhook catch-up, such as App Store notification history or Google
Pub/Sub pull backlog.

Catch-up credentials are optional. A source can receive live webhooks without
them; catch-up is available after credentials are saved.

### PATCH /api/data-sources/{source_id}/credentials

Saves or replaces optional catch-up credentials for an existing source. The same
`webhook_secret`, `authorization`, and `shared_secret` fields are treated as
webhook secrets and stored as hashes; remaining fields are encrypted as catch-up
credentials.

## Products

### GET /api/products/logical

Lists confirmed logical products.

### GET /api/products/source

Lists discovered source products.

Filters:

- `app_id`
- `data_source_id`
- `mapping_state`
- `product_kind`

### POST /api/products/catalog-confirmations

Creates or updates the product catalog from a user-confirmed frontend draft.

Request:

```json
{
  "app_id": "app_...",
  "logical_products": [
    {
      "client_id": "draft_pro_monthly",
      "display_name": "Pro Monthly",
      "product_kind": "subscription",
      "billing_period": "monthly",
      "reporting_category": "Pro"
    }
  ],
  "mappings": [
    {
      "source_product_id": "sp_...",
      "logical_product_client_id": "draft_pro_monthly",
      "mapping_method": "user_confirmed_catalog_draft"
    }
  ],
  "ignored_source_product_ids": []
}
```

The backend validates the batch and persists logical products and mappings
atomically. The backend should not persist unconfirmed draft suggestions.

### GET /api/data-sources/{source_id}

Returns source metadata, health, setup checklist, and webhook URL.

### POST /api/data-sources/{source_id}/test

Runs a source health check.

### POST /api/data-sources/{source_id}/catch-up

Pulls missed webhook notifications only.

Request:

```json
{
  "from": "2026-06-23T00:00:00Z",
  "to": "2026-06-30T00:00:00Z",
  "limit": 100,
  "cursor": null
}
```

Behavior:

- App Store pulls notification history and stores returned `signedPayload`
  bodies.
- Google Play pulls retained Pub/Sub RTDN messages and stores Pub/Sub push-shaped
  bodies.
- No purchase status, subscription state, order detail, sales report, or finance
  report endpoint is called.

## Webhooks

Webhook endpoints should not require browser auth. They use source-specific
verification.

```text
POST /webhooks/revenuecat/{source_id}
POST /webhooks/app-store/{source_id}
POST /webhooks/google-play/{source_id}
POST /webhooks/stripe/{source_id}
POST /webhooks/paddle/{source_id}
POST /webhooks/custom/{source_id}
```

Webhook response should be fast:

```json
{
  "received": true
}
```

Processing happens asynchronously after raw event storage.

## Events

### GET /api/events/raw

Query raw events.

Filters:

- `from`
- `to`
- `source_type`
- `data_source_id`
- `app_id`
- `source_event_type`
- `processing_status`
- `q`

### GET /api/events/raw/{event_id}

Returns raw event payload and processing state.

### GET /api/events/normalized

Query normalized events.

### GET /api/events/normalized/{event_id}

Returns normalized event and links to raw source evidence.

## Transactions

### GET /api/transactions

Filters:

- `from`
- `to`
- `app_id`
- `platform`
- `logical_product_id`
- `source_product_id`
- `country`
- `currency`
- `status`
- `customer_id`

### GET /api/transactions/{transaction_id}

Returns transaction detail, current status, source status, related events, and
source payload links.

## Subscriptions

### GET /api/subscriptions

Filters:

- `status`
- `app_id`
- `platform`
- `logical_product_id`
- `source_product_id`
- `country`

### GET /api/subscriptions/{subscription_id}

Returns subscription timeline.

## Metrics

### GET /api/metrics/overview

Query:

```text
from=2026-06-01
to=2026-06-30
app_id=...
platform=ios
currency=USD
```

Response:

```json
{
  "period": {
    "from": "2026-06-01",
    "to": "2026-06-30"
  },
  "currency": "USD",
  "metrics": {
    "gross_revenue_minor": {
      "value": 124500,
      "definition": "gross_revenue_v1",
      "estimated": false
    },
    "net_revenue_minor": {
      "value": 118200,
      "definition": "net_revenue_v1",
      "estimated": true
    },
    "active_subscriptions": {
      "value": 312,
      "definition": "active_subscriptions_v1",
      "estimated": false
    }
  },
  "warnings": [
    "Net revenue is estimated from webhook payloads and may differ from store payout statements."
  ]
}
```

### GET /api/metrics/revenue-timeseries

Returns daily revenue series.

### GET /api/metrics/subscription-timeseries

Returns daily subscription health series.

### GET /api/metrics/breakdown

Breakdown by:

- `app`
- `platform`
- `product`
- `country`
- `source`

## Source Test Runs

### GET /api/sync-runs

Lists source test and webhook catch-up history. The table name is `sync_runs`
for compatibility with the initial schema, but Revtern does not start purchase
status syncs or report imports.

### GET /api/sync-runs/{sync_run_id}

Returns source test details and errors.

## Jobs

Owner-only diagnostics.

### GET /api/jobs

Lists queued, running, and failed jobs.

### POST /api/jobs/{job_id}/retry

Retries a failed job.

## API Tokens

API tokens can wait until after the local dashboard MVP. When added, tokens
should be scoped:

- read metrics.
- read events.
- write custom events.
- manage sources.

## Error Format

Use a consistent error response:

```json
{
  "error": {
    "code": "invalid_request",
    "message": "from must be before to",
    "request_id": "req_..."
  }
}
```
