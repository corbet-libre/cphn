# cphn

**One phone possession check through one selected provider and channel.**

## Scope

`cphn` normalizes E.164 input, sends one host-generated code through a provider
port, checks possession, derives a community-scoped keyed fingerprint and binds
an attestation payload. The host supplies provider credentials, a CSPRNG, clock
and verification limits. Only a code hash remains in pending state. Phone
possession is not evidence that a person is unique.

Provider/channel ordering, fallback and minimum standards belong to `cfbk`,
configured through `crbk` settings. This leaf has no ordering, cost estimates,
provider probing or retry loop. Each `begin` sends at most once; provider refusal
or uncertain delivery is returned unchanged. A successful code check consumes
its attempt budget, so replay cannot succeed.

`VerificationLimits` contains only explicit validity and submission bounds; it
has no policy defaults. `ChannelSender` represents the one provider selected by
the caller. The host must sign attestation bytes only after a successful check
and must bind the normalized phone's fingerprint to that check. No live provider
integration is supplied or certified here. See [the contract](docs/CONTRACT.md).

## License

Copyright 2026 Julian Y. Richard Corbet. Licensed under
[LGPL-3.0-only](LICENSES/LGPL-3.0-only.txt)
[WITH LGPL-3.0-linking-exception](LICENSES/LGPL-3.0-linking-exception.txt),
with the incorporated [GPL version 3](LICENSES/GPL-3.0-only.txt).
Combined works may link statically or dynamically without relinking duties;
library modifications stay LGPL.
See [licensing notes](LICENSE.md).

## Continuous verification

Dependency updates follow main and are tested against one CI-resolved lockfile.
Line and branch coverage target 100%; failures remain blocking. See
[the coverage contract](docs/COVERAGE.md) for measurement and exclusions.

## Cryptographic implementation choice

Phone deduplication uses RustCrypto `hmac`/`sha2`, and code hashing uses `sha2`.
The code-hash comparison delegates to the existing ecosystem's
[`subtle` 2.6 ConstantTimeEq](https://docs.rs/subtle/latest/subtle/trait.ConstantTimeEq.html)
(BSD-3-Clause), replacing the historical handwritten XOR loop. Both compared
inputs are fixed 32-byte digests. `constant_time_eq` was also considered; `subtle`
provides the established fixed-length slice API needed here.
The crate implements no equality primitive. Upstream documents its constant-time
behavior as best effort on the deployed compiler/hardware; functional native
and wasm tests do not establish a timing proof.

The HMAC constructor names upstream `digest::KeyInit` explicitly, as required
by the maintained HMAC0.13/SHA2 0.11 API. An independently derived fixed HMAC
vector runs natively and on wasm to preserve existing deduplication identities
across dependency updates; no domain prefix or input encoding changed.
