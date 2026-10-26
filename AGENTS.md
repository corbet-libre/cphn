# cphn agent instructions

Write comments and documentation in English. Owned code forbids unsafe Rust
(`#![forbid(unsafe_code)]`, `src/lib.rs:4`).

## Build and test (lane: `~/agents/scratch/cbuild <lane> ...`)

- `fmt -p cphn -- --check`
- `test -p cphn` (lib 24 + doc 0, exit 0; `--locked` needs CI's resolve refresh)
- `clippy -p cphn --all-targets -- -D warnings`
- `check -p cphn --target wasm32-unknown-unknown`
- wasm exec: `test -p cphn --target wasm32-unknown-unknown --lib` with
  `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner`
  (wasm subset 7, headless Chrome, exit 0)
- CI-only (Crow `.ci/verify.sh`): frozen-clock mutation control
  (`.ci/test-clock-regression.py`), nightly `llvm-cov` 100% line+branch gate
  (`docs/COVERAGE.md`)

## Hard invariants (each with its enforcing test in `src/tests/`)

- A comparison identity never proves unique personhood (`docs/CONTRACT.md:6`):
  `impossible_national_number_is_not_a_canonical_identity`,
  `concurrent_registration_calls_use_the_atomic_host_port`.
- Every dispatch needs a bounded cost permit; unknown prices never mean free
  (`docs/CONTRACT.md:15-16`):
  `configured_and_cost_order_cascades_reserve_failed_costs_and_stop_on_pending`.
- Auth/session mismatches never create a fact; malformed input fails closed
  without retry (`docs/CONTRACT.md:6,43-44`):
  `authentication_response_and_session_mismatches_never_create_a_fact`,
  `malformed_responses_and_possible_effects_fail_closed_without_retry`.
- Rich output is redacted non-serializable RAM; only requested rich data survives
  (`docs/CONTRACT.md:7`):
  `actual_twilio_wire_is_correlated_and_only_requested_rich_data_survives`.
- No library database: caller state owns limits, windows and idempotency
  (`docs/CONTRACT.md:33,40`):
  `caller_state_enforces_general_limits_and_idempotency_without_a_library_database`.
- Fixtures only: no live provider calls, spending or signup; tests use inert
  credentials and transports (`local_simulation_...`, `maintained_phone_metadata_...`).
- LGPL-3.0-only WITH linking exception; no GPL/AGPL-only code (`LICENSE.md`).

Pointers: `docs/CONTRACT.md`, `docs/COVERAGE.md`, `docs/PROVIDERS.md`.
