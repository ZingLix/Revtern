# Purchase Types

## Goal

Revtern must support subscriptions and one-time purchases from the beginning.
It should not be modeled as a subscription-only product.

The product model should cover:

- Auto-renewing subscriptions.
- Non-renewing subscriptions where sources expose them.
- Consumable in-app purchases.
- Non-consumable in-app purchases.
- Lifetime unlocks.
- Web purchases through Stripe or Paddle.
- Refunds, revocations, and chargebacks.

## Product Kinds

### subscription

A recurring product with renewal periods.

Examples:

- Pro Monthly.
- Pro Annual.
- Team Monthly.

Important lifecycle events:

- trial started.
- purchase.
- renewal.
- cancellation.
- expiration.
- billing issue.
- grace period.
- refund.
- product change.
- reactivation.

Dashboard metrics:

- Active subscriptions.
- MRR.
- ARR.
- New subscriptions.
- Renewals.
- Cancellations.
- Expirations.
- Churn.
- Trial conversion.

### consumable

A product that can be bought multiple times and consumed.

Examples:

- Coin pack.
- Credit pack.
- Extra export credits.
- AI token bundle.

Important lifecycle events:

- purchase.
- refund.
- partial refund where source supports it.
- consumption if developer sends custom events.

Dashboard metrics:

- One-time revenue.
- Units sold.
- Average order value.
- Refund rate.
- Revenue by pack.

Revtern should not try to track remaining consumable balance in MVP unless the
developer sends custom inventory events. That is product entitlement logic, not
revenue analytics.

### non_consumable

A durable one-time unlock.

Examples:

- Remove ads.
- Extra theme pack.
- One-time feature unlock.

Important lifecycle events:

- purchase.
- refund.
- revocation.

Dashboard metrics:

- One-time revenue.
- Units sold.
- Refund rate.
- Attach rate by app or country later.

### lifetime

A common indie-app pattern: pay once for permanent Pro access.

Examples:

- Lifetime Pro.
- Lifetime Premium.

This can be considered a non-consumable purchase, but Revtern should model it
as first-class because it is important for revenue analysis.

Dashboard metrics:

- Lifetime revenue.
- Lifetime purchase count.
- Lifetime versus subscription mix.
- Refund rate.
- Implied payback comparisons later.

### unknown

Used when Revtern sees a product before it can classify it.

Unknown products should appear in setup and reconciliation warnings so the user
can classify them.

## Event Types

Core normalized event types:

```text
purchase
one_time_purchase
trial_started
trial_converted
renewal
cancellation
expiration
refund
partial_refund
revocation
billing_issue
grace_period_started
grace_period_ended
reactivation
product_change
consumption
```

`purchase` can be generic. `one_time_purchase` is useful when the connector can
clearly identify non-recurring purchases.

## Fact Tables

Revtern should keep two related projections:

```text
transactions
  All money-moving purchase facts.

subscriptions
  Only recurring subscription state.
```

This prevents the data model from forcing one-time purchases into a subscription
shape.

Every paid event should usually create or update a `transactions` row. Only
subscription lifecycle events should create or update `subscriptions`.

## Revenue Dashboard Grouping

Revenue views should separate:

- Subscription revenue.
- One-time purchase revenue.
- Lifetime revenue.
- Consumable revenue.
- Refunds.

The dashboard should also allow an all-revenue total.

## Product Mapping Implications

Logical products should preserve product kind.

Bad mapping:

```text
App Store Lifetime Pro -> Pro Monthly
```

Revtern should warn if a user maps source products with conflicting kinds or
billing periods into one logical product.

Allowed advanced mapping:

```text
Product family: Pro
  Pro Monthly
  Pro Annual
  Pro Lifetime
```

The family can be used for broader reporting, but the individual logical
products should remain separate.

## Store Differences

### App Store

Can expose:

- Auto-renewable subscriptions.
- Non-renewing subscriptions.
- Consumables.
- Non-consumables.

### Google Play

Can expose:

- Subscriptions with base plans and offers.
- One-time products.
- Consumables and non-consumables depending on app behavior.

### RevenueCat

Can forward subscription and non-subscription purchase events. Revtern should
not assume every RevenueCat event is a subscription event.

### Stripe and Paddle

Can expose:

- Recurring subscriptions.
- One-time payments.
- Refunds.
- Chargebacks or disputes depending on source.

## MVP Requirements

The first implementation should support:

- `subscription`.
- `consumable`.
- `non_consumable`.
- `lifetime`.
- `unknown`.
- Transaction projection for all purchases.
- Subscription projection only for subscription products.
- Dashboard split between subscription and one-time revenue.
- Mapping warnings for conflicting product kinds.

