# cphn

**Price-ordered phone verification cascade with opaque dedup.**

`cphn` defines ordering and verification state for phone possession checks.
The host supplies transports (WhatsApp, flash-call, SMS via Bird or equivalent)
and owns provider keys and raw handling. No live calls from the library.

```rust
use cphn::{CascadePolicy, Channel, normalize_e164};
```

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
already fits the HMAC dependency family and avoids a separate primitive package.
The crate implements no equality primitive. Upstream documents its constant-time
behavior as best effort on the deployed compiler/hardware; functional native
and wasm tests do not establish a timing proof.
