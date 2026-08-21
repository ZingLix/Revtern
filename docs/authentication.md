# Authentication and App Access

Revtern is multi-user by default. Every local or OIDC account receives a
personal workspace for apps it owns, and can also access apps shared by other
users. Local authentication always uses this multi-user model.

## Authentication Modes

```text
REVTERN_AUTH_MODE=local
REVTERN_AUTH_MODE=reverse_proxy
REVTERN_AUTH_MODE=disabled
```

### `local`

This is the default production mode. It supports:

- Local email/password accounts with Argon2id password hashes.
- Open, invite-only, or closed registration.
- OpenID Connect login and explicit identity linking.
- Server-side browser sessions with CSRF protection.
- Opaque, revocable bearer sessions for the mobile companion app.
- Per-user app ownership, invitations, roles, and audit events.

The first-run owner screen only bootstraps the first administrator; subsequent
accounts use the same local account and invitation model.

### `reverse_proxy`

Revtern trusts `X-Forwarded-Email` (or `X-Forwarded-User`) only when the
deployment is protected by a trusted authentication proxy. Each discovered
user receives a personal workspace instead of being made an administrator of a
shared global workspace.

Do not expose the API directly when this mode is enabled. The proxy must strip
client-supplied identity headers before adding its trusted values.

### `disabled`

Development only. Outside development it requires
`REVTERN_UNSAFE_DISABLE_AUTH=1`. It must not be used for an internet-accessible
deployment.

## Registration

`REVTERN_REGISTRATION_MODE` controls account creation in `local` mode:

- `invite_only` (default): a valid app invitation is required.
- `open`: anyone who can reach the server can register.
- `closed`: no new local or OIDC accounts can be created.

Invitations are email-bound, expire after seven days, and are stored as token
hashes. A newly generated invitation URL is shown once to an app manager.

## App Roles

Access is evaluated for every app and every API query. Workspace IDs are not an
authorization boundary by themselves.

| Role | Read dashboard and ledger | Raw events and export | Edit app, catalog, sources, jobs | Credentials and members |
| --- | --- | --- | --- | --- |
| Viewer | Yes | No | No | No |
| Analyst | Yes | Yes | No | No |
| Editor | Yes | Yes | Yes | No |
| Manager | Yes | Yes | Yes | Yes |
| Owner / workspace admin | Yes | Yes | Yes | Yes |

The underlying capabilities are:

```text
app.read
ledger.read
events.sensitive.read
export.run
app.write
catalog.write
source.write
source.credentials.write
jobs.run
members.manage
```

App rows identify an `owner_user_id`. Shared access is represented by an
`app_memberships` row linked to an `app_roles` role. Invitations do not grant
access until they are accepted by the matching email account.

## OpenID Connect

Configure one provider with:

```text
REVTERN_OIDC_NAME=Company SSO
REVTERN_OIDC_ISSUER=https://id.example.com/realms/revtern
REVTERN_OIDC_CLIENT_ID=revtern
REVTERN_OIDC_CLIENT_SECRET=replace-me
REVTERN_OIDC_SCOPES=openid profile email
```

Register this redirect URI at the provider:

```text
https://revtern.example.com/api/auth/oidc/callback
```

Revtern uses Authorization Code flow with PKCE, one-time hashed state, nonce,
OIDC discovery, JWKS signature verification, issuer/audience/authorized-party
checks, and an exact configured redirect URI. Production discovery and token
endpoints must use HTTPS.

OIDC subjects are the stable account key. Revtern never automatically links an
OIDC identity to an existing local account by matching email alone. Sign in to
the local account and use Settings → Sign-in methods to link it explicitly.
Creating an OIDC-only account requires a verified email claim.

## Sessions and Secrets

- Browser sessions have a 12-hour idle timeout and 30-day absolute lifetime.
- Session and mobile bearer tokens are stored as SHA-256 hashes.
- Browser mutations require a matching CSRF cookie and header.
- Source credentials are encrypted with `REVTERN_SECRET_KEY`; webhook shared
  secrets are stored as password-style hashes.
- Changes to app membership, invitations, authentication links, exports, and
  app settings create audit rows.

Use a unique `REVTERN_SECRET_KEY` of at least 32 characters and HTTPS in
production. Changing the key makes encrypted source credentials and outstanding
OIDC login transactions unreadable.
