# Product Review

## Scope

This review is based on Revtern's current product documentation, not on a
running UI. It focuses on product definition, onboarding, workflow clarity,
information architecture, trust, and MVP risk.

## High-Level Assessment

Revtern has a strong product spine:

```text
self-hosted revenue data hub
  -> source-neutral ingestion
  -> user-confirmed product catalog
  -> traceable transactions
  -> dashboard and reconciliation
```

The strongest differentiation is not "another subscription dashboard." It is
"a trusted, self-hosted ledger that explains where every app revenue number came
from."

The product should lean into that. The dashboard is useful, but trust and
traceability are the sharper wedge.

## Strengths

### Clear User And Trust Problem

The target user is concrete: indie developers and small app studios with
fragmented revenue data across stores and billing tools. Self-hosting is a
credible trust advantage because the data includes revenue, credentials, and
purchase events.

### Good Product Boundary

Not managing entitlements, not requiring an SDK, and not replacing RevenueCat
is the right boundary. Revtern should be compatible with existing purchase
systems, not ask users to migrate their purchase stack.

### Product Catalog Confirmation Is The Right Model

The confirmed catalog draft flow is a strong product decision:

```text
discover source products
  -> generate draft
  -> user confirms
  -> dashboard aggregates
```

This avoids silent misaggregation, which is one of the main trust risks in a
multi-source revenue product.

### Ledger View Is Essential

The Transactions and Events screens are not secondary debug pages. They are what
make the dashboard believable. Every metric should be drillable down to
transactions, normalized events, and raw source payloads.

## UX Risks

### 1. Onboarding May Ask For Too Much Before Showing Value

The current path can become:

```text
deploy
  -> create owner
  -> create app
  -> configure source
  -> wait for event
  -> confirm catalog
  -> then see dashboard
```

That is correct, but it is a long path before the first payoff. The UI needs a
guided setup checklist with visible progress and a sample/demo mode.

Recommendation:

- Add a demo dataset option for first launch.
- Add a setup checklist: app, source, first event, catalog confirmation,
  dashboard ready.
- Show partial value as soon as raw events arrive, even before aggregation.

### 2. Product Mapping Could Become The Hardest Screen

Product mapping is structurally important, but it can feel like accounting work
if the UI is too table-heavy.

Recommendation:

- Treat mapping as a review queue, not as a blank settings table.
- Group source products into proposed cards.
- Show "why grouped" explanations.
- Highlight only conflicts that require user attention.
- Keep advanced split/ignore controls secondary.

### 3. Dashboard Trust States Need First-Class Design

Financial dashboards fail when users cannot tell whether a number is complete,
estimated, unreconciled, or missing a source.

Recommendation:

Every key metric should carry a trust state:

- `live`
- `estimated`
- `reconciled`
- `incomplete`
- `stale`
- `unmapped`

This should be visible in the UI, not hidden in docs.

### 4. "Revenue" Needs Clear Definitions

Users will ask why App Store Connect, RevenueCat, Stripe, and Revtern do not
match. This is expected. The product should make the distinction obvious:

- gross revenue
- refunds
- net revenue
- estimated proceeds
- source currency
- converted reporting currency

Recommendation:

Make metric definitions available from the first dashboard version. Do not wait
until reconciliation is built.

### 5. Empty States Are Product-Critical

The product will spend a lot of time in incomplete states:

- no app
- no source
- source connected but no events
- events received but no products confirmed
- products confirmed but webhook payloads lack amount fields
- live events available but source coverage incomplete

Recommendation:

Design empty states as workflow steps with one next action, not as generic empty
screens.

## Information Architecture Recommendation

Primary navigation should reflect the user's mental model:

```text
Overview
Revenue
Transactions
Subscriptions
Products
Sources
Reconciliation
Settings
```

`Events` can be inside Transactions or Reconciliation at first. It is important
for trust, but a raw event log may be too technical as a top-level item for
indie developers.

Recommended first version:

- `Transactions` includes linked raw and normalized events.
- `Sources` shows source health and latest events.
- `Reconciliation` later exposes deeper mismatch workflows.

## Recommended MVP Shape

The first release should prove this loop:

```text
connect RevenueCat
  -> receive event
  -> discover source product
  -> confirm catalog draft
  -> see transaction
  -> see simple dashboard metric
  -> drill back to raw event
```

Do not make the first MVP depend on direct App Store/Google API access or
report reconciliation. That is important later, but it is not needed to prove
the core interaction.

Minimum screens:

- First-run setup.
- App setup.
- Source setup for RevenueCat.
- Product catalog confirmation.
- Transactions.
- Overview.
- Source health.

## Accessibility Risks To Watch Later

This cannot be verified without screenshots or implementation, but the product
has predictable accessibility risk areas:

- Dense data tables need keyboard navigation and visible focus.
- Status chips need text labels, not color alone.
- Charts need table alternatives or accessible summaries.
- Raw JSON payloads need readable wrapping, search, and copy behavior.
- Setup errors need clear recovery instructions.
- Mapping cards need accessible drag/drop alternatives if drag/drop is used.

## Product Design Direction

Revtern should feel calm, precise, and operational.

Avoid:

- Marketing-style dashboards.
- Huge hero areas inside the app.
- Decorative cards that make dense financial data harder to scan.
- Vague "insights" copy without source evidence.

Prefer:

- Compact tables.
- Clear status labels.
- Drill-down links.
- Small charts with exact numbers nearby.
- Empty states that move setup forward.
- Plain language around financial uncertainty.

## Top Recommendations

1. Make "trusted revenue ledger" the product promise, not just "dashboard."
2. Put product catalog confirmation into the Phase 1 MVP.
3. Treat every metric as having a visible trust state.
4. Build Transactions before advanced charts.
5. Make mapping a guided review queue.
6. Add demo data or sample mode to shorten time-to-value.
7. Keep RevenueCat as the first connector, then expand to store-native sources.
