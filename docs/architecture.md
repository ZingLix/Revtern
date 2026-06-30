# Revtern Architecture

## Architecture Goals

- Easy to self-host.
- Minimal moving parts for MVP.
- Strong data integrity.
- Source payloads retained exactly as received.
- Idempotent ingestion.
- Clear separation between ingestion, normalization, state building, and
  analytics.
- Good enough for small and medium app businesses before adding analytical
  infrastructure.

## Initial System

```text
          App Store / Google Play / RevenueCat / Stripe / Paddle
                                |
                                v
                         Webhook Ingestion
                                |
                                v
                            raw_events
                                |
                                v
                         normalization job
                                |
                                v
                        normalized_events
                                |
                                v
                 transaction/subscription projection
                                |
                                v
                 daily rollups and dashboard queries
                                |
                                v
                         React web dashboard
```

## Backend

Rust backend crates:

```text
crates/
  api/
    HTTP routes, auth middleware, request validation.
  core/
    Domain types, metric definitions, state transition rules.
  connectors/
    Source-specific webhook ingestion.
  jobs/
    Background workers, source tests, rollups, retries.
```

Recommended Rust libraries:

- `axum` for HTTP.
- `tokio` for async runtime.
- `sqlx` for Postgres access.
- `serde` and `serde_json` for payload handling.
- `time` for date/time.
- `uuid` for ids.
- `tracing` for logs.

## Database

Postgres is the primary store.

Why Postgres:

- Natural fit for multi-tenant app/product/source relationships.
- JSONB can store source payloads.
- Unique indexes solve webhook idempotency.
- SQL is valuable for reconciliation and ad-hoc debugging.
- Easy self-host deployment.
- Backups, migrations, and admin tools are mature.

RocksDB is not recommended as the primary store because Revtern needs
relationships, secondary indexes, aggregation, migrations, access control,
backup workflows, and operational visibility. RocksDB could later be useful for
local caches or embedded experiments, but it should not be the main database.

## Job Processing

For MVP, use Postgres-backed jobs instead of Redis:

- `jobs` table stores queued work.
- Workers claim rows with `FOR UPDATE SKIP LOCKED`.
- Failed jobs retry with exponential backoff.
- Source test runs and errors are visible in the UI.

This keeps self-host deployment simple:

```text
revtern-api
revtern-worker
postgres
```

Later, Redis, Faktory, or a dedicated queue can be added only if needed.

## Frontend

Web frontend:

- React.
- TypeScript.
- Vite.
- TanStack Query for API state.
- React Router for routing.
- Lightweight charting library for time series.
- Generated or shared API types.

The backend can serve the built SPA for simple self-hosting, or the web app can
be deployed separately.

## Future React Native App

React Native can share useful code with the web app, but should not force a
fully shared UI architecture.

Share:

- API client.
- TypeScript types.
- Metric formatting.
- Date range helpers.
- Currency formatting.
- Validation schemas if written in TypeScript.

Do not over-share initially:

- Complex web table UI.
- Dashboard layout components.
- Chart components.

Recommended future layout:

```text
apps/
  web/
  mobile/
packages/
  api-client/
  types/
  formatters/
```

The mobile app can be a companion app for quick checks, alerts, and summaries,
not a full replacement for the web dashboard.

## Data Flow

### Webhook Ingestion

1. Verify source signature where available.
2. Parse enough metadata to identify source, app, and event id.
3. Insert immutable row into `raw_events`.
4. Enqueue normalization job.
5. Return success quickly.

Ingestion must be idempotent. Repeated webhook delivery should produce one raw
event row or multiple raw delivery rows linked to one source event, depending on
source semantics.

### Source Tests And Webhook Catch-Up

1. Create a `sync_runs` row with `sync_type = 'health_check'`.
2. Check whether the source has received events and whether the latest
   processing state is healthy.
3. Mark the run as completed or failed.

For missed-webhook catch-up, create a `sync_runs` row with
`sync_type = 'webhook_catch_up'`, pull provider notification history or message
backlog, store each notification in `raw_events`, and run the same
normalization path. The table name remains `sync_runs` in the first schema, but
Revtern does not fetch purchase status, import reports, or reconstruct
historical records outside webhook-shaped notifications.

### Normalization

Normalization maps source payloads into Revtern events:

- `purchase_started`
- `trial_started`
- `trial_converted`
- `renewed`
- `cancelled`
- `expired`
- `refunded`
- `billing_issue_started`
- `billing_issue_resolved`
- `grace_period_started`
- `reactivated`
- `product_changed`

Each normalized event keeps:

- Link to raw event.
- Source event id.
- Event time.
- Processing version.
- Confidence and warnings.

### Projections

Projection jobs turn event streams into current facts:

- Transactions.
- Subscriptions.
- Customers.
- Product mappings.
- Daily metrics.

Projection should be rebuildable from raw and normalized events.

## Deployment

MVP deployment target:

```text
docker compose up
```

Services:

- `postgres`
- `revtern-api`
- `revtern-worker`
- optional `revtern-web` if the API does not serve static assets

Configuration:

- `DATABASE_URL`
- `REVTERN_BASE_URL`
- `REVTERN_AUTH_MODE`
- `REVTERN_SECRET_KEY`
- source webhook secrets

## Observability

For self-hosting, observability should be built into the app:

- Source health screen.
- Source test history screen.
- Job queue screen.
- Failed event processing screen.
- Structured logs.
- Basic Prometheus endpoint later.

## Security

- Store source credentials encrypted at rest.
- Require HTTPS in production deployment docs.
- Verify webhook signatures.
- Redact secrets from logs.
- Keep raw payload access limited to owner/admin users.
- Provide a no-telemetry default.

## Scale Plan

### Phase 1

Postgres only.

### Phase 2

Postgres partitioning by event time and workspace.

### Phase 3

Optional ClickHouse read model for very large installations.

### Phase 4

Hosted cloud can run the same core with stronger multi-tenant controls.
