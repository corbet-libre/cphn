use super::*;
use cfbk::{Candidate, Config};
use std::collections::BTreeSet;

/// One explicitly configured provider/channel operation and its complete price basis.
pub struct Attempt {
    /// Real inert request, already bound to context and operation terms.
    pub prepared: Prepared,
    /// Exact cost or defensible cap including provider-managed delivery changes.
    pub price: cost::Price,
    /// Quote/tariff provenance; an empty or unknown basis cannot dispatch.
    pub source: String,
    /// Exclusive quote expiry.
    pub quote_valid_until: u64,
}
/// Cascade result. Exposure remains reserved for pending or uncertain operations.
#[derive(Debug)]
pub struct Run {
    /// Verified result, pending ceremony or explicit failure.
    pub result: Result<Step, Error>,
    /// Reserved upper-bound exposure, not a fabricated actual charge.
    pub exposure: cost::Amount,
    /// Number of cost-authorized attempts, including pre-dispatch refusals.
    pub attempts: usize,
}

/// Run caller-selected provider/channel options under one budget using shared `cfbk`.
/// Each compatible option proves phone control; no cheapest-channel assumption is baked in.
/// This convenience path never retries pending or uncertain effects. Preparing options is inert.
pub async fn begin_with_fallback(
    state: &impl DispatchState,
    attempts: Vec<Attempt>,
    transport: &impl Transport,
    order: cost::CostOrder,
    budget: cost::Amount,
    max_attempts: usize,
    clock: &impl Fn() -> u64,
) -> Result<Run, Error> {
    let mut now = clock();
    let offers = attempts
        .iter()
        .enumerate()
        .map(|(id, a)| cost::Offer {
            candidate: Candidate {
                id,
                properties: BTreeSet::from(["phone.control"]),
            },
            quote: cost::Quote {
                price: a.price.clone(),
                source: a.source.clone(),
                valid_until: a.quote_valid_until,
                binding: a.prepared.binding().into(),
            },
        })
        .collect();
    let config = Config {
        order: (0..attempts.len()).collect(),
        minimum: BTreeSet::from(["phone.control"]),
    };
    let plan = cost::Plan::new(
        offers,
        config,
        order,
        budget,
        max_attempts,
        transport.mode(),
        now,
    )
    .map_err(|_| Error::Budget)?;
    let mut cascade = plan.start();
    let mut attempts: Vec<_> = attempts.into_iter().map(Some).collect();
    loop {
        let current = clock();
        if current < now {
            return Ok(Run {
                result: Err(Error::Configuration),
                exposure: cascade.exposure(),
                attempts: cascade.attempts(),
            });
        }
        now = current;
        let permit = match cascade.begin(now) {
            Ok(permit) => permit,
            Err(_) => {
                return Ok(Run {
                    result: Err(Error::Budget),
                    exposure: cascade.exposure(),
                    attempts: cascade.attempts(),
                });
            }
        };
        let index = *permit.candidate();
        let attempt = attempts[index].take().ok_or(Error::Configuration)?;
        let result = attempt
            .prepared
            .dispatch_inner(state, transport, permit, clock, now, None)
            .await;
        let outcome = match &result {
            Ok(Step::Verified(_)) => cost::Outcome::Success,
            Ok(Step::Pending(_)) => cost::Outcome::Pending,
            Err(Error::NotDispatched) => cost::Outcome::SafeFailure,
            Err(Error::Uncertain | Error::ProviderResponse { .. }) => cost::Outcome::Uncertain,
            _ => cost::Outcome::Refused,
        };
        cascade.complete(outcome, None).map_err(|_| Error::Budget)?;
        if outcome != cost::Outcome::SafeFailure {
            return Ok(Run {
                result,
                exposure: cascade.exposure(),
                attempts: cascade.attempts(),
            });
        }
    }
}
