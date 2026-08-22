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
- Multi-user app ownership and explicit per-app sharing.

## Implemented Stack

- Backend: Rust, Axum, Tokio, SQLx.
- Database: Postgres.
- Web frontend: React, TypeScript, Vite.
- Mobile app: Expo SDK 57 and React Native, sharing the API client, types, and
  business logic with the web app where practical.
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

- Local multi-user accounts and OIDC login.
- Per-user apps with Viewer, Analyst, Editor, and Manager sharing roles.
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
  source payload unchanged, and projects decoded lifecycle events. Incoming
  JWS signatures and their `x5c` certificate chains are verified against the
  bundled Apple root certificates before storage; bundle id, environment, and
  production app Apple id claims are also checked. Configured In-App Purchase
  keys enable one-click Sandbox or Production test notifications, and missed
  notification catch-up uses the same verification path.
- Google Play RTDN: accepts Cloud Pub/Sub push messages, decodes the base64
  `DeveloperNotification`, stores the raw source payload unchanged, and
  projects lifecycle events from the webhook payload. Pub/Sub push OIDC validates
  the configured audience and service-account email. Android Publisher lookup
  classifies test purchases and supplies the per-renewal order id when available.
  Missed RTDN catch-up pulls retained Pub/Sub messages only.

## Non-Goals

- Replacing RevenueCat, Adapty, Apphud, or Qonversion as purchase SDKs.
- Managing end-user access or entitlements.
- SCIM provisioning and organization policy management.
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
npm run dev:web
```

To run the iOS/Android companion app after starting the API:

```bash
npm run dev:mobile
```

The Vite server proxies `/api` and `/webhooks` to `http://127.0.0.1:3001`
by default. Set `VITE_API_BASE_URL` only when the API is running elsewhere.

Useful checks:

```bash
cargo check
npm run typecheck -w @revtern/web
npm run build -w @revtern/web
```

`/healthz` is the process liveness endpoint. `/readyz` also checks Postgres and
is used by the container health check.

After first-run setup, the Overview page includes a demo seed action so the
dashboard, product mapping, transactions, events, and reconciliation screens can
be exercised before real webhooks arrive.

App Store events are classified as production or sandbox from Apple payload
environment fields. Google Play RTDN uses the same webhook for test and
production purchases, so Revtern receives RTDNs without extra configuration but
needs Android Publisher API access on the configured service account to verify
each purchase token as production or test. Unverified Google purchases are
marked `unknown` and excluded from production revenue metrics.

## Repository Shape

Current layout:

```text
revtern/
  apps/
    web/                 React dashboard
    mobile/              Expo iOS/Android companion app
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

## Contributing and Security

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Please
report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## License

Revtern, including the server, web dashboard, shared packages, and mobile app,
is licensed under the [Apache License 2.0](LICENSE). Official mobile builds may
be sold through app stores; the source code remains available under the same
license. Third-party components remain subject to their respective licenses.
The license does not grant permission to present unofficial builds as official
Revtern products or to use Revtern branding beyond customary attribution.
