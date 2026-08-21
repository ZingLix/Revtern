# Revtern Product Design

## One-Line Description

Revtern is an open-source, self-hosted revenue dashboard for indie app
developers.

## Target User

The primary user is an independent app developer or very small app studio that:

- Has one or more mobile apps.
- Sells subscriptions or in-app purchases through App Store or Google Play.
- May also use RevenueCat, Stripe, Paddle, Adapty, Apphud, or Qonversion.
- Does not want to build an internal revenue data pipeline.
- Wants to self-host because purchase and revenue data is sensitive.

The secondary user is a small studio with multiple apps and multiple billing
channels that needs a simple, trusted source of truth.

## Product Thesis

Store dashboards and subscription platforms are useful, but their data is
fragmented. A developer may need to check App Store Connect, Google Play
Console, RevenueCat, Stripe, and spreadsheets to answer a basic question:

> What did my apps actually earn, what changed, and can I trust the number?

Revtern should make that answer available in one place while preserving the
raw source records behind every metric.

## What Revtern Does

Revtern ingests purchase-related data from multiple sources:

- App Store server notifications.
- Google Play real-time developer notifications.
- RevenueCat webhooks.
- Stripe webhooks.
- Paddle webhooks.
- Custom backend webhooks.
- Later: Adapty, Apphud, Qonversion.

It then:

- Stores raw events unchanged.
- Normalizes source-specific events into a shared event model.
- Builds transaction and subscription state.
- Rolls up metrics by day, app, platform, product, country, and currency.
- Shows dashboards and drill-down views.
- Surfaces reconciliation problems.

## Product Boundaries

### In Scope

- Revenue dashboard.
- Subscription dashboard.
- Product catalog review and mapping.
- Refund and cancellation tracking.
- Raw event and normalized event inspection.
- Source connection status.
- Source test history.
- Missed-webhook catch-up history.
- Metric definitions.
- Reconciliation views.
- CSV export.
- API access for local automation.

### Out of Scope for MVP

- End-user entitlement checks.
- Client purchase SDK.
- Paywall builder.
- A/B testing.
- Customer messaging.
- Ad attribution.
- Advanced forecasting.
- Full accounting and tax compliance.

## Core Screens

### Overview

Shows a compact summary:

- Gross revenue.
- Estimated proceeds.
- Active subscriptions.
- New subscriptions.
- Renewals.
- Refund amount.
- Refund rate.
- Churned subscriptions.
- Net change versus previous period.

Filters:

- Date range.
- App.
- Platform.
- Product.
- Country.
- Currency.
- Data source.

### Revenue

Revenue over time with breakdowns:

- Gross revenue.
- Store fees.
- Estimated proceeds.
- Refunds.
- Net revenue.
- One-time purchases versus subscriptions.
- Platform split.
- Product split.
- Country split.

### Subscriptions

Subscription health:

- Active subscriptions.
- Trials.
- Trial conversions.
- New subscriptions.
- Renewals.
- Cancellations.
- Expirations.
- Billing retry or grace period.
- Reactivations.
- MRR and ARR.

### Events

A searchable event log:

- Source event id.
- Source.
- Platform.
- Event type.
- App.
- Product.
- Transaction id.
- Original transaction id.
- Customer id if available.
- Event time.
- Ingested time.
- Processing state.

Each event links to:

- Raw payload.
- Normalized payload.
- Related transaction.
- Related subscription.
- Related processing job.

### Transactions

A ledger-style view of purchase facts:

- Transaction id.
- Platform.
- Product.
- Source product.
- Amount.
- Currency.
- Country.
- Customer id.
- Purchase time.
- Renewal or original transaction id when available.
- Refund time if refunded.
- Current status.
- Source confidence.

Common statuses:

- Paid.
- Renewed.
- Refunded.
- Partially refunded.
- Revoked.
- Disputed.
- Pending.
- Failed.
- Unknown.

### Products

The product catalog is created through a confirmation flow.

Revtern discovers source products from connected data sources. The frontend
groups those source products into a generated catalog draft. The user reviews
and confirms the draft. Only then does Revtern create logical products and
durable mappings.

The Products screen shows:

- Logical products.
- Source products mapped to each logical product.
- Product kind.
- Billing period.
- Reporting category.
- Unmapped source products.
- Ignored source products.
- Mapping warnings.

### Reconciliation

Shows data quality and source mismatch:

- Webhook events that failed processing.
- Duplicate source records.
- Conflicting amounts or currencies.
- Out-of-order state transitions.
- Metrics with incomplete source coverage.

### Sources

Shows connected data sources:

- App Store.
- Google Play.
- RevenueCat.
- Stripe.
- Paddle.

For each source:

- Connection status.
- Last webhook received.
- Last source test.
- Last webhook catch-up.
- Last error.
- Webhook secret status.
- Webhook URL.
- Setup checklist.

## Metric Philosophy

Every metric should answer:

- What sources contributed to this number?
- What webhook events were included?
- What currency conversion was used?
- Is this gross, net, proceeds, or estimated?
- Is this based on live webhook payloads or incomplete source coverage?

Revtern should avoid hiding uncertainty. If a number is estimated, the UI should
label it as estimated.

## MVP User Story

1. Developer starts Revtern through Docker Compose.
2. Developer creates the first owner account.
3. Developer creates an app in Revtern.
4. Developer can share that app with a role-specific invitation.
5. Developer connects RevenueCat webhook or App Store/Google source.
6. Revtern receives raw purchase events.
7. Revtern normalizes events and updates rollups.
8. Developer opens the dashboard and sees revenue, subscriptions, refunds, and
   raw event details.
9. Developer can inspect how a metric was calculated.

## Differentiation

Compared with closed hosted tools:

- Self-hostable.
- Raw data is inspectable.
- No required SDK migration.
- Works as a neutral layer above existing purchase infrastructure.
- Designed for indie developers first, not enterprise reporting teams.

Compared with RevenueCat or Adapty:

- Revtern does not own purchase flow or entitlements.
- Revtern can ingest from them as data sources.
- Revtern focuses on cross-source analytics and reconciliation.

Compared with generic BI:

- Revtern understands app-store subscription semantics.
- Revtern provides opinionated revenue metrics out of the box.
- Revtern keeps source-specific raw data linked to normalized events.
