# Changelog

## Unreleased

- Repository transferred to github.com/corbet-foss/cphn (old cmtymeet URLs redirect); package metadata points there.

## 0.1.1

- Repository moved to github.com/cmtymeet/cphn; package metadata points there.
- Released from CI through crates.io trusted publishing.

## 0.1.0 — initial scaffold

- Price-ordered cascade (WhatsApp, flash-call, SMS), E.164 normalize,
  hashed OTP with constant-time verify, HMAC community-scoped dedup,
  Ed25519 attestation bytes for cvld issuance.
- Fixture-only tests, no live provider calls.
- Licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception.
