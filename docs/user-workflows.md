# User Workflows

## Current Project State

Revtern currently has a first-pass Rust API and React web dashboard. The usage
below is the product workflow the implementation should support.

The first usable release should be a self-hosted web app started with Docker
Compose.

## Primary User Journey

The core user journey is:

```text
Deploy Revtern
  -> create owner account
  -> create app
  -> optionally invite Viewer, Analyst, Editor, or Manager collaborators
  -> connect a data source
  -> review generated product catalog draft
  -> confirm product catalog and mappings
  -> receive webhook purchase data
  -> review dashboard
  -> drill into events and transactions
  -> fix source or data issues
  -> use Revtern as daily revenue home
```

## 1. Deploy Revtern

The user starts with a self-host deployment.

Expected MVP flow:

```bash
git clone https://github.com/ZingLix/Revtern
cd revtern
cp .env.example .env
docker compose up -d
```

Then they open:

```text
http://localhost:3000
```

For production self-hosting, the user should put Revtern behind HTTPS through
Caddy, Nginx, Traefik, Cloudflare Tunnel, Tailscale Funnel, or another reverse
proxy.

After setup, every account owns its own apps. App managers can invite another
email without exposing other apps in the owner's personal workspace.

## 2. First-Run Setup

On the first page load, Revtern checks whether setup is complete.

If not, it asks the user to create:

- Owner email.
- Owner password.
- Workspace name.

This creates:

- One owner user.
- One default workspace.
- One secure local session.

The product should not ask users to understand teams, organizations, roles, or
billing accounts during first-run setup.

## 3. Create an App

After login, Revtern asks the user to create their first app.

Fields:

- App name.
- Apple bundle id, optional.
- Google package name, optional.
- Default currency, optional.

Example:

```text
Name: Tiny Notes
Apple Bundle ID: com.example.tinynotes
Google Package: com.example.tinynotes
Default Currency: USD
```

This app becomes the main filter for dashboards and source mapping.

## 4. Connect a Data Source

The user chooses one data source to start.

Recommended first options:

- RevenueCat webhook.
- Custom API.
- App Store notifications.
- Google Play RTDN.

The setup screen should show:

- What credentials are needed.
- Where to paste Revtern's webhook URL.
- Whether the source is receiving events.
- Last successful event.
- Whether missed-webhook catch-up is configured.
- Last processing error.

## 5. Review Product Catalog Draft

After the source has exposed products or sent purchase events, Revtern has
source products. The frontend then generates a product catalog draft.

The draft groups source products into proposed Revtern products.

Example:

```text
Draft product: Pro Monthly
  App Store: com.example.tinynotes.pro.monthly
  Google Play: pro_monthly / base_plan_monthly
  RevenueCat: com.example.tinynotes.pro.monthly

Draft product: Lifetime Pro
  App Store: com.example.tinynotes.pro.lifetime
  Google Play: lifetime_pro
```

The user can edit:

- Product name.
- Product kind.
- Billing period.
- Reporting category.
- Which source products belong together.
- Whether a source product should be ignored.

Nothing is created as a logical product until the user confirms the draft.

## 6. Confirm Product Catalog

When the user confirms, Revtern creates:

- `logical_products`.
- `product_mappings`.
- ignored source-product decisions if any.

Dashboard product totals use only confirmed mappings. Unmapped products remain
visible in event and transaction views and can appear in an `Unmapped` bucket.

## 7. RevenueCat First Flow

This is the easiest first integration for many indie developers.

User flow:

1. User selects `RevenueCat`.
2. Revtern creates a data source and shows a webhook URL.
3. User copies the webhook URL into RevenueCat.
4. User configures the shared secret or authorization header.
5. User sends a test webhook if available.
6. Revtern stores the raw event.
7. Revtern discovers source products from the event payload.
8. The frontend generates a catalog draft.
9. User confirms products and mappings.
10. Revtern normalizes mapped events into dashboard-ready facts.
11. The dashboard starts showing purchases and renewals.

What the user sees:

- Source status changes from `waiting_for_events` to `active`.
- Event log shows received RevenueCat events.
- Product mapping shows confirmed or unmapped source products.
- Overview dashboard shows revenue once normalized events exist.

## 8. App Store Flow

User flow:

1. User selects `App Store`.
2. Revtern shows the App Store Server Notification URL.
3. User configures the URL in App Store Connect.
4. Revtern receives signed notifications.
5. Revtern decodes the notification and nested transaction payloads.
6. Revtern stores the raw notification and normalizes lifecycle events.

What Revtern should explain clearly:

- Revtern only pulls App Store notification history for missed-webhook
  catch-up.
- App Store notification-history credentials are optional for live webhooks and
  only required before running missed-webhook catch-up.
- Metrics use fields present in the notification payload.
- Store payout-style numbers may differ from Revtern estimates.

## 9. Google Play Flow

User flow:

1. User selects `Google Play`.
2. Revtern shows setup instructions for Pub/Sub RTDN.
3. User configures Pub/Sub push to Revtern.
4. Revtern receives RTDN messages.
5. Revtern decodes the Pub/Sub payload.
6. Revtern stores the raw notification and normalizes lifecycle events.

What Revtern should explain clearly:

- Revtern only pulls retained Pub/Sub RTDN messages for missed-webhook catch-up.
- Pub/Sub pull credentials are optional for live webhooks and only required
  before running missed-RTDN catch-up.
- RTDN does not include price by default.
- Google Play revenue can only use amount fields included in the pushed payload.

## 10. Dashboard Use

Once data exists, the user lands on the Overview dashboard.

The default dashboard answers:

- How much did my apps earn today?
- How does this compare with yesterday or last month?
- How many active subscriptions do I have?
- How many new subscribers and renewals happened?
- How much was refunded?
- Did anything unusual happen?

Default filters:

- Last 30 days.
- All apps.
- All platforms.
- All products.
- Source currency unless a reporting currency is configured.

## 11. Drill-Down Flow

Every metric should be clickable.

Example:

User clicks `Refunds`.

Revtern shows:

- Refund transactions.
- Refund events.
- Source.
- App.
- Product.
- Country.
- Amount.
- Raw payload link.

User can then open a raw event to inspect the original store or RevenueCat
payload.

This is important because Revtern should be trusted as a ledger, not just a
charting layer.

## 12. Reconciliation Flow

When Revtern detects a mismatch or missing source data, it should show a clear
issue.

Examples:

- A webhook event failed normalization.
- Google Play RTDN arrived without amount fields.
- A transaction appears twice from multiple webhook sources.
- Net revenue is estimated from webhook payloads.
- Currency conversion is unavailable.
- A source product has not been confirmed into the product catalog.

User actions:

- Retry failed job.
- Send a source test webhook.
- Run missed-webhook catch-up.
- Open setup guide.
- Mark duplicate mapping.
- Export rows for manual review.

## 13. Daily Usage

After setup, the user should mostly use Revtern as a daily check-in.

Typical daily workflow:

1. Open dashboard.
2. Check today's revenue and subscriber movement.
3. Check refunds or churn spikes.
4. Look at app/platform/product breakdown.
5. Read warnings if any source is unhealthy.
6. Export CSV or share screenshot if needed.

Optional future daily workflow:

- Receive morning email summary.
- Receive Slack/Telegram/Discord alerts.
- Open mobile app to check revenue quickly.

## 14. Mobile Companion Workflow

The future React Native app should not try to replace the full web dashboard.

It should focus on:

- Today's revenue.
- Active subscriptions.
- Refunds.
- Source health.
- Alerts.
- Simple charts.

The mobile app should use the same backend API and shared TypeScript client as
the web app.

## 15. Admin Workflow

Owner-only admin screens:

- Data sources.
- Webhook secrets.
- Source test runs.
- Failed jobs.
- Raw events.
- Backup instructions.
- Version and migration status.

This matters for self-hosting because users need to understand whether their
local system is healthy.

## 16. No-SDK Workflow

MVP should not require the developer to add a client SDK to their app.

That means the first version can work through:

- Store server notifications.
- RevenueCat webhooks.
- Custom backend events.

Later, a lightweight SDK can be considered only for optional context:

- `app_user_id`.
- Paywall id.
- Campaign.
- Experiment id.
- Install timestamp.

The core promise should remain: Revtern can be useful without taking over the
purchase flow.

## 17. Minimal Happy Path

The shortest valuable flow is:

```text
docker compose up
  -> create owner
  -> create app
  -> connect RevenueCat webhook
  -> receive test event
  -> confirm generated product catalog draft
  -> see event log
  -> see dashboard update
```

This should be the first implementation milestone.
