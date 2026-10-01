use super::*;
use std::cell::{Cell, RefCell};

// An out-of-band mailbox exercises the provider boundary; it does not verify codes.
#[derive(Default)]
struct Mailbox(RefCell<Vec<(Channel, String, String)>>);
impl ChannelSender for Mailbox {
    fn send(&self, channel: Channel, e164: &str, code: &str) -> Result<(), Error> {
        self.0
            .borrow_mut()
            .push((channel, e164.into(), code.into()));
        Ok(())
    }
}

fn limits() -> VerificationLimits {
    VerificationLimits {
        code_lifetime_secs: 300,
        max_attempts: 3,
    }
}

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
    let policy = limits();
    let mut p = begin(
        &Mailbox::default(),
        &policy,
        "+15555550100",
        Channel::Sms,
        "482916",
        1000,
    )
    .expect("begin");
    assert!(check(&mut p, "482916", 1001).is_ok());
    let mut q = begin(
        &Mailbox::default(),
        &policy,
        "+15555550100",
        Channel::Sms,
        "111111",
        1000,
    )
    .expect("begin");
    assert_eq!(check(&mut q, "000000", 1001), Err(Error::Rejected));
    assert_eq!(check(&mut q, "111111", 2000), Err(Error::Rejected));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn code_shape_and_attempt_budget_refuse_without_weakening_the_deadline() {
    let policy = VerificationLimits {
        max_attempts: 2,
        ..limits()
    };
    assert!(matches!(
        begin(
            &Mailbox::default(),
            &policy,
            "invalid",
            Channel::Sms,
            "1234",
            0
        ),
        Err(Error::InvalidPhone)
    ));
    for code in ["123", "12345678901", "123a", "１２３４"] {
        assert!(matches!(
            begin(
                &Mailbox::default(),
                &policy,
                "+15555550100",
                Channel::Sms,
                code,
                0
            ),
            Err(Error::Rejected)
        ));
    }
    for code in ["1234", "1234567890"] {
        let mut pending = begin(
            &Mailbox::default(),
            &policy,
            "+15555550100",
            Channel::Sms,
            code,
            100,
        )
        .unwrap();
        assert_eq!(check(&mut pending, "wrong", 100), Err(Error::Rejected));
        assert_eq!(pending.attempts_left, 1);
        assert_eq!(check(&mut pending, code, 100), Ok(()));
        assert_eq!(pending.attempts_left, 0);
        assert_eq!(check(&mut pending, code, 100), Err(Error::Rejected));
    }
    let mut pending = begin(
        &Mailbox::default(),
        &policy,
        "+15555550100",
        Channel::Sms,
        "1234",
        u64::MAX - 1,
    )
    .unwrap();
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn fixed_dedup_vector_survives_crypto_dependency_updates() {
    assert_eq!(
        dedup_handle(b"fixture-secret", "fixture", "+15555550100"),
        "4456eb7087301dbb6035295daa3d398869b422083b3ff560846b3b3dce55815c"
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn each_selected_channel_sends_once_and_a_success_consumes_the_code() {
    for channel in [Channel::Whatsapp, Channel::FlashCall, Channel::Sms] {
        let mailbox = Mailbox::default();
        let mut pending = begin(
            &mailbox,
            &limits(),
            " +1 (555) 555-0100 ",
            channel,
            "482916",
            100,
        )
        .unwrap();
        let delivered = mailbox.0.borrow();
        assert_eq!(delivered.len(), 1);
        assert_eq!(
            delivered[0],
            (channel, "+15555550100".into(), "482916".into())
        );
        assert_eq!(pending.channel, channel);
        assert_eq!(check(&mut pending, &delivered[0].2, 101), Ok(()));
        assert_eq!(
            check(&mut pending, &delivered[0].2, 102),
            Err(Error::Rejected)
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_limits_and_inputs_never_send() {
    let mailbox = Mailbox::default();
    for policy in [
        VerificationLimits {
            code_lifetime_secs: 0,
            max_attempts: 1,
        },
        VerificationLimits {
            code_lifetime_secs: 1,
            max_attempts: 0,
        },
    ] {
        assert!(matches!(
            begin(&mailbox, &policy, "+15555550100", Channel::Sms, "1234", 0),
            Err(Error::Rejected)
        ));
    }
    assert!(matches!(
        begin(&mailbox, &limits(), "invalid", Channel::Sms, "1234", 0),
        Err(Error::InvalidPhone)
    ));
    assert!(matches!(
        begin(&mailbox, &limits(), "+15555550100", Channel::Sms, "bad", 0),
        Err(Error::Rejected)
    ));
    assert!(mailbox.0.borrow().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn provider_decline_and_uncertain_delivery_never_retry_or_switch() {
    struct Refusal<'a>(&'a Cell<u32>, bool);
    impl ChannelSender for Refusal<'_> {
        fn send(&self, channel: Channel, _e164: &str, _code: &str) -> Result<(), Error> {
            assert_eq!(channel, Channel::FlashCall);
            self.0.set(self.0.get() + 1);
            Err(if self.1 {
                Error::Declined
            } else {
                Error::Transport
            })
        }
    }
    for declined in [true, false] {
        let calls = Cell::new(0);
        let sender = Refusal(&calls, declined);
        let error = begin(
            &sender,
            &limits(),
            "+15555550100",
            Channel::FlashCall,
            "482916",
            100,
        )
        .unwrap_err();
        assert_eq!(
            error,
            if declined {
                Error::Declined
            } else {
                Error::Transport
            }
        );
        assert_eq!(calls.get(), 1);
    }
}
