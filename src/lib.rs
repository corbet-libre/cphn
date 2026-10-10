//! Reusable phone verification, scoped comparison identities and caller-owned limits.
//! The Twilio adapter uses maintained HTTP/form libraries and an explicitly trusted transport.
//! No provider I/O occurs without a cost permit; no database connection is opened.
#![forbid(unsafe_code)]

mod adapters;
mod flow;
mod identity;
pub use adapters::{Adapter, AdapterEvidence, verify_with};
mod simulation;
pub use simulation::{Simulated, SimulationOffer, SimulationSettings, simulate};
mod twilio;
pub use cfbk::cost;
pub use flow::{Attempt, Run, begin_with_fallback};
pub use identity::{Identity, Registration, RegistrationState, Reservation, StateError};
pub use twilio::{Channel, DispatchReport, Prepared, Twilio};
use zeroize::Zeroizing;

/// Private owned bytes: explicit access only; no automatic serialization or logging.
#[derive(PartialEq, Eq)]
pub struct Private(Zeroizing<Vec<u8>>);
impl Private {
    /// Construct from caller-owned transient input or a trusted transport response.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }
    /// Explicitly borrow the private bytes in RAM.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}
impl std::fmt::Debug for Private {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Private([redacted])")
    }
}

/// Uniform errors never include destinations, tokens or provider responses.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Invalid identifier or code.
    #[error("invalid verification input")]
    Input,
    /// Invalid configuration or unsupported provider mode.
    #[error("invalid verification configuration")]
    Configuration,
    /// Provider explicitly refused verification.
    #[error("verification refused")]
    Refused,
    /// Transport established that nothing was dispatched; safe for configured fallback.
    #[error("verification not dispatched")]
    NotDispatched,
    /// A provider effect may have happened; no automatic retry or fallback.
    #[error("verification outcome uncertain")]
    Uncertain,
    /// Caller atomic operation or shared-budget reservation refused.
    #[error("verification operation reservation refused")]
    Reservation,
    /// Non-success provider response. Side effects and billing may be uncertain.
    #[error("verification provider returned a non-success response")]
    ProviderResponse {
        /// Actual HTTP status.
        status: u16,
        /// Exact bounded private response, never authenticated verification evidence.
        response: Private,
    },
    /// Explicitly requested private upstream data retained on a rejected result.
    #[error("{error}")]
    Reported {
        /// Original redacted failure category.
        error: Box<Error>,
        /// Private available upstream data; never authentication evidence.
        response: Private,
    },
    /// Cost authorization did not match this dispatch.
    #[error("verification cost authorization refused")]
    Budget,
}

impl Error {
    /// Explicit complete upstream error bytes, when an HTTP failure was received.
    /// These are provider assertions, not verified domain facts.
    pub fn upstream(&self) -> Option<&Private> {
        match self {
            Self::ProviderResponse { response, .. } | Self::Reported { response, .. } => {
                Some(response)
            }
            _ => None,
        }
    }
}
/// Trusted host transport boundary. Implementations authenticate HTTPS, bound response size,
/// disable redirects/retries/logging, and return `NotDispatched` only with definite evidence.
/// A member-controlled transport is not a verifier. Fixture transports are test-only.
pub trait Transport {
    /// Explicit transport mode; simulation is never accepted by a real adapter.
    fn mode(&self) -> cost::Mode;
    /// Dispatch exactly one prepared HTTPS request. Never persist request or response bodies.
    /// Check `DispatchDeadline` in request extensions using a fresh trusted clock
    /// immediately before I/O, including after any queue wait. Refuse missing or
    /// expired deadlines as `NotDispatched`; never retry a possibly sent operation.
    fn send(
        &self,
        request: http::Request<Private>,
    ) -> impl std::future::Future<Output = Result<http::Response<Private>, Error>>;
}

/// Local-only HTTP request extension. Never sent to the provider as a header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchDeadline {
    /// Exclusive minimum of the authenticated context and quote expiries.
    pub valid_until: u64,
}

/// Caller-owned atomic dispatch and shared-budget reservation. Implementations
/// must reject already dispatched operation IDs and conflicting terms, including
/// retries after uncertainty or a process restart. Preserve maximum exposure
/// until independently reconciled. No raw payloads are supplied or persisted here.
/// A failed reservation must leave no side effect and cannot authorize fallback.
pub trait DispatchState {
    /// Atomically bind operation to ceremony/request and reserve its maximum
    /// against the host's shared ceremony budget. Only a new reservation succeeds.
    /// Check the host's fresh clock against `valid_until` before returning success.
    fn reserve(
        &self,
        ceremony: &[u8; 32],
        operation: &[u8; 32],
        binding: &str,
        maximum: &cost::Amount,
        valid_until: u64,
    ) -> Result<(), Error>;
}

/// Trusted caller context for one verification; all fields stay in memory.
pub struct Context {
    /// Opaque subject/action/operation binding, chosen before starting the provider session.
    pub binding: [u8; 32],
    /// Unique caller operation ID for this dispatch; distinct from the ceremony.
    pub operation: [u8; 32],
    /// Comparison scope; no hard-coded community or lifetime semantics.
    pub scope: Vec<u8>,
    /// Stable identity key, separate from provider credentials; zeroized on drop.
    pub identity_key: Zeroizing<[u8; 32]>,
    /// Exclusive lifetime cap for this session and its resulting fact.
    pub expires_at: u64,
    /// Explicitly return complete provider response bytes on success.
    pub include_upstream: bool,
}

/// Transient, correlated provider session. Not serializable or a verified fact.
pub struct Pending {
    canonical: Zeroizing<String>,
    sid: String,
    account: String,
    service: String,
    channel: Channel,
    context: Context,
}
impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pending([redacted])")
    }
}

/// Authenticated phone-control fact, with no raw identifier by default.
#[derive(Debug)]
pub struct Verified {
    identity: Identity,
    binding: [u8; 32],
    expires_at: u64,
    upstream: Option<Private>,
}
impl Verified {
    /// Scoped comparison handle; this does not establish human uniqueness.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    /// Subject/action/check binding supplied before the provider ceremony.
    pub fn binding(&self) -> &[u8; 32] {
        &self.binding
    }
    /// Exclusive validity cap chosen by the caller.
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
    /// Full upstream response only when the caller requested it.
    pub fn upstream(&self) -> Option<&Private> {
        self.upstream.as_ref()
    }
    /// Ask the caller's atomic state to enforce at-most-X, idempotently.
    pub fn reserve(
        &self,
        state: &impl RegistrationState,
        registration: Registration<'_>,
        now: u64,
    ) -> Result<Reservation, StateError> {
        if now >= self.expires_at {
            return Err(StateError);
        }
        state.reserve(&self.identity, registration)
    }
}

/// A started or completed verification. Pending stops provider fallback.
#[derive(Debug)]
pub enum Step {
    /// User input/provider completion still required; retain only in RAM.
    Pending(Pending),
    /// Authenticated and transformed result.
    Verified(Verified),
}

/// Canonicalize using maintained international phone metadata; extensions are not identities.
pub fn normalize(input: &str) -> Result<String, Error> {
    let number = phonenumber::parse(None, input).map_err(|_| Error::Input)?;
    if !phonenumber::is_valid(&number) || number.extension().is_some() {
        return Err(Error::Input);
    }
    Ok(number.format().mode(phonenumber::Mode::E164).to_string())
}

#[cfg(test)]
mod tests;
