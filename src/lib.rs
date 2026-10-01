//! Price-ordered phone verification cascade with opaque dedup.
//!
//! The host implements [`ChannelSender`] for each transport (WhatsApp auth,
//! flash-call, SMS). The library owns ordering, OTP state, constant-time
//! verification, uniform errors and HMAC dedup. Raw phones never enter logs
//! or attestation output.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Transport channel, ordered cheapest first by [`CascadePolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Channel {
    /// WhatsApp authentication template. Cheapest in EU, needs app installed.
    Whatsapp,
    /// Missed-call CLI is the code. Android auto-read, iOS/web manual.
    FlashCall,
    /// Raw SMS fallback. Universal coverage, highest EU per-message cost.
    Sms,
}

/// Host-implemented transport. The library never holds provider keys.
pub trait ChannelSender {
    /// Send a code out-of-band. Returns provider message id for accounting.
    /// Must not log the destination phone.
    fn send(&self, channel: Channel, e164: &str, code: &str) -> Result<String, Error>;
    /// Estimated cost in USD cents per segment for a destination country.
    fn cost_cents(&self, channel: Channel, country: &str) -> Option<u32>;
}

/// Ordered cascade policy. Explicit user decline stops, failure falls through.
#[derive(Debug, Clone)]
pub struct CascadePolicy {
    /// Ordered cheapest first, e.g. [Whatsapp, FlashCall, Sms].
    pub order: Vec<Channel>,
    /// Code validity seconds, e.g. 300-600.
    pub code_lifetime_secs: u64,
    /// Maximum attempts per code before invalidation.
    pub max_attempts: u32,
}

impl Default for CascadePolicy {
    fn default() -> Self {
        Self {
            order: vec![Channel::Whatsapp, Channel::FlashCall, Channel::Sms],
            code_lifetime_secs: 300,
            max_attempts: 3,
        }
    }
}

/// Normalize to E.164: leading `+`, 7-15 digits. Returns canonical form.
pub fn normalize_e164(input: &str) -> Option<String> {
    let trimmed = input.trim().replace([' ', '-', '(', ')'], "");
    let digits = trimmed.strip_prefix('+')?;
    if !(7..=15).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("+{digits}"))
}

/// Opaque dedup handle. HMAC, not encryption: irreversible without the secret.
/// Domain-separated by community so one phone yields different handles per community.
/// Pass E.164 from [`normalize_e164`]; other forms of one number yield other handles.
#[must_use]
pub fn dedup_handle(server_secret: &[u8], community_id: &str, e164: &str) -> String {
    let mut mac = <Hmac<Sha256> as hmac::digest::KeyInit>::new_from_slice(server_secret)
        .expect("secret accepts any length");
    mac.update(b"cphn.dedup.v1\n");
    mac.update(community_id.as_bytes());
    mac.update(b"\n");
    mac.update(e164.as_bytes());
    use std::fmt::Write;
    let bytes = mac.finalize().into_bytes();
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Pending OTP with hashed code. Host stores this, never the raw code.
#[derive(Debug, Clone)]
pub struct Pending {
    /// Hash of the code (raw SHA-256 bytes). Constant-time compared.
    pub code_hash: [u8; 32],
    /// Expiry as unix seconds per host clock.
    pub expires_at: u64,
    /// Remaining attempts.
    pub attempts_left: u32,
    /// Channel used for this attempt.
    pub channel: Channel,
}

/// Errors carry no phone, code or provider id.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Invalid E.164 input.
    #[error("invalid phone")]
    InvalidPhone,
    /// Code expired or attempts exhausted. Uniform to avoid enumeration.
    #[error("verification rejected")]
    Rejected,
    /// Explicit user decline of a channel. Caller stops cascade.
    #[error("declined")]
    Declined,
    /// Transport failure. Caller may fall through to next channel.
    #[error("transport unavailable")]
    Transport,
}

/// Start a verification: normalize the phone, hash the host-generated code.
/// Only the hash is kept in the returned [`Pending`]; the host sends the code itself.
pub fn begin(
    policy: &CascadePolicy,
    e164: &str,
    channel: Channel,
    code: &str,
    now_secs: u64,
) -> Result<Pending, Error> {
    if normalize_e164(e164).is_none() {
        return Err(Error::InvalidPhone);
    }
    if code.len() < 4 || code.len() > 10 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Rejected);
    }
    let digest = {
        use sha2::Digest;
        let mut h = Sha256::new();
        h.update(b"cphn.code.v1\n");
        h.update(code.as_bytes());
        h.finalize().into()
    };
    Ok(Pending {
        code_hash: digest,
        expires_at: now_secs.saturating_add(policy.code_lifetime_secs),
        attempts_left: policy.max_attempts,
        channel,
    })
}

/// Verify a submitted code in constant time. Mutates attempts. Uniform error.
pub fn check(pending: &mut Pending, code: &str, now_secs: u64) -> Result<(), Error> {
    if now_secs >= pending.expires_at || pending.attempts_left == 0 {
        return Err(Error::Rejected);
    }
    pending.attempts_left = pending.attempts_left.saturating_sub(1);
    let digest = {
        use sha2::Digest;
        let mut h = Sha256::new();
        h.update(b"cphn.code.v1\n");
        h.update(code.as_bytes());
        h.finalize()
    };
    if bool::from(pending.code_hash.as_slice().ct_eq(&digest[..])) {
        Ok(())
    } else {
        Err(Error::Rejected)
    }
}

/// Attestation bytes the host signs with Ed25519 for cvld issuance.
/// Binds dedup handle, policy digest and expiry. Contains no raw phone.
#[must_use]
pub fn attestation_bytes(dedup: &str, policy_digest: &str, valid_until: u64) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!([
        "cphn.attestation.v1",
        dedup,
        policy_digest,
        valid_until,
    ]))
    .expect("JSON serializes")
}

#[cfg(test)]
mod tests;
