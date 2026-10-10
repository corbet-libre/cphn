//! Trusted provider extension boundary; member JSON is never accepted as authenticated evidence.
use super::*;

/// Authenticated data supplied by a trusted adapter implementation, not a client input type.
/// Implementations must independently authenticate their provider and bind its ceremony/result.
#[derive(Debug)]
pub struct AdapterEvidence {
    /// Independently verified canonicalizable identifier, held only in RAM.
    pub identifier: Private,
    /// Subject/action/check binding authenticated or durably correlated by the adapter.
    pub binding: [u8; 32],
    /// Exclusive provider evidence expiry; caller cap can only shorten it.
    pub expires_at: u64,
    /// Complete upstream output if available; core returns it only on explicit request.
    pub upstream: Option<Private>,
}
/// Extend phone verification with independently authenticated provider implementations.
/// This is a trusted application-code port, not an endpoint accepting arbitrary JSON claims.
pub trait Adapter {
    /// The provider's transient input/proof/session reference type.
    type Input;
    /// Opaque request binding covering account, mode, context.operation, context and all relevant input.
    /// Use a keyed transform for low-entropy/private terms; never return raw input.
    fn binding(&self, input: &Self::Input, context: &Context) -> Result<String, Error>;
    /// Actual credential/endpoint mode. Simulation cannot produce production evidence.
    fn mode(&self) -> cost::Mode;
    /// Authenticate once, with no hidden retry, side effect or persistence outside the permit.
    /// Provider errors must remain uniform; charge uncertainty must remain explicit.
    /// Recheck the permit/context deadline with a fresh trusted clock after any
    /// internal queue and immediately before I/O. No hidden retry is authorized.
    fn authenticate(
        &self,
        input: Self::Input,
        context: &Context,
        permit: cost::DispatchPermit<usize>,
        now: u64,
    ) -> impl std::future::Future<Output = Result<AdapterEvidence, Error>>;
}
/// Authenticate an extension adapter and enforce the common privacy/identity transformation.
/// Network adapters need caller-authorized bounded costs; this function never discovers prices.
pub async fn verify_with<A: Adapter>(
    state: &impl DispatchState,
    adapter: &A,
    input: A::Input,
    context: Context,
    permit: cost::DispatchPermit<usize>,
    clock: &impl Fn() -> u64,
) -> Result<Verified, Error> {
    let now = clock();
    if context.scope.is_empty()
        || context.expires_at <= now
        || adapter.mode() != cost::Mode::Production
    {
        return Err(Error::Configuration);
    }
    let binding = adapter.binding(&input, &context)?;
    permit
        .validate(&binding, adapter.mode(), now)
        .map_err(|_| Error::Budget)?;
    state
        .reserve(
            &context.binding,
            &context.operation,
            &binding,
            permit.quote().price.maximum().ok_or(Error::Budget)?,
            permit.quote().valid_until.min(context.expires_at),
        )
        .map_err(|_| Error::Reservation)?;
    let deadline = permit.quote().valid_until.min(context.expires_at);
    let dispatched = clock();
    if dispatched < now {
        return Err(Error::Configuration);
    }
    if dispatched >= deadline {
        return Err(Error::NotDispatched);
    }
    let evidence = adapter
        .authenticate(input, &context, permit, dispatched)
        .await?;
    let completed = clock();
    let response = if context.include_upstream {
        evidence
            .upstream
            .as_ref()
            .map(|data| Private::new(data.expose().to_vec()))
    } else {
        None
    };
    let result = if completed < dispatched || completed >= context.expires_at {
        Err(Error::Uncertain)
    } else {
        project(evidence, context, completed)
    };
    result.map_err(|error| match response {
        Some(response) => Error::Reported {
            error: Box::new(error),
            response,
        },
        None => error,
    })
}

pub(crate) fn project(
    evidence: AdapterEvidence,
    context: Context,
    now: u64,
) -> Result<Verified, Error> {
    if evidence.binding != context.binding
        || evidence.expires_at <= now
        || context.expires_at <= now
        || context.scope.is_empty()
    {
        return Err(Error::Refused);
    }
    let raw = std::str::from_utf8(evidence.identifier.expose()).map_err(|_| Error::Refused)?;
    let canonical = Zeroizing::new(normalize(raw)?);
    let identity = Identity::derive(
        &context.identity_key,
        &context.scope,
        "phone.e164",
        &canonical,
    );
    let upstream = if context.include_upstream {
        evidence.upstream
    } else {
        None
    };
    Ok(Verified {
        identity,
        binding: context.binding,
        expires_at: context.expires_at.min(evidence.expires_at),
        upstream,
    })
}
