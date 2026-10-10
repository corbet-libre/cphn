# Agent instructions

Write comments and documentation in English. Owned code forbids unsafe Rust.

## Product boundary

- cphn is reusable phone expertise; cvld is one consumer. Caller rules select
  providers/channels, requested information, scope and general at-most-X limits.
- Maintained upstream normalization and independent provider verification; no
  home-made OTP/cryptographic protocols or fixed cheapest-provider assumptions.
- Shared `cfbk` owns cost planning and fallback. Every real dispatch requires a
  matching bounded cost permit; unknown prices never mean free.
- Raw results are explicit, private, RAM-only. No database connections, SQL,
  file caches or hidden persistence. Default diagnostics redact private data.
- Caller atomic state owns registration, counting windows, release and replay;
  identifier control is not one-human uniqueness.
- No live provider/sandbox calls, spending, account signup, registry publication
  or deployment during development. Tests use inert credentials and transports.
- Preserve LGPL-3.0-only WITH LGPL-3.0-linking-exception; no GPL/AGPL-only code.

## Quality boundary

Workstations edit and format; Crow compiles, tests and enforces existing full
source coverage. Commit candidates before remote validation. Test owned behavior
through real upstream libraries; report unexecuted live flows separately.
