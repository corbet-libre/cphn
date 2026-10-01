use super::*;
use std::collections::HashMap;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn e164_normalize() {
    assert_eq!(
        normalize_e164("+1 555 555 0100"),
        Some("+15555550100".to_owned())
    );
    assert_eq!(normalize_e164("49170"), None);
    assert_eq!(normalize_e164("+49"), None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dedup_is_community_scoped() {
    let a = dedup_handle(b"s", "alpha", "+15555550100");
    let b = dedup_handle(b"s", "beta", "+15555550100");
    assert_ne!(a, b);
    assert_eq!(a.len(), 64);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn otp_roundtrip_and_expiry() {
    let policy = CascadePolicy::default();
    let mut p = begin(&policy, "+15555550100", Channel::Sms, "482916", 1000).expect("begin");
    assert!(check(&mut p, "482916", 1001).is_ok());
    let mut q = begin(&policy, "+15555550100", Channel::Sms, "111111", 1000).expect("begin");
    assert_eq!(check(&mut q, "000000", 1001), Err(Error::Rejected));
    assert_eq!(check(&mut q, "111111", 2000), Err(Error::Rejected));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cascade_default_order_is_cheapest_first() {
    let order: HashMap<Channel, usize> = CascadePolicy::default()
        .order
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, i))
        .collect();
    assert!(order[&Channel::Whatsapp] < order[&Channel::FlashCall]);
    assert!(order[&Channel::FlashCall] < order[&Channel::Sms]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn code_shape_and_attempt_budget_refuse_without_weakening_the_deadline() {
    let policy = CascadePolicy {
        max_attempts: 2,
        ..CascadePolicy::default()
    };
    assert!(matches!(
        begin(&policy, "invalid", Channel::Sms, "1234", 0),
        Err(Error::InvalidPhone)
    ));
    for code in ["123", "12345678901", "123a", "１２３４"] {
        assert!(matches!(
            begin(&policy, "+15555550100", Channel::Sms, code, 0),
            Err(Error::Rejected)
        ));
    }
    for code in ["1234", "1234567890"] {
        let mut pending = begin(&policy, "+15555550100", Channel::Sms, code, 100).unwrap();
        assert_eq!(check(&mut pending, "wrong", 100), Err(Error::Rejected));
        assert_eq!(pending.attempts_left, 1);
        assert_eq!(check(&mut pending, code, 100), Ok(()));
        assert_eq!(pending.attempts_left, 0);
        assert_eq!(check(&mut pending, code, 100), Err(Error::Rejected));
    }
    let mut pending = begin(&policy, "+15555550100", Channel::Sms, "1234", u64::MAX - 1).unwrap();
    assert_eq!(pending.expires_at, u64::MAX);
    assert_eq!(check(&mut pending, "1234", u64::MAX), Err(Error::Rejected));
    assert_eq!(pending.attempts_left, 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn canonical_phone_and_public_attestation_boundaries() {
    assert_eq!(
        normalize_e164(" +1 (555) 555-0100 "),
        Some("+15555550100".into())
    );
    assert!(normalize_e164("+1234567").is_some());
    assert!(normalize_e164("+123456789012345").is_some());
    assert!(normalize_e164("+1234567890123456").is_none());
    assert!(normalize_e164("+1234abc").is_none());
    let bytes = attestation_bytes("opaque-handle", "policy", 123);
    let fields: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        fields,
        serde_json::json!(["cphn.attestation.v1", "opaque-handle", "policy", 123])
    );
    assert_ne!(bytes, attestation_bytes("other-handle", "policy", 123));
    assert_ne!(
        bytes,
        attestation_bytes("opaque-handle", "other-policy", 123)
    );
    assert_ne!(bytes, attestation_bytes("opaque-handle", "policy", 124));
    for error in [
        Error::InvalidPhone,
        Error::Rejected,
        Error::Declined,
        Error::Transport,
    ] {
        assert!(!error.to_string().is_empty());
    }
}
