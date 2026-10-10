//! Reusable phone verification, scoped comparison identities and caller-owned limits.
//! The Twilio adapter uses maintained HTTP/form libraries and an explicitly trusted transport.
//! No provider I/O occurs without a cost permit; no database connection is opened.
#![forbid(unsafe_code)]

mod flow;
mod identity;
mod simulation;
pub use simulation::{Simulated, SimulationOffer, SimulationSettings, simulate};
mod twilio;
pub use cfbk::cost;
pub use flow::{Attempt, Run, begin_with_fallback};
pub use identity::{Identity, Registration, RegistrationState, Reservation, StateError};
pub use twilio::{Channel, Prepared, Twilio};
use zeroize::Zeroizing;

/// Private owned bytes: explicit access only; no automatic serialization or logging.
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
    /// Cost authorization did not match this dispatch.
    #[error("verification cost authorization refused")]
    Budget,
}

/// Trusted host transport boundary. Implementations authenticate HTTPS, bound response size,
/// disable redirects/retries/logging, and return `NotDispatched` only with definite evidence.
/// A member-controlled transport is not a verifier. Fixture transports are test-only.
pub trait Transport {
    /// Explicit transport mode; simulation is never accepted by a real adapter.
    fn mode(&self) -> cost::Mode;
    /// Dispatch exactly one prepared HTTPS request. Never persist request or response bodies.
    fn send(
        &self,
        request: http::Request<Private>,
    ) -> impl std::future::Future<Output = Result<http::Response<Private>, Error>>;
}

/// Trusted caller context for one verification; all fields stay in memory.
pub struct Context {
    /// Opaque subject/action/operation binding, chosen before starting the provider session.
    pub binding: [u8; 32],
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
