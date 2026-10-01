//! Single-provider phone possession verification with an opaque fingerprint.
//!
//! The caller selects one provider and channel through Fallback (`cfbk`) using
//! Rulebook (`crbk`) settings. This leaf normalizes a phone, sends one code through
//! [`ChannelSender`], verifies the reply and prepares an attestation binding.
//! It neither orders providers nor retries or changes the selected channel.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Transport channel selected by the caller for one verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Channel {
    /// WhatsApp authentication template.
    Whatsapp,
    /// Missed-call CLI is the code. Android auto-read, iOS/web manual.
    FlashCall,
    /// SMS message.
    Sms,
}

/// One host-selected provider. The library never holds provider credentials.
pub trait ChannelSender {
    /// Send one code out of band, without logging the destination or code.
    /// A refusal or uncertain transport outcome is returned without another send.
    fn send(&self, channel: Channel, e164: &str, code: &str) -> Result<(), Error>;
}

/// Caller-supplied bounds for one possession check, without policy defaults.
#[derive(Debug, Clone, Copy)]
pub struct VerificationLimits {
    /// Nonzero validity in seconds, supplied by authenticated host settings.
    pub code_lifetime_secs: u64,
    /// Nonzero maximum submissions before the code is invalidated.
    pub max_attempts: u32,
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
    /// Explicit user decline of the selected channel.
    #[error("declined")]
    Declined,
    /// Selected provider unavailable or delivery outcome uncertain.
    #[error("transport unavailable")]
    Transport,
}

/// Normalize and validate before sending once through the selected provider.
/// Only the hash is kept in the returned [`Pending`]. The host generates the code
/// with its CSPRNG and supplies one provider/channel already selected by `cfbk`.
/// A provider error produces no pending check and never triggers a retry.
pub fn begin(
    sender: &impl ChannelSender,
    limits: &VerificationLimits,
    e164: &str,
    channel: Channel,
    code: &str,
    now_secs: u64,
) -> Result<Pending, Error> {
    let e164 = normalize_e164(e164).ok_or(Error::InvalidPhone)?;
    if limits.code_lifetime_secs == 0 || limits.max_attempts == 0 {
        return Err(Error::Rejected);
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
    sender.send(channel, &e164, code)?;
    Ok(Pending {
        code_hash: digest,
        expires_at: now_secs.saturating_add(limits.code_lifetime_secs),
        attempts_left: limits.max_attempts,
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
        pending.attempts_left = 0;
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
