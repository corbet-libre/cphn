use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;

/// Provider channel selected explicitly by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// SMS; the provider may upgrade delivery to RCS under service settings.
    Sms,
    /// WhatsApp; provider service settings may allow SMS fallback.
    Whatsapp,
    /// Voice call.
    Call,
}
impl Channel {
    /// Twilio's exact wire channel name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::Whatsapp => "whatsapp",
            Self::Call => "call",
        }
    }
    fn accepts(self, actual: &str) -> bool {
        match self {
            Self::Sms => matches!(actual, "sms" | "rcs"),
            Self::Whatsapp => matches!(actual, "whatsapp" | "sms"),
            Self::Call => actual == "call",
        }
    }
}

/// One explicitly configured Twilio Verify account and service.
/// No account lookup, credential discovery, retries or hidden SDK persistence.
pub struct Twilio {
    account: String,
    service: String,
    secret: Private,
}
impl Twilio {
    /// Production credentials only. Twilio Verify has no supported test-credential sandbox
    /// in this adapter; trials can charge and must not be labelled simulation.
    pub fn new(
        account: String,
        service: String,
        secret: Private,
        mode: cost::Mode,
    ) -> Result<Self, Error> {
        if !sid(&account, "AC")
            || !sid(&service, "VA")
            || secret.expose().is_empty()
            || mode != cost::Mode::Production
        {
            return Err(Error::Configuration);
        }
        Ok(Self {
            account,
            service,
            secret,
        })
    }
    /// Prepare one verification start without I/O. The resulting binding is used in a quote.
    pub fn prepare_begin(
        &self,
        destination: &str,
        channel: Channel,
        context: Context,
        now: u64,
    ) -> Result<Prepared, Error> {
        let canonical = Zeroizing::new(normalize(destination)?);
        let pending = Pending {
            canonical,
            sid: String::new(),
            account: self.account.clone(),
            service: self.service.clone(),
            channel,
            context,
        };
        self.prepare(pending, None, now)
    }
    /// Prepare one check for this exact RAM-held session; never accepts an arbitrary SID.
    pub fn prepare_check(
        &self,
        mut pending: Pending,
        code: &str,
        operation: [u8; 32],
        now: u64,
    ) -> Result<Prepared, Error> {
        if !(4..=10).contains(&code.len())
            || !code.bytes().all(|b| b.is_ascii_digit())
            || !sid(&pending.sid, "VE")
        {
            return Err(Error::Input);
        }
        pending.context.operation = operation;
        self.prepare(pending, Some(code), now)
    }
    fn prepare(&self, pending: Pending, code: Option<&str>, now: u64) -> Result<Prepared, Error> {
        if pending.account != self.account
            || pending.service != self.service
            || pending.context.scope.is_empty()
            || now >= pending.context.expires_at
        {
            return Err(Error::Configuration);
        }
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        let operation = if let Some(code) = code {
            form.append_pair("VerificationSid", &pending.sid)
                .append_pair("Code", code);
            "VerificationCheck"
        } else {
            form.append_pair("To", &pending.canonical)
                .append_pair("Channel", pending.channel.as_str());
            "Verifications"
        };
        let body = Private::new(form.finish().into_bytes());
        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/{operation}",
            self.service
        );
        let secret = std::str::from_utf8(self.secret.expose()).map_err(|_| Error::Configuration)?;
        let basic = Zeroizing::new(format!("{}:{secret}", self.account));
        let authorization = Zeroizing::new(format!("Basic {}", STANDARD.encode(basic.as_bytes())));
        let mut request = http::Request::builder()
            .method("POST")
            .uri(url)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .expect("validated service SID and static HTTP request parts");
        let mut header =
            http::HeaderValue::from_str(&authorization).expect("Basic base64 is a valid header");
        header.set_sensitive(true);
        request
            .headers_mut()
            .insert(http::header::AUTHORIZATION, header);
        let terms = serde_json::to_string(&(
            request.uri().to_string(),
            pending.account.as_str(),
            "production",
            pending.channel.as_str(),
            pending.context.binding,
            pending.context.operation,
            pending.context.expires_at,
            pending.context.include_upstream,
            request.body().expose(),
        ))
        .expect("a tuple of strings, bytes and integers serializes");
        let digest = Identity::derive(
            &pending.context.identity_key,
            &pending.context.scope,
            "twilio.verify.request.v1",
            &terms,
        );
        let binding = digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Prepared {
            request,
            pending,
            binding,
            checking: code.is_some(),
        })
    }
}

/// Inert prepared operation with a private request and a keyed, non-revealing quote binding.
pub struct Prepared {
    request: http::Request<Private>,
    pending: Pending,
    binding: String,
    checking: bool,
}
impl Prepared {
    /// Bind a price preview to every relevant account/mode/operation/request term.
    pub fn binding(&self) -> &str {
        &self.binding
    }
    /// Make one request only after validating the cost permit and real transport mode.
    pub async fn dispatch(
        self,
        state: &impl DispatchState,
        transport: &impl Transport,
        permit: cost::DispatchPermit<usize>,
        now: u64,
    ) -> Result<Step, Error> {
        permit
            .validate(&self.binding, cost::Mode::Production, now)
            .map_err(|_| Error::Budget)?;
        if transport.mode() != cost::Mode::Production || now >= self.pending.context.expires_at {
            return Err(Error::Configuration);
        }
        state
            .reserve(
                &self.pending.context.binding,
                &self.pending.context.operation,
                &self.binding,
                permit.quote().price.maximum().ok_or(Error::Budget)?,
            )
            .map_err(|_| Error::Reservation)?;
        let response = transport.send(self.request).await?;
        if response.body().expose().len() > 65536 {
            return Err(Error::Uncertain);
        }
        if !response.status().is_success() {
            return Err(Error::ProviderResponse {
                status: response.status().as_u16(),
                response: response.into_body(),
            });
        }
        let decoded: Reply =
            serde_json::from_slice(response.body().expose()).map_err(|_| Error::Uncertain)?;
        if decoded.account_sid != self.pending.account
            || decoded.service_sid != self.pending.service
            || decoded.to != *self.pending.canonical
            || !self.pending.channel.accepts(&decoded.channel)
            || !sid(&decoded.sid, "VE")
        {
            return Err(Error::Refused);
        }
        if self.checking && decoded.sid != self.pending.sid {
            return Err(Error::Refused);
        }
        if decoded.status == "pending" {
            let mut pending = self.pending;
            pending.sid = decoded.sid;
            return Ok(Step::Pending(pending));
        }
        if !self.checking || decoded.status != "approved" || decoded.valid != Some(true) {
            return Err(Error::Refused);
        }
        let evidence = AdapterEvidence {
            identifier: Private::new(self.pending.canonical.as_bytes().to_vec()),
            binding: self.pending.context.binding,
            expires_at: self.pending.context.expires_at,
            upstream: Some(response.into_body()),
        };
        adapters::project(evidence, self.pending.context, now).map(Step::Verified)
    }
}

#[derive(Deserialize)]
struct Reply {
    sid: String,
    account_sid: String,
    service_sid: String,
    to: String,
    channel: String,
    status: String,
    valid: Option<bool>,
}
fn sid(value: &str, prefix: &str) -> bool {
    value.len() == 34
        && value.starts_with(prefix)
        && value[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
