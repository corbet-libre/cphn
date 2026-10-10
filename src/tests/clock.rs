use super::*;
use std::cell::Cell;

struct WaitingState<'a> {
    clock: &'a Cell<u64>,
    journal: Journal,
    advance: u64,
}
impl DispatchState for WaitingState<'_> {
    fn reserve(
        &self,
        ceremony: &[u8; 32],
        operation: &[u8; 32],
        binding: &str,
        maximum: &cost::Amount,
        valid_until: u64,
    ) -> Result<(), Error> {
        self.journal
            .reserve(ceremony, operation, binding, maximum, valid_until)?;
        self.clock.set(self.advance);
        Ok(())
    }
}
struct WaitingWire<'a> {
    clock: &'a Cell<u64>,
    wire: Wire,
    completed: u64,
}
impl Transport for WaitingWire<'_> {
    fn mode(&self) -> cost::Mode {
        self.wire.mode()
    }
    async fn send(
        &self,
        request: http::Request<Private>,
    ) -> Result<http::Response<Private>, Error> {
        let response = self.wire.send(request).await;
        yield_once().await;
        self.clock.set(self.completed);
        response
    }
}
#[test]
fn deadlines_cover_reservation_reply_and_fallback_without_releasing_exposure() {
    let clock = Cell::new(100);
    let state = WaitingState {
        clock: &clock,
        journal: Journal::default(),
        advance: 200,
    };
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let wire = Wire::new(vec![]);
    assert_eq!(
        block_on(p.dispatch(&state, &wire, permit, &|| clock.get())).unwrap_err(),
        Error::NotDispatched
    );
    assert!(wire.requests.borrow().is_empty());
    assert_eq!(state.journal.inner.lock().unwrap().0.len(), 1);

    for completed in [99, 200, 301] {
        clock.set(100);
        let p = adapter()
            .prepare_check(pending(true), "123456", [4; 32], 100)
            .unwrap();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let wire = WaitingWire {
            clock: &clock,
            wire: Wire::new(vec![Ok(response(reply("approved")))]),
            completed,
        };
        let journal = Journal::default();
        let report = block_on(p.dispatch_report(&journal, &wire, permit, &|| clock.get()));
        if (100..300).contains(&completed) {
            assert!(matches!(report.result, Ok(Step::Verified(_))));
        } else {
            assert!(matches!(report.result, Err(Error::Uncertain)));
        }
        assert_eq!(
            report.response.unwrap().body().expose(),
            serde_json::to_vec(&reply("approved")).unwrap()
        );
        assert_eq!(journal.inner.lock().unwrap().0.len(), 1);
        assert_eq!(wire.wire.requests.borrow().len(), 1);
    }
    clock.set(100);
    let wire = WaitingWire {
        clock: &clock,
        wire: Wire::new(vec![Err(Error::NotDispatched)]),
        completed: 200,
    };
    let attempts = (0..2)
        .map(|index| {
            let mut c = context();
            c.operation = [index; 32];
            Attempt {
                prepared: adapter().prepare_begin(DEST, CHANNEL, c, 100).unwrap(),
                price: cost::Price::AtMost(money(10)),
                source: "fixture".into(),
                quote_valid_until: 200,
            }
        })
        .collect();
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        attempts,
        &wire,
        cost::CostOrder::Configured,
        money(100),
        2,
        &|| clock.get(),
    ))
    .unwrap();
    assert_eq!(run.result.unwrap_err(), Error::Budget);
    assert_eq!(run.attempts, 1);
    assert_eq!(wire.wire.requests.borrow().len(), 1);
    assert_eq!(run.exposure, money(10));
}
struct WaitingExtension<'a> {
    clock: &'a Cell<u64>,
    completed: u64,
    binding: String,
    bad_binding: bool,
}
impl Adapter for WaitingExtension<'_> {
    type Input = ();
    fn binding(&self, _: &(), _: &Context) -> Result<String, Error> {
        Ok(self.binding.clone())
    }
    fn mode(&self) -> cost::Mode {
        cost::Mode::Production
    }
    async fn authenticate(
        &self,
        _: (),
        _: &Context,
        _: cost::DispatchPermit<usize>,
        _: u64,
    ) -> Result<AdapterEvidence, Error> {
        yield_once().await;
        self.clock.set(self.completed);
        let mut result = evidence();
        if self.bad_binding {
            result.binding = [8; 32];
        }
        Ok(result)
    }
}
#[test]
fn extension_completion_preserves_requested_private_reply_without_an_expired_fact() {
    for (completed, bad_binding, rich) in [
        (300, false, true),
        (300, false, false),
        (150, true, true),
        (99, false, true),
        (200, false, true),
        (260, false, true),
    ] {
        let clock = Cell::new(100);
        let p = prepared();
        let permit = get_permit(&p, cost::Mode::Production, 200);
        let extension = WaitingExtension {
            clock: &clock,
            completed,
            binding: p.binding().into(),
            bad_binding,
        };
        let mut context = context();
        context.include_upstream = rich;
        let result = block_on(verify_with(
            &Journal::default(),
            &extension,
            (),
            context,
            permit,
            &|| clock.get(),
        ));
        if completed == 200 {
            assert!(result.is_ok());
            continue;
        }
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.upstream().is_some(), rich);
        if rich {
            assert_eq!(
                error.upstream().unwrap().expose(),
                b"complete upstream result"
            );
            assert!(
                matches!(&error, Error::Reported { error, .. } if **error == if bad_binding || completed == 260 { Error::Refused } else { Error::Uncertain })
            );
        } else {
            assert_eq!(error, Error::Uncertain);
        }
        assert!(!format!("{error:?} {error}").contains("complete upstream result"));
    }
    let clock = Cell::new(100);
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let extension = WaitingExtension {
        clock: &clock,
        completed: 100,
        binding: p.binding().into(),
        bad_binding: false,
    };
    let state = WaitingState {
        clock: &clock,
        journal: Journal::default(),
        advance: 200,
    };
    assert_eq!(
        block_on(verify_with(
            &state,
            &extension,
            (),
            context(),
            permit,
            &|| clock.get()
        ))
        .unwrap_err(),
        Error::NotDispatched
    );
    assert_eq!(clock.get(), 200);
}

async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}

#[test]
fn rollback_before_reservation_or_dispatch_never_reaches_transport() {
    let reads = Cell::new(0);
    let clock = || {
        let count = reads.replace(reads.get() + 1);
        if count == 0 { 100 } else { 99 }
    };
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let wire = Wire::new(vec![]);
    assert_eq!(
        block_on(p.dispatch(&Journal::default(), &wire, permit, &clock)).unwrap_err(),
        Error::Configuration
    );
    assert!(wire.requests.borrow().is_empty());
    reads.set(0);
    let attempt = Attempt {
        prepared: prepared(),
        price: cost::Price::AtMost(money(10)),
        source: "fixture".into(),
        quote_valid_until: 200,
    };
    let run = block_on(begin_with_fallback(
        &Journal::default(),
        vec![attempt],
        &wire,
        cost::CostOrder::Configured,
        money(100),
        1,
        &clock,
    ))
    .unwrap();
    assert_eq!(run.result.unwrap_err(), Error::Configuration);
    assert_eq!(run.attempts, 0);
    assert_eq!(run.exposure, money(0));
    let clock = Cell::new(100);
    let state = WaitingState {
        clock: &clock,
        journal: Journal::default(),
        advance: 99,
    };
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    assert_eq!(
        block_on(p.dispatch(&state, &wire, permit, &|| clock.get())).unwrap_err(),
        Error::Configuration
    );
    assert!(wire.requests.borrow().is_empty());
    clock.set(100);
    let state = WaitingState {
        clock: &clock,
        journal: Journal::default(),
        advance: 99,
    };
    let p = prepared();
    let permit = get_permit(&p, cost::Mode::Production, 200);
    let extension = WaitingExtension {
        clock: &clock,
        completed: 100,
        binding: p.binding().into(),
        bad_binding: false,
    };
    assert_eq!(
        block_on(verify_with(
            &state,
            &extension,
            (),
            context(),
            permit,
            &|| clock.get()
        ))
        .unwrap_err(),
        Error::Configuration
    );
    assert_eq!(clock.get(), 99);
}
