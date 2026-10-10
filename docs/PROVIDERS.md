# Phone integrations, costs and credentials

| Integration | Implemented | Qualification limit |
|---|---|---|
| Twilio Verify v2 | Create/check, SMS/RCS, WhatsApp with possible SMS fallback, and voice, correlated response validation, full-result option | Wire contract exercised with inert transports; live delivery not run |
| Other providers | Can be researched and integrated under the same domain/cost boundary | No other concrete provider is implemented or enabled |

The adapter wraps maintained `http`, `url`, Serde and RustCrypto implementations;
Twilio publishes official SDKs for several languages but no Rust SDK in its
Verify guide. This small form/response integration avoids introducing another
language runtime or claiming an unmaintained SDK as maintained.

## Domain knowledge

`phonenumber` supplies country metadata, parsing and E.164 formatting.
Extensions are rejected. Phone control does not prove one person: shared,
reassigned, VoIP and multiple numbers remain distinct policy considerations.
Twilio's current service settings can upgrade SMS to RCS and fall back from
WhatsApp to SMS; these are declared compatible phone-control channels. The
adapter does not promise WhatsApp-only delivery. A tariff must cover every
provider-managed route enabled for the service; otherwise refuse dispatch.

[Phone metadata library](https://docs.rs/phonenumber/latest/phonenumber/),
[Twilio channel behavior](https://www.twilio.com/docs/verify/fallback-scenarios).


## Credentials and transport

Inject a Twilio account SID (`AC...`), Verify service SID (`VA...`) and its auth
token explicitly. The token stays in private RAM; the Basic authorization header
is marked sensitive. IDs/configuration are separate from secrets. Credential
rotation keeps account/service identity and pending context stable. The stable
HMAC identity key is unrelated to that token and requires a planned migration
when changed. Do not forward either secret to a member.

The caller injects a trusted HTTPS transport. Authenticate the fixed
`verify.twilio.com` endpoint, bound time and response size, and disable redirects,
retries, request/response logging and caches. Only a definitively unstarted
transport can report `NotDispatched`; HTTP errors and ambiguous delivery retain
cost exposure. The adapter never converts arbitrary supplied JSON into a proof.

Owned private buffers are zeroized on drop. HTTP/header/parser libraries may
make ordinary memory copies; host swap/dumps are outside this crate's control.
Pending destination/session data remains in RAM only. Requested durable storage,
registration and cross-process operation identity belong to the caller.

## Cost and execution modes

There is no hard-coded tariff or assumption that a channel is cheapest. The host
supplies an exact request-bound quote or a defensible maximum, denomination,
source revision and expiry. Unknown, stale, mismatched or over-budget offers are
refused before dispatch. Budgets cover the whole cascade, including failed
attempts. Start and check are separately bound operations; account for both
transport and verification fees rather than treating the OTP check as free.
The library conservatively reports reserved exposure, not invented actual fees.

Twilio's pricing depends on channel/destination/account terms and may include
multiple components. Consult [Verify pricing](https://www.twilio.com/en-us/verify/pricing)
and the configured service tariff. Local preparation and planning cannot contact
quote endpoints. Provider-managed routing must fit the supplied maximum.

Real dispatch accepts explicit Production credentials/transport/permits.
Sandbox is refused: this adapter does not implement a qualified Verify sandbox,
and trial accounts must not be presumed free. [Twilio test credentials](https://www.twilio.com/docs/iam/test-credentials)
cover selected APIs; they do not justify labelling a Verify call free.
Simulation uses the shared pure planner and explicit dummy outcomes; it has no
transport or verified-result constructor. No live calls are authorized here.

## Evidence and upstream reference

Fixture tests use actual HTTP requests, form encoding, normalization, HMAC and
JSON parsing. They cover correlation, privacy, scoped limits, pending/rejected/
uncertain outcomes, changed quotes/modes and costed provider/channel fallback.
Live carrier/mail delivery and service provisioning are not executed or claimed
qualified. Maintained dependency ranges follow current compatible releases;
committed CI locks record the exact tested graph.

[Verification API](https://www.twilio.com/docs/verify/api/verification),
[verification-check API](https://www.twilio.com/docs/verify/api/verification-check).
