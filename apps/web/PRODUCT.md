# Product

## Register

product

## Users

Revtern is for independent app developers and very small app studios who sell
subscriptions, in-app purchases, or app-adjacent products through App Store,
Google Play, RevenueCat, Stripe, Paddle, or custom backends. They are usually
working alone or in a small team, often after shipping or monitoring a release,
and need to understand revenue without building an internal data pipeline.

Their job is to connect trusted purchase sources, confirm product mappings,
watch revenue and subscription movement, inspect the events behind a number,
and fix data quality issues before they distort decisions.

## Product Purpose

Revtern is an open-source, self-hosted revenue data hub. It ingests purchase
and billing events, keeps raw source payloads traceable, normalizes them into a
shared model, and turns them into dashboards, ledgers, source health checks,
and reconciliation views.

Success means a developer can answer: what did my apps earn, what changed, and
can I trust the number? The product should make the answer fast to read,
rewarding to revisit, and grounded in source evidence.

## Brand Personality

Precise, trustworthy, rewarding.

Revtern should feel like a serious operations tool that still lets developers
feel the accomplishment of building a business. Data-heavy pages may be bold,
energetic, and visually satisfying when they reveal progress, clean setup, or
healthy revenue, but the interface must never make financial uncertainty feel
more certain than it is.

## Anti-references

Avoid marketing-style dashboards, huge hero sections inside the app, generic
SaaS metric-card grids, decorative card clutter, vague "insights" without
source evidence, and casino-like gamification.

Avoid designs that hide uncertainty, imply precision the data does not have,
or make charts feel exciting at the cost of legibility. Revtern should not look
like a subscription SDK, paywall builder, entitlement service, ad attribution
suite, or hosted enterprise analytics product.

## Design Principles

Make trust visible. Every important metric should show its source confidence,
calculation status, and path back to transactions, events, or raw payloads.

Reward progress without distorting reality. The Overview and Revenue surfaces
can be cool, vivid, and satisfying when data is flowing, but state labels,
definitions, and warnings must stay clear.

Compress the path to value. Empty and setup states should move the user toward
the next concrete step: create an app, connect a source, receive an event,
confirm mappings, or review the dashboard.

Treat mapping as review, not data entry. Product catalog work should feel like
confirming Revtern's proposed understanding, with reasons and conflicts made
obvious.

Keep the ledger close to the chart. Dashboards are believable only when users
can drill into transactions, normalized events, and raw payloads without losing
context.

## Accessibility & Inclusion

Target WCAG 2.2 AA by default. Dense tables, filters, setup forms, source
configuration, drawers, and interactive charts need visible focus states,
keyboard access, readable contrast, and text labels for every semantic status.

State must never rely on color alone. Motion should respect
`prefers-reduced-motion`; celebratory or high-energy data moments need reduced
motion alternatives that preserve meaning without animation.
