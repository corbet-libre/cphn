use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::num::NonZeroU32;

/// Opaque keyed comparison identity, scoped to an identifier scheme and caller scope.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Identity([u8; 32]);
impl Identity {
    pub(crate) fn derive(key: &[u8; 32], scope: &[u8], issuer: &str, subject: &str) -> Self {
        let mut mac = <Hmac<Sha256> as hmac::digest::KeyInit>::new_from_slice(key)
            .expect("HMAC accepts 32-byte keys");
        for part in [
            b"cphn.identity.v1".as_slice(),
            scope,
            issuer.as_bytes(),
            subject.as_bytes(),
        ] {
            mac.update(&(part.len() as u64).to_be_bytes());
            mac.update(part);
        }
        Self(mac.finalize().into_bytes().into())
    }
    /// Explicit bytes for caller-owned atomic comparison and persistence.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Identity([redacted])")
    }
}

/// Caller-defined registration semantics, with no implicit lifetime or community.
#[derive(Debug, Clone, Copy)]
pub struct Registration<'a> {
    /// Stable idempotency identity for this registration, bound by the caller to its subject.
    pub id: &'a [u8],
    /// Caller-defined counting window/reset domain.
    pub window: &'a [u8],
    /// Maximum simultaneous/counting registrations in that window.
    pub limit: NonZeroU32,
}
/// Atomic reservation result; `AlreadyReserved` must not consume another slot.
#[derive(Debug, PartialEq, Eq)]
pub enum Reservation {
    /// This registration acquired a slot.
    Reserved,
    /// The same bound registration already owns its slot.
    AlreadyReserved,
    /// Caller-selected limit reached.
    LimitReached,
}
/// State unavailable or evidence expired; fail closed.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("registration unavailable")]
pub struct StateError;
/// Caller-owned atomic state. No database connections or persistence in this crate.
pub trait RegistrationState {
    /// Atomically bind `(identity, window, id)` and enforce `limit`.
    /// Reject conflicting reuse of an id; retries must preserve its original binding.
    /// Distributed callers must implement this across processes, not count in RAM.
    /// Check a fresh trusted clock against the exclusive `valid_until` inside the
    /// atomic commit, including after synchronous lock/queue waits. Expiry must
    /// leave state unchanged; a synchronous callback may still block.
    fn reserve(
        &self,
        identity: &Identity,
        registration: Registration<'_>,
        valid_until: u64,
    ) -> Result<Reservation, StateError>;
}
