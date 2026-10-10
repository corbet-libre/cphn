# cphn

Reusable phone verification, opaque comparison identities and caller-defined
registration limits. `cvld` is one consumer; explicit callers can request complete
upstream responses in RAM. The library has no database connection.

The first concrete adapter implements Twilio Verify SMS/RCS, WhatsApp with possible SMS fallback, and voice. Maintained
libraries normalize identifiers and construct authenticated requests. Shared
`cfbk` plans caller-selected provider/channel cascades under explicit cost bounds;
unknown costs cannot authorize dispatch. Independent provider checks establish
control, never one-human uniqueness.

See [the contract](docs/CONTRACT.md) and [provider, credential and cost notes](docs/PROVIDERS.md).
Live provider flows remain unexecuted; remote checks and independent review are
required before qualification.

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
