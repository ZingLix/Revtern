# Contributing to Revtern

Revtern is currently alpha software. Bug reports, focused fixes, connector
improvements, tests, and documentation updates are welcome.

## Before You Start

- Search existing issues before opening a new one.
- Open an issue before starting a large feature or architectural change.
- Do not include credentials, webhook payloads, customer data, or other
  sensitive information in issues, logs, fixtures, or pull requests.
- Report security issues according to [SECURITY.md](SECURITY.md), not in a
  public issue.

## Development Setup

Install the JavaScript dependencies and start Postgres, then run the API and
web app from the repository root:

```bash
npm install
REVTERN_BIND=127.0.0.1:3001 cargo run -p revtern-api
npm run dev:web
```

See [README.md](README.md) and the files under [docs](docs) for configuration,
architecture, and connector details.

## Checks

Run the checks relevant to your change before submitting a pull request:

```bash
cargo fmt --check
cargo test --workspace
npm run typecheck
npm run build:web
```

Add or update tests when changing parsing, normalization, authentication,
metrics, or other behavior that can affect stored revenue data.

## Pull Requests

Keep pull requests focused. Explain the user-visible behavior, note any schema
or configuration changes, and include screenshots for web interface changes.
Avoid unrelated formatting or dependency churn.

## Contribution License

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Revtern is licensed under the Apache License 2.0, consistent
with the project's [LICENSE](LICENSE).
