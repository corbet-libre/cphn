# Phone verification contract

The caller selects one or more explicit provider/channel operations and asks
for phone control, a scoped comparison identity and its required information.
The library owns normalization, provider-response verification and transformation.
A comparison identity does not establish that the holder is a unique person.

1. Construct a configured `Twilio` adapter with injected credentials. No secrets
   discovery, account registration or implicit network access takes place.
2. `prepare_begin` normalizes the destination and binds a private HTTP request to
   account/service, operation, channel, subject/action context, identity scope,
   validity and requested output. Preparation is inert; the quote binding is keyed.
3. Supply an exact price or defensible maximum with source and expiry. The maximum
   must cover provider-managed routing and failed-attempt charges. `cfbk` supplies
   the configured/cost order and whole-cascade budget, then a move-only permit. Every dispatch additionally requires `DispatchState`
   to atomically reserve its unique operation ID and maximum exposure against
   the host ceremony budget before the provider transport can run.
4. `begin_with_fallback` performs the cascade and conservatively retains each
   attempt's maximum exposure. Only a transport-proven `NotDispatched` result
   permits fallback. Pending, uncertain, rejected and successful results stop it.
5. A pending session remains in RAM. `prepare_check` binds one supplied code to its
   exact provider session and context; it cannot accept an arbitrary member SID.
   Checks also require cost authorization and a new caller operation ID. No automatic resending or retries.
6. An approved authenticated response must match account, service, destination,
   supported channel and session. The result carries only the scoped keyed
   comparison value, context binding and caller validity cap by default.
7. Explicit rich output returns all response bytes, including unknown extensions,
   in a redacted, non-serializable RAM wrapper. Provider fields are distinguishable
   from the verified domain fact. Only requested rich output retains the address.
8. `Verified::reserve` calls the host's atomic registration port with the opaque
   identity, stable registration ID, caller-defined window and nonzero limit.
   The host must reject conflicting ID reuse and implement cross-process atomicity;
   an in-memory fixture is not a production database or a uniqueness guarantee.

The transport is a trusted application boundary. It must authenticate HTTPS,
limit responses and timeouts, disable redirects/retries and avoid payload logging
or caching. A member-controlled transport cannot supply verification evidence.
The adapter bounds parsed responses to 64 KiB and emits uniform errors.

There is no direct database connection or persistent session journal. The caller
owns operation identity and pending cost exposure across restarts through the
required `DispatchState` port. Reconstructed requests cannot bypass a retained
reservation. Exact already-dispatched retries and conflicting request/budget
terms refuse; the host may recover a known prior result outside the adapter. Losing RAM
state does not authorize resending: an uncertain effect remains uncertain.
Expired verified facts cannot reserve registrations. Host policy checks validity
when consuming returned facts and supplies fresh, correctly bound operation IDs.

See [integration and cost notes](PROVIDERS.md) for actual capabilities and limits.

Non-success HTTP responses retain their bounded exact body in `ProviderResponse`.
Default errors/Debug redact it; `Error::upstream` is explicit private RAM access.
Provider error fields remain assertions, not authenticated verification facts.
HTTP failure keeps charge/side-effect uncertainty and never triggers fallback.

The dispatch-state port receives the exclusive minimum of quote and ceremony
expiry and must check its fresh host clock before returning authorization. A
slow failed attempt cannot reuse the cascade's original planning timestamp to
send an expired fallback. No implicit clock or database is introduced.
