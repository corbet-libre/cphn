# Phone possession contract

Home: `cvld` gate leaves; provider selection is composed through `cfbk` from
`crbk` settings. The library performs one check, never a provider cascade.

1. The host selects one provider/channel, obtains verification limits and creates
   a code using its CSPRNG. The host owns credentials and raw-input hygiene.
2. `begin` normalizes the phone and validates the code and nonzero limits before
   invoking `ChannelSender::send` exactly once. It returns hashed pending state
   only if that call succeeds. Errors carry no identifiers and never cause retry.
3. `check` enforces expiry and a finite attempt budget using the supplied clock.
   It compares SHA-256 code digests through `subtle`. Success consumes the code.
4. The trusted host derives `dedup_handle` from the normalized phone and its
   community secret. It binds that handle, policy digest and expiry through
   `attestation_bytes`, and signs only after verified possession. The byte builder
   itself confers no admission authority. No raw phone enters the attestation.

A send error can be an unknown outcome, not proof that nothing was delivered.
The caller must reconcile it before any further side effect. Independent-provider
opaque admission, recycling policy and production adapters remain host concerns.

Native and actual wasm vectors exercise all channels, real OTP hashing/comparison,
HMAC identity vectors, canonical delivery, expiry, exhaustion, one-use success,
provider decline and transport uncertainty. An in-memory out-of-band mailbox tests
only the external provider boundary; no live carrier validation is claimed.
No provider calls, spending or personal phone fixtures are used in CI.
