# Revtern Roadmap

## Phase 0: Design

- Product design.
- Architecture design.
- Data model.
- Auth model.
- Connector strategy.
- Initial project scaffold.

## Phase 1: Local MVP

Goal: Run locally and ingest a simple event source.

Scope:

- Rust API server.
- Postgres migrations.
- First-run owner setup.
- Session login.
- React dashboard shell.
- Data source configuration model.
- RevenueCat webhook ingestion.
- Raw event storage.
- Source product discovery.
- Product catalog draft confirmation.
- Transaction projection.
- Normalization job.
- Event log UI.
- Minimal overview metric.

Why RevenueCat first:

- Easier setup than official store webhooks.
- Useful for many indie developers.
- Provides a quick validation path for the normalized event model.

## Phase 2: App Store and Google Play

Scope:

- App Store Server Notification endpoint.
- App Store notification signature verification.
- Google Play RTDN endpoint.
- Google Pub/Sub push verification strategy.
- Source setup guides in UI.
- Webhook payload coverage warnings.
- Missed-webhook catch-up for App Store notification history and Google Pub/Sub
  backlog.

## Phase 3: Metrics

Scope:

- Subscriptions projection.
- Daily metrics rollup.
- Overview dashboard.
- Revenue dashboard.
- Subscription dashboard.
- Refund dashboard.
- Date range and app/platform/product filters.

## Phase 4: Reconciliation

Scope:

- Source mismatch detection.
- Webhook duplicate detection.
- Duplicate event detection.
- Missing amount/currency warnings.
- Missed-webhook catch-up diagnostics.
- Metric drill-down to source evidence.

## Phase 5: More Sources

Scope:

- Stripe.
- Paddle.
- CSV import.
- Custom API source.
- Adapty/Apphud/Qonversion webhooks if enough users ask for them.

## Phase 6: Self-Host Polish

Scope:

- Docker Compose.
- Backup guide.
- HTTPS reverse proxy guide.
- Health checks.
- Source health UI.
- Job queue UI.
- Upgrade/migration guide.

## Phase 7: Hosted Option

Only after the self-hosted product is useful.

Scope:

- Multi-workspace.
- Billing.
- Team users.
- Stronger audit logging.
- SSO for paid plans.
- Managed webhook reliability.
