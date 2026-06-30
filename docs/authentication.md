# Authentication Design

## Short Answer

Revtern should have a minimal user system from the beginning, but not a complex
enterprise account system.

For a self-hosted product that stores revenue, purchase events, and store API
credentials, running with no login is risky. However, full organizations,
invitations, SSO, audit logs, and granular RBAC can wait.

## Recommended MVP Auth Model

Support three modes:

```text
REVTERN_AUTH_MODE=single_user
REVTERN_AUTH_MODE=reverse_proxy
REVTERN_AUTH_MODE=disabled
```

### single_user

Default production mode.

Behavior:

- On first launch, Revtern requires owner setup.
- First user becomes `owner`.
- Email and password login.
- Session cookie.
- CSRF protection for browser requests.
- All data belongs to one workspace.

This is enough for most self-hosted indie developers.

### reverse_proxy

For users who deploy behind Authelia, Tailscale, Cloudflare Access, OAuth2
Proxy, or similar.

Behavior:

- Revtern trusts configured headers from a trusted reverse proxy.
- Example headers: `X-Forwarded-User`, `X-Forwarded-Email`.
- Local password login can be disabled.
- Still creates an internal user row for ownership and audit references.

This mode is useful for advanced self-hosters.

### disabled

Development-only mode.

Behavior:

- No login.
- Server should warn loudly at startup.
- Should be refused unless `REVTERN_ENV=development` or an explicit unsafe flag
  is set.

## Why Not Skip Users Entirely?

Even for self-host:

- Store credentials need access control.
- Raw purchase data can contain sensitive information.
- A browser dashboard exposed accidentally should not leak revenue data.
- Future hosted cloud needs a migration path.
- User ownership is useful for setup, secrets, and audit trails.

The important part is keeping the first version small.

## MVP Tables

### users

- `id`
- `email`
- `password_hash`
- `display_name`
- `role`
- `created_at`
- `last_login_at`

### sessions

- `id`
- `user_id`
- `session_hash`
- `expires_at`
- `created_at`
- `last_seen_at`

### workspaces

- `id`
- `name`
- `created_at`

MVP can enforce one workspace globally.

## Roles

MVP roles:

- `owner`: can configure sources, view raw events, manage settings.
- `viewer`: can view dashboards and non-secret data.

Viewer can be added after the first owner-only version. The schema can support
it early without exposing the UI.

## Passwords and Sessions

Use:

- Argon2id for password hashing.
- Secure, HTTP-only, SameSite cookies.
- Server-side sessions stored in Postgres.
- CSRF token for state-changing browser requests.

Avoid JWT-only browser auth for MVP. Server-side sessions are easier to revoke
and inspect in a self-hosted admin tool.

## Future Auth

Later additions:

- Invite users.
- Multiple workspaces.
- OAuth login.
- SSO.
- API tokens.
- Audit log.
- Granular RBAC.

Do not build these before the data pipeline and dashboard are useful.

