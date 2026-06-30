# Revtern

Open-source revenue dashboard for indie app developers.

Revtern is a self-hosted purchase and revenue data hub for apps. It connects
store and billing data sources such as App Store, Google Play, RevenueCat,
Stripe, and Paddle, normalizes purchase events into a shared model, and gives
developers a calm dashboard for revenue, subscriptions, refunds, churn, and
reconciliation.

## Positioning

Revtern is not a subscription SDK and does not manage end-user entitlements.
It is a neutral analytics layer for developers who already sell through app
stores or billing platforms and want one place to understand what happened.

Core principles:

- Self-host first.
- Open-source and inspectable.
- Data-source neutral.
- Raw events are retained and traceable.
- Metrics must explain their source and calculation.
- No required client SDK for the first version.

## Implemented Stack

- Backend: Rust, Axum, Tokio, SQLx.
- Database: Postgres.
- Web frontend: React, TypeScript, Vite.
- Future mobile app: React Native, sharing API client, types, and business
  logic with the web app where practical.
- Deployment: Docker Compose first, Kubernetes later if needed.

## Documentation

- [Product Design](docs/product-design.md)
- [Product Review](docs/product-review.md)
- [User Workflows](docs/user-workflows.md)
- [Architecture](docs/architecture.md)
- [Connectors](docs/connectors.md)
- [API Design](docs/api.md)
- [Product Mapping](docs/product-mapping.md)
- [Purchase Types](docs/purchase-types.md)
- [Data Model](docs/data-model.md)
- [Authentication](docs/authentication.md)
- [Roadmap](docs/roadmap.md)

## MVP Scope

The first useful version should support:

- Single self-hosted workspace.
- Minimal owner login.
- App Store webhook ingestion.
- Google Play RTDN webhook ingestion.
- Missed-webhook catch-up for provider notification history/backlog.
- RevenueCat webhook ingestion.
- Normalized purchase event stream.
- Basic revenue, refund, subscription, and churn dashboard.
- Raw event viewer for debugging and reconciliation.

## Current Connector Status

- RevenueCat webhook: usable for raw ingest, source-product discovery,
  normalization, transactions, subscriptions, and overview metrics.
- Custom API webhook: usable for custom backend purchase/refund/renewal events.
- App Store Server Notifications V2: accepts `signedPayload`, decodes the
  notification JWS and nested transaction/renewal JWS payloads, stores the raw
  source payload unchanged, and projects decoded lifecycle events. Apple
  certificate-chain validation is still hardening work. Missed notification
  catch-up pulls App Store notification history and stores the returned
  `signedPayload` as the same raw webhook shape.
- Google Play RTDN: accepts Cloud Pub/Sub push messages, decodes the base64
  `DeveloperNotification`, stores the raw source payload unchanged, and
  projects lifecycle events from the webhook payload. Missed RTDN catch-up pulls
  retained Pub/Sub messages only; it does not query Play purchase status.

## Non-Goals

- Replacing RevenueCat, Adapty, Apphud, or Qonversion as purchase SDKs.
- Managing end-user access or entitlements.
- Full enterprise RBAC in the first release.
- Data warehouse scale before the product model is proven.

## Running Locally

The easiest full-stack path is Docker Compose:

```bash
cp .env.example .env
docker compose up --build
```

Then open:

```text
http://localhost:3000
```

The same container serves the web app and API. API routes are available under:

```text
http://localhost:3000/api
```

For local development without Docker, start Postgres yourself and set
`DATABASE_URL`. To run the built web app from the API server:

```bash
npm run build -w @revtern/web
REVTERN_WEB_DIST=apps/web/dist REVTERN_BIND=127.0.0.1:3000 cargo run -p revtern-api
```

For Vite hot reload, run the API and web dev server separately:

```bash
REVTERN_BIND=127.0.0.1:3001 cargo run -p revtern-api
VITE_API_BASE_URL=http://localhost:3001 npm run dev:web
```

Useful checks:

```bash
cargo check
npm run typecheck -w @revtern/web
npm run build -w @revtern/web
```

After first-run setup, the Overview page includes a demo seed action so the
dashboard, product mapping, transactions, events, and reconciliation screens can
be exercised before real webhooks arrive.

## Repository Shape

Current layout:

```text
revtern/
  apps/
    web/                 React dashboard
  crates/
    api/                 Axum HTTP server
    core/                Domain model and business logic
    connectors/          App Store, Google Play, RevenueCat, Stripe, Paddle
    jobs/                Background normalization and rollup workers
  packages/
    api-client/          Shared TypeScript API client
    types/               Shared generated or handwritten TS types
  docs/
  deploy/
    docker-compose.yml
```
