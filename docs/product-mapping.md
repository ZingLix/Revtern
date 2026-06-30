# Product Mapping

## Problem

The same product can appear under different identifiers in different systems.

Example:

```text
Logical product: Pro Monthly

App Store:   com.example.app.pro.monthly
Google Play: pro_monthly + base_plan_monthly
RevenueCat:  pro_monthly, entitlement=pro
Stripe:      price_123
Paddle:      pri_abc
CSV:         Pro - Monthly
```

Revtern needs to aggregate these as one product without losing the original
source identities.

## Core Idea

Use two layers:

```text
logical_products
  The product the developer wants to see in reports.

source_products
  The exact product, SKU, base plan, offer, price, or item from one source.

product_mappings
  The user-confirmed link between them.
```

Dashboard queries aggregate by `logical_product_id`. Debugging and
reconciliation can still drill down to `source_product_id` and raw payloads.

## Example

```text
logical_products
  id: lp_pro_monthly
  name: Pro Monthly
  kind: subscription
  billing_period: monthly

source_products
  sp_ios_pro_monthly
    source: app_store
    external_product_id: com.example.app.pro.monthly

  sp_android_pro_monthly
    source: google_play
    external_product_id: pro_monthly
    external_base_plan_id: monthly

  sp_revenuecat_pro_monthly
    source: revenuecat
    external_product_id: com.example.app.pro.monthly
    raw_metadata.entitlement_id: pro

  sp_stripe_pro_monthly
    source: stripe
    external_price_id: price_123

product_mappings
  sp_ios_pro_monthly      -> lp_pro_monthly
  sp_android_pro_monthly  -> lp_pro_monthly
  sp_revenuecat_pro_monthly -> lp_pro_monthly
  sp_stripe_pro_monthly   -> lp_pro_monthly
```

## Why Not Match by Name Only?

Name matching is unsafe.

Bad examples:

- `pro_monthly` in two different apps may be unrelated.
- A Google Play offer id can represent a discount, not a separate product.
- A Stripe product can contain many prices.
- RevenueCat entitlement `pro` may include monthly, annual, and lifetime SKUs.
- App Store product ids often include bundle prefixes that differ by platform.

Revtern can suggest mappings, but the user should confirm them before they
affect official dashboard totals.

## Source Identity Rules

Each connector should extract a stable source product key.

### App Store

Recommended source product key:

```text
app_store:{bundle_id}:{product_id}
```

Relevant fields:

- `bundle_id`
- `product_id`

Offers and introductory pricing should be stored as event dimensions, not as a
different logical product by default.

### Google Play

Recommended source product key:

```text
google_play:{package_name}:{product_id}:{base_plan_id}
```

Relevant fields:

- `package_name`
- `product_id`
- `base_plan_id`
- `offer_id`

`offer_id` should usually be a pricing or acquisition dimension, not the core
product identity. If a developer wants offers reported as separate products,
Revtern can allow that as an advanced mapping option.

### RevenueCat

Recommended source product key:

```text
revenuecat:{project_or_app_id}:{store}:{product_identifier}
```

Relevant fields:

- `store`
- `product_identifier`
- `entitlement_ids`
- `offering_id`
- `package_id`

RevenueCat entitlements are useful hints, but they are not always products.
For example, `pro` can include monthly, annual, and lifetime products.

### Stripe

Recommended source product key:

```text
stripe:{account_id}:{price_id}
```

Relevant fields:

- `product_id`
- `price_id`
- `recurring.interval`
- `currency`
- `unit_amount`

Stripe `product_id` is closer to a product family. Stripe `price_id` is usually
the source product that maps to a Revtern logical product.

### Paddle

Recommended source product key:

```text
paddle:{account_id}:{price_id}
```

Relevant fields:

- `product_id`
- `price_id`
- `billing_cycle`
- `currency`
- `unit_price`

## Single Creation Path

Logical products have one creation path:

```text
source products are discovered
  -> frontend builds a catalog draft
  -> user reviews and edits the draft
  -> user confirms
  -> backend creates logical_products and product_mappings
```

Revtern should not have a separate blank "create product" flow. It should also
not let connectors silently create logical products in the background.

This keeps product creation understandable: every logical product exists because
the user confirmed a proposed catalog.

## Mapping Workflow

### Source Product Discovery

When Revtern sees a new source product:

1. Create `source_products` if it does not exist.
2. Leave it `unmapped` unless an active mapping already exists.
3. Show it in `Sources -> Product Mapping`.
4. Let the frontend include it in the next catalog draft.

### Frontend Catalog Draft

The frontend should show unmapped source products grouped by app, likely
product, kind, period, amount, and source.

The draft can propose:

- New logical products.
- Source products linked to those logical products.
- Source products linked to existing logical products.
- Source products ignored for dashboard aggregation.
- Source products split by base plan or offer if needed.

The draft is client-side state until the user confirms it. The backend should
not persist suggestions before confirmation.

### User Confirmation

On confirmation, the UI submits one batch:

- Logical products to create.
- Existing logical products to update if the user edited names or categories.
- Source-product-to-logical-product mappings to create.
- Source products to ignore.

The backend validates the batch and then creates the durable catalog records.

### Dashboard Behavior

Default dashboard totals should use:

- Active product mappings confirmed by the user.

Unmapped source products should still appear in:

- Event log.
- Transactions.
- Source health warnings.
- An `Unmapped` dashboard bucket if needed.

## Draft Generation Heuristics

The frontend can suggest draft groupings using:

- Exact normalized SKU match.
- Same app, same period, same amount, same currency.
- RevenueCat product identifier matching App Store or Google product id.
- RevenueCat entitlement plus period.
- Custom API `logical_product_key`.
- User-defined aliases saved after prior confirmations.

Draft suggestions should include an explanation:

```text
Suggested because both products are monthly subscriptions with SKU pro_monthly.
```

These suggestions do not affect metrics until the user confirms them.

## Logical Product vs Entitlement

These are different concepts.

```text
Entitlement: Pro access
Logical products:
  Pro Monthly
  Pro Annual
  Pro Lifetime
```

For revenue reporting, Revtern usually aggregates by logical product. For
access-style reporting, Revtern can later add `entitlements` or `product_groups`.

MVP should not model entitlement as the primary product because it would merge
monthly and annual revenue into a bucket that is too broad for many dashboard
questions.

## Product Families

Later, Revtern can add product families:

```text
Product family: Pro
  Logical product: Pro Monthly
  Logical product: Pro Annual
  Logical product: Pro Lifetime
```

This lets the UI answer both:

- How is Pro doing overall?
- How is Pro Monthly doing versus Pro Annual?

For MVP, `reporting_category` on `logical_products` is enough.

## Reconciliation

Product mapping affects reconciliation.

Revtern should flag:

- Source product has no logical product.
- Two source products may represent the same logical product.
- One source product is mapped to a logical product with a conflicting billing
  period.
- Direct store events and RevenueCat events may double-count the same purchase.

The raw source product identity should always remain visible.

## Recommended MVP

Build the first version with:

- `logical_products`.
- `source_products`.
- `product_mappings`.
- Frontend-generated catalog draft.
- User-confirmed batch creation.
- Safe suggestions, not silent merging.
- `Unmapped` bucket in dashboards.

Avoid advanced catalog modeling until real data shows where the edge cases are.
