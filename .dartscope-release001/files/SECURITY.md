# Security Policy

## Supported Versions

DartScope is pre-1.0. Security fixes are provided for the latest published `0.1.x` release and the
current `main` branch. Older development snapshots are not maintained after a replacement release
is available.

| Version | Security fixes |
| --- | --- |
| Latest `0.1.x` | Supported |
| `main` | Best-effort pre-release fixes |
| Older snapshots | Not supported |

## Reporting A Vulnerability

Use the repository Security tab and choose **Report a vulnerability** to submit a private GitHub
Security Advisory. Include the affected crate or CLI command, the version or commit, reproduction
steps, expected impact, and any proposed mitigation.

Do not include exploit details, secrets, private source code, or unredacted customer data in a
public issue. When private advisory reporting is unavailable, open a public issue that contains no
vulnerability details and asks the maintainers to establish a private reporting channel.

## Handling And Disclosure

Maintainers will validate the report, identify affected releases, prepare tests and a fix, and
coordinate disclosure with the reporter. There is no guaranteed response SLA, but reports are
handled as a priority relative to normal feature work.

Published Rust crates cannot be deleted from crates.io. A vulnerable release may be yanked after a
fixed release is available; the changelog and advisory should identify the safe replacement.
