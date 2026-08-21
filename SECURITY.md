# Security Policy

## Supported Versions

Revtern is currently alpha software. Security fixes are applied to the latest
commit on the `main` branch; older commits and unreleased development snapshots
are not supported.

## Reporting a Vulnerability

Please do not report vulnerabilities in a public issue, discussion, or pull
request. Use GitHub's private vulnerability reporting feature for this
repository instead.

Include the affected version or commit, deployment mode, impact, reproduction
steps, and any suggested mitigation. Remove credentials, personal data, store
payloads, and customer information from the report.

Reports are reviewed on a best-effort basis while the project is in alpha. A
fix may be coordinated privately before details are published.

## Deployment Responsibility

Self-hosters are responsible for using unique secrets, TLS, access controls,
database backups, and a supported reverse proxy. The example development
configuration is not a production security baseline.
