use super::*;
use futures::executor::block_on;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    num::NonZeroU32,
    sync::Mutex,
};
const DEST: &str = "+12025550123";
const CHANNEL: Channel = Channel::Sms;
fn account() -> String {
    format!("AC{}", "0".repeat(32))
}
fn service() -> String {
    format!("VA{}", "0".repeat(32))
}
fn session() -> String {
    format!("VE{}", "0".repeat(32))
}
fn adapter() -> Twilio {
    Twilio::new(
        account(),
        service(),
        Private::new(b"dummy-secret".to_vec()),
        cost::Mode::Production,
    )
    .unwrap()
}
fn context() -> Context {
    Context {
        binding: [1; 32],
        operation: [3; 32],
        scope: b"scope".to_vec(),
        identity_key: zeroize::Zeroizing::new([2; 32]),
        expires_at: 300,
        include_upstream: false,
    }
}
fn prepared() -> Prepared {
    adapter()
        .prepare_begin(DEST, CHANNEL, context(), 100)
        .unwrap()
}
fn money(atoms: u64) -> cost::Amount {
    cost::Amount {
        unit: cost::Unit {
            code: "USD".into(),
            scale: 2,
        },
        atoms,
    }
}
fn get_permit(p: &Prepared, mode: cost::Mode, expiry: u64) -> cost::DispatchPermit<usize> {
    cost::Plan::new(
        vec![cost::Offer {
            candidate: cfbk::Candidate::new(0, ["control"]),
            quote: cost::Quote {
                price: cost::Price::AtMost(money(10)),
                source: "fixture-tariff".into(),
                valid_until: expiry,
                binding: p.binding().into(),
            },
        }],
        cfbk::Config::new([0], ["control"]),
        cost::CostOrder::Configured,
        money(10),
        1,
        mode,
        100,
    )
    .unwrap()
    .start()
    .begin(100)
    .unwrap()
}
fn reply(status: &str) -> serde_json::Value {
    serde_json::json!({"sid":session(),"account_sid":account(),"service_sid":service(),"to":DEST,"channel":CHANNEL.as_str(),"status":status,"valid":status=="approved","extension":{"complete":"upstream"}})
}
fn response(v: serde_json::Value) -> http::Response<Private> {
    http::Response::new(Private::new(serde_json::to_vec(&v).unwrap()))
}
struct Wire {
    results: RefCell<VecDeque<Result<http::Response<Private>, Error>>>,
    requests: RefCell<Vec<http::Request<Private>>>,
    mode: cost::Mode,
}
impl Wire {
    fn new(results: Vec<Result<http::Response<Private>, Error>>) -> Self {
        Self {
            results: RefCell::new(results.into()),
            requests: RefCell::new(vec![]),
            mode: cost::Mode::Production,
        }
    }
}
impl Transport for Wire {
    fn mode(&self) -> cost::Mode {
        self.mode
    }
    async fn send(
        &self,
        request: http::Request<Private>,
    ) -> Result<http::Response<Private>, Error> {
        assert!(request.extensions().get::<DispatchDeadline>().is_some());
        self.requests.borrow_mut().push(request);
        self.results.borrow_mut().pop_front().unwrap()
    }
}
fn pending(rich: bool) -> Pending {
    let mut c = context();
    c.include_upstream = rich;
    let p = adapter().prepare_begin(DEST, CHANNEL, c, 100).unwrap();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    match block_on(p.dispatch(
        &Journal::default(),
        &Wire::new(vec![Ok(response(reply("pending")))]),
        permit,
        100,
    ))
    .unwrap()
    {
        Step::Pending(p) => p,
        _ => panic!("must be pending"),
    }
}
fn verified(rich: bool) -> Verified {
    let p = adapter()
        .prepare_check(pending(rich), "123456", [4; 32], 101)
        .unwrap();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    match block_on(p.dispatch(
        &Journal::default(),
        &Wire::new(vec![Ok(response(reply("approved")))]),
        permit,
        101,
    ))
    .unwrap()
    {
        Step::Verified(p) => p,
        _ => panic!("must be verified"),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn actual_twilio_wire_is_correlated_and_only_requested_rich_data_survives() {
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let wire = Wire::new(vec![Ok(response(reply("pending")))]);
    let step = block_on(p.dispatch(&Journal::default(), &wire, permit, 100)).unwrap();
    let Step::Pending(pending) = step else {
        panic!("pending")
    };
    assert!(!format!("{pending:?}").contains(DEST));
    let requests = wire.requests.borrow();
    let request = &requests[0];
    assert_eq!(request.method(), "POST");
    assert_eq!(request.uri().host(), Some("verify.twilio.com"));
    assert!(request.uri().path().ends_with("/Verifications"));
    assert!(request.headers()[http::header::AUTHORIZATION].is_sensitive());
    let form: HashMap<_, _> = url::form_urlencoded::parse(request.body().expose())
        .into_owned()
        .collect();
    assert_eq!(form["To"], DEST);
    assert_eq!(form["Channel"], CHANNEL.as_str());
    let p = adapter()
        .prepare_check(pending, "123456", [4; 32], 101)
        .unwrap();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let wire = Wire::new(vec![Ok(response(reply("approved")))]);
    let Step::Verified(v) = block_on(p.dispatch(&Journal::default(), &wire, permit, 101)).unwrap()
    else {
        panic!("verified")
    };
    assert!(
        wire.requests.borrow()[0]
            .uri()
            .path()
            .ends_with("/VerificationCheck")
    );
    let form: HashMap<_, _> =
        url::form_urlencoded::parse(wire.requests.borrow()[0].body().expose())
            .into_owned()
            .collect();
    assert_eq!(form["VerificationSid"], session());
    assert_eq!(form["Code"], "123456");
    assert!(!form.contains_key("To"));
    assert_eq!(v.binding(), &[1; 32]);
    assert_eq!(v.expires_at(), 300);
    assert!(v.upstream().is_none());
    assert!(!format!("{v:?}").contains(DEST));
    let rich = verified(true);
    assert_eq!(rich.identity(), v.identity());
    let value: serde_json::Value =
        serde_json::from_slice(rich.upstream().unwrap().expose()).unwrap();
    assert_eq!(value["extension"]["complete"], "upstream");
    assert_eq!(value["to"], DEST);
    assert!(!format!("{rich:?}").contains(DEST));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn authentication_response_and_session_mismatches_never_create_a_fact() {
    for (field, value) in [
        ("account_sid", serde_json::json!("wrong")),
        ("service_sid", serde_json::json!("wrong")),
        ("to", serde_json::json!("wrong")),
        ("channel", serde_json::json!("unsupported")),
        ("sid", serde_json::json!("invalid")),
        ("sid", serde_json::json!(format!("VE{}", "1".repeat(32)))),
        ("status", serde_json::json!("rejected")),
        ("valid", serde_json::json!(false)),
    ] {
        let mut r = reply("approved");
        r[field] = value;
        let p = adapter()
            .prepare_check(pending(false), "123456", [4; 32], 101)
            .unwrap();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        assert_eq!(
            block_on(p.dispatch(
                &Journal::default(),
                &Wire::new(vec![Ok(response(r))]),
                permit,
                101
            ))
            .unwrap_err(),
            Error::Refused
        );
    }
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    assert_eq!(
        block_on(p.dispatch(
            &Journal::default(),
            &Wire::new(vec![Ok(response(reply("approved")))]),
            permit,
            101
        ))
        .unwrap_err(),
        Error::Refused
    );
    let p = adapter()
        .prepare_check(pending(false), "123456", [4; 32], 101)
        .unwrap();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    assert!(matches!(
        block_on(p.dispatch(
            &Journal::default(),
            &Wire::new(vec![Ok(response(reply("pending")))]),
            permit,
            101
        )),
        Ok(Step::Pending(_))
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_responses_and_possible_effects_fail_closed_without_retry() {
    for result in [
        Ok(http::Response::builder()
            .status(500)
            .body(Private::new(vec![]))
            .unwrap()),
        Ok(http::Response::new(Private::new(vec![b'x'; 65537]))),
        Ok(http::Response::new(Private::new(b"malformed".to_vec()))),
        Err(Error::Uncertain),
        Err(Error::NotDispatched),
    ] {
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let wire = Wire::new(vec![result]);
        assert!(block_on(p.dispatch(&Journal::default(), &wire, permit, 101)).is_err());
        assert_eq!(wire.requests.borrow().len(), 1);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn credentials_inputs_scope_time_and_modes_are_validated_before_transport() {
    for (a, s, secret, mode) in [
        (
            "invalid".into(),
            service(),
            b"secret".to_vec(),
            cost::Mode::Production,
        ),
        (
            account(),
            "invalid".into(),
            b"secret".to_vec(),
            cost::Mode::Production,
        ),
        (account(), service(), vec![], cost::Mode::Production),
        (
            account(),
            service(),
            b"secret".to_vec(),
            cost::Mode::Sandbox,
        ),
        (
            account(),
            service(),
            b"secret".to_vec(),
            cost::Mode::Simulation,
        ),
        (
            format!("AC{}", "z".repeat(32)),
            service(),
            b"secret".to_vec(),
            cost::Mode::Production,
        ),
    ] {
        assert!(Twilio::new(a, s, Private::new(secret), mode).is_err());
    }
    assert!(
        adapter()
            .prepare_begin("invalid", CHANNEL, context(), 100)
            .is_err()
    );
    let mut c = context();
    c.scope.clear();
    assert!(adapter().prepare_begin(DEST, CHANNEL, c, 100).is_err());
    assert!(
        adapter()
            .prepare_begin(DEST, CHANNEL, context(), 300)
            .is_err()
    );
    let invalid = Twilio::new(
        account(),
        service(),
        Private::new(vec![255]),
        cost::Mode::Production,
    )
    .unwrap();
    assert!(
        invalid
            .prepare_begin(DEST, CHANNEL, context(), 100)
            .is_err()
    );
    for code in ["123", "12345678901", "123x"] {
        assert!(
            adapter()
                .prepare_check(pending(false), code, [4; 32], 101)
                .is_err()
        );
    }
    let other = Twilio::new(
        format!("AC{}", "1".repeat(32)),
        service(),
        Private::new(b"secret".to_vec()),
        cost::Mode::Production,
    )
    .unwrap();
    assert!(
        other
            .prepare_check(pending(false), "123456", [4; 32], 101)
            .is_err()
    );
    let other = Twilio::new(
        account(),
        format!("VA{}", "1".repeat(32)),
        Private::new(b"secret".to_vec()),
        cost::Mode::Production,
    )
    .unwrap();
    assert!(
        other
            .prepare_check(pending(false), "123456", [4; 32], 101)
            .is_err()
    );
    for mode in [cost::Mode::Simulation, cost::Mode::Sandbox] {
        let p = prepared();
        let permit = get_permit(&p, mode, 200);
        let wire = Wire::new(vec![]);
        assert_eq!(
            block_on(p.dispatch(&Journal::default(), &wire, permit, 101)).unwrap_err(),
            Error::Budget
        );
        assert!(wire.requests.borrow().is_empty());
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let mut wire = Wire::new(vec![]);
        wire.mode = mode;
        assert_eq!(
            block_on(p.dispatch(&Journal::default(), &wire, permit, 101)).unwrap_err(),
            Error::Configuration
        );
    }
    for expired in [200, 300] {
        let p = prepared();
        let permit = get_permit(
            &p,
            cost::Mode::Production,
            if expired == 200 { 200 } else { 400 },
        );
        assert!(
            block_on(p.dispatch(&Journal::default(), &Wire::new(vec![]), permit, expired)).is_err()
        );
    }
    let p = prepared();
    let wrong = adapter()
        .prepare_begin(
            DEST,
            CHANNEL,
            Context {
                binding: [9; 32],
                ..context()
            },
            100,
        )
        .unwrap();
    let permit = get_permit(&wrong, cost::Mode::Production, 200);
    assert_eq!(
        block_on(p.dispatch(&Journal::default(), &Wire::new(vec![]), permit, 101)).unwrap_err(),
        Error::Budget
    );
    for error in [
        Error::Input,
        Error::Configuration,
        Error::Refused,
        Error::NotDispatched,
        Error::Uncertain,
        Error::Budget,
    ] {
        assert!(!error.to_string().contains(DEST));
    }
}

fn attempt(amount: u64) -> Attempt {
    Attempt {
        prepared: adapter()
            .prepare_begin(
                DEST,
                CHANNEL,
                Context {
                    operation: [u8::try_from(amount).unwrap(); 32],
                    ..context()
                },
                100,
            )
            .unwrap(),
        price: cost::Price::AtMost(money(amount)),
        source: "fixture-tariff".into(),
        quote_valid_until: 200,
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn configured_and_cost_order_cascades_reserve_failed_costs_and_stop_on_pending() {
    let wire = Wire::new(vec![
        Err(Error::NotDispatched),
        Ok(response(reply("pending"))),
    ]);
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        vec![attempt(10), attempt(20)],
        &wire,
        cost::CostOrder::Configured,
        money(30),
        2,
        100,
    ))
    .unwrap();
    assert!(matches!(run.result, Ok(Step::Pending(_))));
    assert_eq!(run.attempts, 2);
    assert_eq!(run.exposure.atoms, 30);
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        vec![attempt(20), attempt(10)],
        &Wire::new(vec![Ok(response(reply("pending")))]),
        cost::CostOrder::Cheapest,
        money(30),
        2,
        100,
    ))
    .unwrap();
    assert_eq!(run.exposure.atoms, 10);
    assert_eq!(run.attempts, 1);
    assert!(format!("{run:?}").contains("Run"));
    for e in [Error::Uncertain, Error::Refused] {
        let run = block_on(begin_with_fallback(
            &Journal::default(),
            vec![attempt(10), attempt(20)],
            &Wire::new(vec![Err(e)]),
            cost::CostOrder::Configured,
            money(30),
            2,
            100,
        ))
        .unwrap();
        assert_eq!(run.attempts, 1);
        assert_eq!(run.exposure.atoms, 10);
    }
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        vec![attempt(10)],
        &Wire::new(vec![Err(Error::NotDispatched)]),
        cost::CostOrder::Configured,
        money(10),
        1,
        100,
    ))
    .unwrap();
    assert_eq!(run.result.unwrap_err(), Error::Budget);
    let mut unknown = attempt(10);
    unknown.price = cost::Price::Unknown;
    assert!(
        block_on(begin_with_fallback(
            &Journal::default(),
            vec![unknown],
            &Wire::new(vec![]),
            cost::CostOrder::Configured,
            money(10),
            1,
            100
        ))
        .is_err()
    );
    let checked = adapter()
        .prepare_check(pending(false), "123456", [4; 32], 101)
        .unwrap();
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        vec![Attempt {
            prepared: checked,
            ..attempt(10)
        }],
        &Wire::new(vec![Ok(response(reply("approved")))]),
        cost::CostOrder::Configured,
        money(10),
        1,
        101,
    ))
    .unwrap();
    assert!(matches!(run.result, Ok(Step::Verified(_))));
}

type Registrations = HashMap<(Identity, Vec<u8>), Vec<Vec<u8>>>;
#[derive(Default)]
struct State(Mutex<Registrations>);
impl RegistrationState for State {
    fn reserve(&self, identity: &Identity, r: Registration<'_>) -> Result<Reservation, StateError> {
        let mut state = self.0.lock().unwrap();
        let entries = state
            .entry((identity.clone(), r.window.to_vec()))
            .or_default();
        if entries.iter().any(|id| id == r.id) {
            return Ok(Reservation::AlreadyReserved);
        }
        if entries.len() >= r.limit.get() as usize {
            return Ok(Reservation::LimitReached);
        }
        entries.push(r.id.to_vec());
        Ok(Reservation::Reserved)
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn caller_state_enforces_general_limits_and_idempotency_without_a_library_database() {
    let v = verified(false);
    let state = State::default();
    let r = Registration {
        id: b"first",
        window: b"caller-window",
        limit: NonZeroU32::new(2).unwrap(),
    };
    assert_eq!(v.reserve(&state, r, 101), Ok(Reservation::Reserved));
    assert_eq!(v.reserve(&state, r, 101), Ok(Reservation::AlreadyReserved));
    assert_eq!(
        v.reserve(&state, Registration { id: b"second", ..r }, 101),
        Ok(Reservation::Reserved)
    );
    assert_eq!(
        v.reserve(&state, Registration { id: b"third", ..r }, 101),
        Ok(Reservation::LimitReached)
    );
    assert_eq!(
        v.reserve(
            &state,
            Registration {
                window: b"different-window",
                ..r
            },
            101
        ),
        Ok(Reservation::Reserved)
    );
    assert_eq!(v.reserve(&state, r, 300), Err(StateError));
    struct Down;
    impl RegistrationState for Down {
        fn reserve(&self, _: &Identity, _: Registration<'_>) -> Result<Reservation, StateError> {
            Err(StateError)
        }
    }
    assert_eq!(v.reserve(&Down, r, 101), Err(StateError));
    assert!(!StateError.to_string().is_empty());
    assert!(format!("{r:?}").contains("Registration"));
    assert_ne!(
        Identity::derive(&[2; 32], b"scope", "domain", "id"),
        Identity::derive(&[2; 32], b"other", "domain", "id")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn maintained_phone_metadata_and_all_channels_are_exercised() {
    assert_eq!(normalize("+1 (202) 555-0123").unwrap(), DEST);
    for invalid in ["invalid", "+1", "+12025550123 ext 1"] {
        assert!(normalize(invalid).is_err());
    }
    for (channel, wire) in [
        (Channel::Sms, "sms"),
        (Channel::Sms, "rcs"),
        (Channel::Whatsapp, "whatsapp"),
        (Channel::Whatsapp, "sms"),
        (Channel::Call, "call"),
    ] {
        let p = adapter()
            .prepare_begin(DEST, channel, context(), 100)
            .unwrap();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let mut r = reply("pending");
        r["channel"] = serde_json::json!(wire);
        assert!(matches!(
            block_on(p.dispatch(
                &Journal::default(),
                &Wire::new(vec![Ok(response(r))]),
                permit,
                101
            )),
            Ok(Step::Pending(_))
        ));
    }
}

#[test]
fn local_simulation_uses_real_planning_and_transformations_without_credentials_or_transport() {
    fn offers() -> Vec<SimulationOffer> {
        vec![
            SimulationOffer {
                channel: CHANNEL,
                price: cost::Price::AtMost(money(10)),
                source: "dummy-tariff".into(),
                valid_until: 200,
                outcome: cost::Outcome::SafeFailure,
            },
            SimulationOffer {
                channel: CHANNEL,
                price: cost::Price::AtMost(money(20)),
                source: "dummy-tariff".into(),
                valid_until: 200,
                outcome: cost::Outcome::Success,
            },
        ]
    }
    fn settings() -> SimulationSettings {
        SimulationSettings {
            order: cost::CostOrder::Configured,
            budget: money(30),
            max_attempts: 2,
            now: 100,
        }
    }
    let result = simulate(DEST, b"scope", offers(), settings()).unwrap();
    assert_eq!(result.attempts, 2);
    assert_eq!(result.exposure.atoms, 30);
    assert_eq!(result.outcome, cost::Outcome::Success);
    let other = simulate(DEST, b"different-scope", offers(), settings()).unwrap();
    assert_ne!(result.comparison, other.comparison);
    assert!(format!("{result:?}").contains("Simulated"));
    for outcome in [
        cost::Outcome::Pending,
        cost::Outcome::Uncertain,
        cost::Outcome::Refused,
    ] {
        let mut values = offers();
        values[0].outcome = outcome;
        assert_eq!(
            simulate(DEST, b"scope", values, settings())
                .unwrap()
                .attempts,
            1
        );
    }
    assert!(simulate("invalid", b"scope", offers(), settings()).is_err());
    assert!(simulate(DEST, b"", offers(), settings()).is_err());
    let mut unknown = offers();
    unknown
        .iter_mut()
        .for_each(|o| o.price = cost::Price::Unknown);
    assert!(simulate(DEST, b"scope", unknown, settings()).is_err());
    let mut short = settings();
    short.max_attempts = 1;
    assert!(simulate(DEST, b"scope", offers(), short).is_err());
    assert!(format!("{:?}", offers()).contains("SimulationOffer"));
    assert!(format!("{:?}", settings()).contains("SimulationSettings"));
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn concurrent_registration_calls_use_the_atomic_host_port() {
    let verified = verified(false);
    let state = State::default();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = (0..6u8)
            .map(|id| {
                let v = &verified;
                let s = &state;
                scope.spawn(move || {
                    v.reserve(
                        s,
                        Registration {
                            id: &[id],
                            window: b"window",
                            limit: NonZeroU32::new(2).unwrap(),
                        },
                        101,
                    )
                    .unwrap()
                })
            })
            .collect();
        let results: Vec<_> = tasks.into_iter().map(|task| task.join().unwrap()).collect();
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Reservation::Reserved)
                .count(),
            2
        );
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Reservation::LimitReached)
                .count(),
            4
        );
    });
}

#[test]
fn malformed_private_session_and_wrong_sid_prefix_are_refused() {
    let mut p = pending(false);
    p.sid = "invalid".into();
    assert!(adapter().prepare_check(p, "123456", [4; 32], 101).is_err());
    assert!(
        Twilio::new(
            format!("XX{}", "0".repeat(32)),
            service(),
            Private::new(b"secret".to_vec()),
            cost::Mode::Production
        )
        .is_err()
    );
}

#[test]
fn impossible_national_number_is_not_a_canonical_identity() {
    assert!(normalize("+1202555").is_err());
}

struct Extension {
    mode: cost::Mode,
    result: RefCell<Option<Result<AdapterEvidence, Error>>>,
    binding: Result<String, Error>,
}
impl Adapter for Extension {
    type Input = ();
    fn mode(&self) -> cost::Mode {
        self.mode
    }
    fn binding(&self, _: &(), _: &Context) -> Result<String, Error> {
        self.binding
            .as_ref()
            .cloned()
            .map_err(|_| Error::Configuration)
    }
    async fn authenticate(
        &self,
        _: (),
        _: &Context,
        _: cost::DispatchPermit<usize>,
        _: u64,
    ) -> Result<AdapterEvidence, Error> {
        self.result.borrow_mut().take().unwrap()
    }
}
fn evidence() -> AdapterEvidence {
    AdapterEvidence {
        identifier: Private::new(DEST.as_bytes().to_vec()),
        binding: [1; 32],
        expires_at: 250,
        upstream: Some(Private::new(b"complete upstream result".to_vec())),
    }
}
#[test]
fn open_adapter_port_projects_only_authenticated_scoped_requested_data() {
    for rich in [false, true] {
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let extension = Extension {
            mode: cost::Mode::Production,
            result: RefCell::new(Some(Ok(evidence()))),
            binding: Ok(p.binding().into()),
        };
        let mut c = context();
        c.include_upstream = rich;
        let verified = block_on(verify_with(
            &Journal::default(),
            &extension,
            (),
            c,
            permit,
            101,
        ))
        .unwrap();
        assert_eq!(verified.expires_at(), 250);
        assert_eq!(verified.upstream().is_some(), rich);
        assert_eq!(verified.identity(), self::verified(false).identity());
    }
    for case in 0..5 {
        let mut e = evidence();
        match case {
            0 => e.binding = [9; 32],
            1 => e.expires_at = 100,
            2 => e.identifier = Private::new(vec![255]),
            3 => e.identifier = Private::new(b"invalid".to_vec()),
            _ => {}
        }
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let result = if case == 4 {
            Err(Error::Uncertain)
        } else {
            Ok(e)
        };
        let extension = Extension {
            mode: cost::Mode::Production,
            result: RefCell::new(Some(result)),
            binding: Ok(p.binding().into()),
        };
        assert!(
            block_on(verify_with(
                &Journal::default(),
                &extension,
                (),
                context(),
                permit,
                101
            ))
            .is_err()
        );
    }
    for case in 0..5 {
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let mut c = context();
        let mut extension = Extension {
            mode: cost::Mode::Production,
            result: RefCell::new(None),
            binding: Ok(p.binding().into()),
        };
        match case {
            0 => c.scope.clear(),
            1 => c.expires_at = 100,
            2 => extension.mode = cost::Mode::Simulation,
            3 => extension.binding = Ok("wrong".into()),
            _ => extension.binding = Err(Error::Configuration),
        }
        assert!(
            block_on(verify_with(
                &Journal::default(),
                &extension,
                (),
                c,
                permit,
                101
            ))
            .is_err()
        );
    }
    for case in 0..2 {
        let mut c = context();
        if case == 0 {
            c.scope.clear();
        } else {
            c.expires_at = 100;
        }
        assert!(adapters::project(evidence(), c, 101).is_err());
    }
    assert!(!format!("{:?}", evidence()).contains(DEST));
}

type DispatchRecords = HashMap<[u8; 32], ([u8; 32], String, cost::Amount)>;
struct Journal {
    inner: Mutex<(DispatchRecords, HashMap<[u8; 32], u64>)>,
    limit: u64,
}
impl Default for Journal {
    fn default() -> Self {
        Self {
            inner: Mutex::new((HashMap::new(), HashMap::new())),
            limit: 100,
        }
    }
}
impl DispatchState for Journal {
    fn reserve(
        &self,
        ceremony: &[u8; 32],
        operation: &[u8; 32],
        binding: &str,
        maximum: &cost::Amount,
        valid_until: u64,
    ) -> Result<(), Error> {
        if valid_until <= 101 {
            return Err(Error::Reservation);
        }
        let mut state = self.inner.lock().unwrap();
        if state.0.contains_key(operation) || maximum.unit != money(0).unit {
            return Err(Error::Reservation);
        }
        let used = state.1.get(ceremony).copied().unwrap_or(0);
        let total = used.checked_add(maximum.atoms).ok_or(Error::Reservation)?;
        if total > self.limit {
            return Err(Error::Reservation);
        }
        state
            .0
            .insert(*operation, (*ceremony, binding.into(), maximum.clone()));
        state.1.insert(*ceremony, total);
        Ok(())
    }
}
#[test]
fn host_dispatch_reservation_blocks_uncertain_retries_conflicting_terms_and_budget_reuse() {
    let journal = Journal {
        limit: 10,
        ..Journal::default()
    };
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let wire = Wire::new(vec![Err(Error::Uncertain)]);
    assert_eq!(
        block_on(p.dispatch(&journal, &wire, permit, 101)).unwrap_err(),
        Error::Uncertain
    );
    assert_eq!(wire.requests.borrow().len(), 1);
    for changed in [false, true] {
        let mut c = context();
        if changed {
            c.scope = b"changed terms".to_vec();
        }
        let p = adapter().prepare_begin(DEST, CHANNEL, c, 100).unwrap();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let wire = Wire::new(vec![]);
        assert_eq!(
            block_on(p.dispatch(&journal, &wire, permit, 101)).unwrap_err(),
            Error::Reservation
        );
        assert!(wire.requests.borrow().is_empty());
    }
    let mut c = context();
    c.operation = [9; 32];
    let p = adapter().prepare_begin(DEST, CHANNEL, c, 100).unwrap();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let wire = Wire::new(vec![]);
    assert_eq!(
        block_on(p.dispatch(&journal, &wire, permit, 101)).unwrap_err(),
        Error::Reservation
    );
    assert!(wire.requests.borrow().is_empty());
    let state = journal.inner.lock().unwrap();
    assert_eq!(state.0.len(), 1);
    assert_eq!(state.1[&context().binding], 10);
}
#[test]
fn concurrent_prepared_requests_acquire_only_one_dispatch_reservation() {
    let journal = Journal::default();
    let counts = std::thread::scope(|scope| {
        let jobs = (0..4)
            .map(|_| {
                let journal = &journal;
                scope.spawn(move || {
                    let p = prepared();
                    let permit = get_permit(&p, cost::Mode::Production, 200);
                    let wire = Wire::new(vec![Ok(response(reply("pending")))]);
                    let accepted = block_on(p.dispatch(journal, &wire, permit, 101)).is_ok();
                    (usize::from(accepted), wire.requests.borrow().len())
                })
            })
            .collect::<Vec<_>>();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
    });
    assert_eq!(counts, (1, 1));
}
#[test]
fn extension_cannot_dispatch_after_host_reservation_refusal() {
    let journal = Journal {
        limit: 0,
        ..Journal::default()
    };
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let extension = Extension {
        mode: cost::Mode::Production,
        result: RefCell::new(Some(Ok(evidence()))),
        binding: Ok(p.binding().into()),
    };
    assert_eq!(
        block_on(verify_with(
            &journal,
            &extension,
            (),
            context(),
            permit,
            101
        ))
        .unwrap_err(),
        Error::Reservation
    );
    assert!(extension.result.borrow().is_some());
}

#[test]
fn upstream_http_failure_bytes_are_explicit_private_and_stop_fallback() {
    let body = br#"{"code":20429,"message":"private-provider-detail","extension":42}"#;
    let response = http::Response::builder()
        .status(429)
        .body(Private::new(body.to_vec()))
        .unwrap();
    let wire = Wire::new(vec![Ok(response)]);
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        vec![attempt(10), attempt(20)],
        &wire,
        cost::CostOrder::Configured,
        money(30),
        2,
        100,
    ))
    .unwrap();
    let error = run.result.unwrap_err();
    assert_eq!(error.upstream().unwrap().expose(), body);
    assert!(!format!("{error:?} {error}").contains("private-provider-detail"));
    assert!(Error::Uncertain.upstream().is_none());
    assert_eq!(wire.requests.borrow().len(), 1);
    assert_eq!(run.exposure.atoms, 10);
}

#[test]
fn delayed_fallback_rechecks_expiry_at_the_host_reservation_edge() {
    struct Clock(std::cell::Cell<u64>);
    impl DispatchState for Clock {
        fn reserve(
            &self,
            _: &[u8; 32],
            _: &[u8; 32],
            _: &str,
            _: &cost::Amount,
            valid_until: u64,
        ) -> Result<(), Error> {
            if self.0.get() >= valid_until {
                Err(Error::Reservation)
            } else {
                Ok(())
            }
        }
    }
    struct Delayed<'a> {
        clock: &'a Clock,
        calls: std::cell::Cell<usize>,
    }
    impl Transport for Delayed<'_> {
        fn mode(&self) -> cost::Mode {
            cost::Mode::Production
        }
        async fn send(&self, _: http::Request<Private>) -> Result<http::Response<Private>, Error> {
            self.calls.set(self.calls.get() + 1);
            self.clock.0.set(201);
            Err(Error::NotDispatched)
        }
    }
    let clock = Clock(std::cell::Cell::new(100));
    let wire = Delayed {
        clock: &clock,
        calls: std::cell::Cell::new(0),
    };
    let run = block_on(begin_with_fallback(
        &clock,
        vec![attempt(10), attempt(20)],
        &wire,
        cost::CostOrder::Configured,
        money(30),
        2,
        100,
    ))
    .unwrap();
    assert_eq!(run.result.unwrap_err(), Error::Reservation);
    assert_eq!(wire.calls.get(), 1);
}

#[test]
fn complete_report_preserves_bounded_pending_refused_and_malformed_responses() {
    let mut wrong = reply("pending");
    wrong["service_sid"] = serde_json::json!(format!("VA{}", "1".repeat(32)));
    let cases = [
        (200, serde_json::to_vec(&reply("pending")).unwrap(), true),
        (200, serde_json::to_vec(&wrong).unwrap(), false),
        (200, b"private-malformed-body".to_vec(), false),
        (403, b"private-error-body".to_vec(), false),
    ];
    for (status, body, accepted) in cases {
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let response = http::Response::builder()
            .status(status)
            .header("x-provider", "private-extension")
            .body(Private::new(body.clone()))
            .unwrap();
        let wire = Wire::new(vec![Ok(response)]);
        let report = block_on(p.dispatch_report(&Journal::default(), &wire, permit, 100));
        assert_eq!(report.result.is_ok(), accepted);
        let raw = report.response.unwrap();
        assert_eq!(raw.status().as_u16(), status);
        assert_eq!(raw.headers()["x-provider"], "private-extension");
        assert!(!format!("{raw:?}").contains("private-extension"));
        assert_eq!(raw.body().expose(), body);
        assert_eq!(wire.requests.borrow().len(), 1);
    }
    for result in [
        Err(Error::Uncertain),
        Ok(http::Response::new(Private::new(vec![0; 65537]))),
    ] {
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let report =
            block_on(p.dispatch_report(&Journal::default(), &Wire::new(vec![result]), permit, 100));
        assert!(report.result.is_err());
        assert!(report.response.is_none());
    }
}

#[test]
fn transport_receives_effective_deadline_and_refuses_queued_expired_work() {
    struct Queued;
    impl Transport for Queued {
        fn mode(&self) -> cost::Mode {
            cost::Mode::Production
        }
        async fn send(
            &self,
            request: http::Request<Private>,
        ) -> Result<http::Response<Private>, Error> {
            let deadline = request.extensions().get::<DispatchDeadline>().unwrap();
            assert_eq!(deadline.valid_until, 150);
            let fresh_time_after_queue = 151;
            assert!(fresh_time_after_queue >= deadline.valid_until);
            Err(Error::NotDispatched)
        }
    }
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 150);
    assert_eq!(
        block_on(p.dispatch(&Journal::default(), &Queued, permit, 100)).unwrap_err(),
        Error::NotDispatched
    );
}
