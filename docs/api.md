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
/api/registration
/api/session
/api/auth
/api/invitations
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
  "auth_mode": "local",
  "registration_mode": "invite_only",
  "oidc": null
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

### GET /api/auth/providers

Returns enabled local/OIDC sign-in methods and the registration mode.

### GET /api/auth/oidc/start

Starts OIDC Authorization Code + PKCE login. Optional `return_to` must be a
local path. An optional `invite_token` carries an app invitation through first
sign-in.

### GET /api/auth/oidc/link

Starts explicit OIDC linking for the authenticated account.

### GET /api/auth/identities

Lists sign-in methods linked to the current account.

### POST /api/registration

Creates a local account when registration policy permits it. In `invite_only`
mode, `invite_token` is required and is accepted in the same transaction.

### GET /api/invitations/{token}

Returns the public invitation preview without granting app access.

### POST /api/invitations/{token}

Accepts an invitation for the authenticated account whose normalized email
matches the invitation.

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

### POST /api/mobile/session

Creates a 30-day bearer session for an iOS or Android device. This endpoint is
available in `local` auth mode and returns an opaque access token. The
server stores only its hash.

Request:

```json
{
  "email": "dev@example.com",
  "password": "password"
}
```

Response:

```json
{
  "logged_in": true,
  "access_token": "opaque-token",
  "token_type": "Bearer",
  "expires_in": 2592000
}
```

Authenticated mobile requests send `Authorization: Bearer <access_token>`.

### DELETE /api/mobile/session

Revokes the bearer session used for the request.

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

Lists only apps the current user can access. Each record includes `role` and the
effective `permissions` array.

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

### GET /api/apps/{app_id}/members

Lists app members, pending invitations, and assignable roles. Requires
`members.manage`.

### POST /api/apps/{app_id}/invitations

Creates or replaces a pending email-bound invitation. The plaintext invitation
URL is returned once; only its hash is stored.

### PATCH /api/apps/{app_id}/members/{user_id}

Changes an explicit app member role.

### DELETE /api/apps/{app_id}/members/{user_id}

Removes explicit shared access.

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

Webhook secrets are stored as hashes. Remaining credential fields are encrypted.
App Store live ingestion requires `bundle_id`, `environment`, and
`app_apple_id` for production; Apple root certificates are bundled by the
server. Google Pub/Sub OIDC uses
`pubsub_oidc_audience` and `pubsub_service_account_email`. API keys and service
account fields additionally enable notification catch-up and Play purchase
lookup.

### PATCH /api/data-sources/{source_id}/credentials

Saves or replaces verification, lookup, and catch-up credentials for an existing source. The same
`webhook_secret`, `authorization`, and `shared_secret` fields are treated as
webhook secrets and stored as hashes; remaining fields are encrypted as catch-up
credentials. Send the complete credential object when updating; encrypted fields
are replaced as one unit.

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

### POST /api/data-sources/{source_id}/app-store-test-notification

Uses the encrypted In-App Purchase key on an App Store source to generate a
short-lived JWT and ask Apple to send a signed `TEST` notification to the URL
configured for the selected environment.

Request:

```json
{
  "environment": "sandbox"
}
```

`environment` must be `sandbox` or `production` and must be enabled on the
source. The response includes Apple's `test_notification_token`; delivery is
confirmed separately when the webhook arrives.

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
- Catch-up itself does not import purchase history, sales reports, or finance
  reports. After a Google RTDN is recovered, the normal Android Publisher lookup
  may verify that notification's purchase environment, order id, and amount.

## Webhooks

Webhook endpoints should not require browser auth. They use source-specific
verification.

App Store and Google Play reject unverifiable pushes with `401`. Other webhook
sources reject a bad secret when a shared secret is configured and expose
unsigned events as unverified when no provider-specific verifier exists.

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
- `environment`
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
- `environment`
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
- `environment`
- `logical_product_id`
- `source_product_id`
- `country`

### GET /api/subscriptions/{subscription_id}

Returns subscription timeline.

## Metrics

Revenue and subscription metrics count only rows with
`environment = "production"`. Sandbox, test, and unverified `unknown` purchases
remain visible in event, transaction, and subscription APIs but are excluded
from production revenue metrics.

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
