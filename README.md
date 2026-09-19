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
