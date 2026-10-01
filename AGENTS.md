# Agent instructions

Write all code comments and documentation in English.

## Product boundary

- cphn performs one phone-possession check through one selected provider/channel.
  It owns normalization, OTP state, keyed fingerprinting and attestation binding.
  Ordering, fallback and minimum standards belong to cfbk, configured by crbk.
  No provider order, cost policy, retries or accepting production stubs here.
- Raw phone numbers must never enter logs, errors, test fixtures or
  attestation output. Store only HMAC(server_secret, community | E.164)
  for dedup. Deleting raw after verification is hygiene, not blindness:
  a self-operated checker still fails the strict cvld opaque bar where
  an independent provider must retain inputs.
- Phone proves control at a time, not one-human uniqueness. Recycling,
  VoIP/burner policy and re-verification frequency belong in host policy.
- This crate is LGPL-3.0-only WITH LGPL-3.0-linking-exception: combined works
  may link statically or dynamically without relinking duties; library
  modifications stay LGPL. Do not add implementation code available
  only under the full GPL or AGPL.
- No live provider calls in tests. Fixtures only. No spending, registration
  or outreach from this library.

## Quality boundary

- `cargo fmt --check`, `cargo clippy --all-targets` (no warnings),
  `cargo test` — all green before every commit. No local workstation
  builds per task status; use GHA, Crow fallback while GHA is down.
- Validate one provider call, constant-time code verification, uniform errors,
  expiry, rate-limit hooks and HMAC domain separation with fixtures.
