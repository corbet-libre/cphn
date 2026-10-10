//! Deterministic local planning and identity transformations without transport or credentials.
use super::*;

/// A dummy provider outcome and caller-supplied hypothetical price basis.
#[derive(Debug)]
pub struct SimulationOffer {
    /// Explicit compatible channel; no implicit cheapest-channel assumption.
    pub channel: Channel,
    /// Hypothetical exact price or cap; unknown remains unusable.
    pub price: cost::Price,
    /// Price provenance supplied by the simulation caller.
    pub source: String,
    /// Exclusive quote expiry.
    pub valid_until: u64,
    /// Explicit dummy outcome, never authenticated production evidence.
    pub outcome: cost::Outcome,
}
/// Caller-selected limits for deterministic simulation.
#[derive(Debug)]
pub struct SimulationSettings {
    /// Configured order or compare actual supplied compatible prices.
    pub order: cost::CostOrder,
    /// Whole-cascade budget, preserving denomination and scale.
    pub budget: cost::Amount,
    /// Maximum attempts.
    pub max_attempts: usize,
    /// Caller-supplied clock; simulation never reads the system clock.
    pub now: u64,
}
/// Simulation-only result. It cannot be passed as `Verified` or to its registration API.
#[derive(Debug)]
pub struct Simulated {
    /// Opaque dummy comparison bytes derived using an explicit simulation domain.
    pub comparison: [u8; 32],
    /// Reserved upper-bound cost, not an actual provider charge.
    pub exposure: cost::Amount,
    /// Number of hypothetical attempts.
    pub attempts: usize,
    /// Final explicit dummy outcome.
    pub outcome: cost::Outcome,
}
/// Exercise maintained normalization, scoped transformation and real fallback planning locally.
/// No credentials or transport can be supplied; no production proof can be constructed.
pub fn simulate(
    destination: &str,
    scope: &[u8],
    offers: Vec<SimulationOffer>,
    settings: SimulationSettings,
) -> Result<Simulated, Error> {
    let canonical = normalize(destination)?;
    if scope.is_empty() {
        return Err(Error::Configuration);
    }
    let comparison = *Identity::derive(&[0; 32], scope, "phone.simulation", &canonical).as_bytes();
    let planned = offers
        .iter()
        .enumerate()
        .map(|(id, o)| cost::Offer {
            candidate: cfbk::Candidate::new(id, ["phone.control"]),
            quote: cost::Quote {
                price: o.price.clone(),
                source: o.source.clone(),
                valid_until: o.valid_until,
                binding: format!("simulation:{}:{id}", o.channel.as_str()),
            },
        })
        .collect();
    let plan = cost::Plan::new(
        planned,
        cfbk::Config::new(0..offers.len(), ["phone.control"]),
        settings.order,
        settings.budget,
        settings.max_attempts,
        cost::Mode::Simulation,
        settings.now,
    )
    .map_err(|_| Error::Budget)?;
    let mut cascade = plan.start();
    loop {
        let permit = cascade.begin(settings.now).map_err(|_| Error::Budget)?;
        let outcome = offers[*permit.candidate()].outcome;
        cascade.complete(outcome, None).map_err(|_| Error::Budget)?;
        if outcome != cost::Outcome::SafeFailure {
            return Ok(Simulated {
                comparison,
                exposure: cascade.exposure(),
                attempts: cascade.attempts(),
                outcome,
            });
        }
    }
}
