//! reqwest implementation of `EventsApi` with 401-refresh, 429 and 5xx handling.

use super::*;
use crate::auth::TokenProvider;
use reqwest::Method;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_BASE: &str = "https://api.awsevents.com";
const MAX_THROTTLE_RETRIES: u32 = 6;
const MAX_WAIT_SECS: u64 = 65;

pub struct HttpApi {
    base: String,
    client: reqwest::Client,
    tokens: Arc<dyn TokenProvider>,
    quota: Quota,
    /// Multiplier for Retry-After / backoff sleeps; tests set 0 for speed.
    sleep_scale: f64,
}

use quota::Quota;

/// One request: which operation, its quota cost, URL arguments, query, body, and whether it is a write
/// (a failed write has an unknown outcome).
struct Call<'a> {
    op: Op,
    units: u32,
    spec: &'a ops::OpSpec,
    args: &'a [&'a str],
    query: &'a [(&'a str, String)],
    body: Option<serde_json::Value>,
    write: bool,
}

impl<'a> Call<'a> {
    fn read(op: Op, units: u32, spec: &'a ops::OpSpec, args: &'a [&'a str]) -> Self {
        Self { op, units, spec, args, query: &[], body: None, write: false }
    }
    fn write(op: Op, units: u32, spec: &'a ops::OpSpec, args: &'a [&'a str]) -> Self {
        Self { write: true, ..Self::read(op, units, spec, args) }
    }
    fn query(mut self, q: &'a [(&'a str, String)]) -> Self {
        self.query = q;
        self
    }
    fn body(mut self, b: serde_json::Value) -> Self {
        self.body = Some(b);
        self
    }
}

struct Reply {
    content_language: Option<String>,
    body: Vec<u8>,
}

impl HttpApi {
    pub fn new(base: impl Into<String>, tokens: Arc<dyn TokenProvider>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("riv/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client builds with default TLS config");
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            client,
            tokens,
            quota: Quota::new(),
            sleep_scale: 1.0,
        }
    }

    pub fn with_sleep_scale(mut self, scale: f64) -> Self {
        self.sleep_scale = scale;
        self
    }

    async fn nap(&self, secs: f64) {
        let d = secs * self.sleep_scale;
        if d > 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(d)).await;
        }
    }

    /// Send one request with policy: refresh once on 401, honor 429 Retry-After, retry reads on 5xx.
    /// `units` is the quota cost; `write` marks requests whose outcome is unknown on failure.
    async fn send(&self, call: Call<'_>) -> ApiResult<Reply> {
        let Call { op, units, spec, args, query, body, write } = call;
        let url = format!("{}{}", self.base, spec.path(args));
        let method = Method::from_bytes(spec.method.as_bytes()).unwrap_or(Method::GET);
        let mut refreshed = false;
        let mut throttled = 0;
        let mut server_errs = 0;
        loop {
            let mut rb = self.client.request(method.clone(), &url).query(query);
            if let Some(t) = self.tokens.token().await {
                rb = rb.bearer_auth(t);
            }
            if let Some(b) = &body {
                rb = rb.json(b);
            }
            let resp = match rb.send().await {
                Ok(r) => r,
                Err(e) => {
                    // Connection-level failure: for a write the outcome is unknown.
                    let msg = format!("{} request failed: {}", method, strip_url(e));
                    if !write && server_errs < 2 {
                        server_errs += 1;
                        self.nap(0.5 * f64::from(server_errs)).await;
                        continue;
                    }
                    return Err(ApiError::Unknown(msg));
                }
            };
            let status = resp.status();
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            let content_language =
                resp.headers().get("content-language").and_then(|v| v.to_str().ok()).map(str::to_string);
            let bytes = match resp.bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    if write {
                        return Err(ApiError::Unknown(format!("body read failed: {}", strip_url(e))));
                    }
                    Vec::new()
                }
            };
            let reply = Reply { content_language, body: bytes };
            match status.as_u16() {
                429 => {
                    // A refused request spends no quota, so waiting then retrying is safe even for writes.
                    let secs = retry_after.unwrap_or(1).min(MAX_WAIT_SECS);
                    throttled += 1;
                    if throttled > MAX_THROTTLE_RETRIES {
                        return Err(ApiError::Throttled { retry_after_secs: secs });
                    }
                    self.nap(secs as f64).await;
                    continue;
                }
                401 => {
                    if refreshed {
                        return Err(ApiError::Unauthorized);
                    }
                    refreshed = true;
                    match self.tokens.refresh().await {
                        Ok(Some(_)) => continue,
                        _ => return Err(ApiError::Unauthorized),
                    }
                }
                500 | 503 => {
                    if write {
                        return Err(ApiError::Unknown(format!("server error {}", status.as_u16())));
                    }
                    if server_errs >= 2 {
                        return Err(ApiError::Unknown(format!("server error {}", status.as_u16())));
                    }
                    server_errs += 1;
                    self.nap(0.5 * f64::from(server_errs)).await;
                    continue;
                }
                _ => {}
            }
            self.quota.record(op, units);
            return match status.as_u16() {
                200..=299 => Ok(reply),
                400 => Err(ApiError::BadRequest(message_of(&reply.body))),
                403 => Err(ApiError::Forbidden(message_of(&reply.body))),
                404 => Err(ApiError::NotFound),
                409 => Err(ApiError::Closed),
                other => Err(ApiError::Unknown(format!("unexpected status {other}"))),
            };
        }
    }

    async fn json<T: serde::de::DeserializeOwned>(&self, r: Reply) -> ApiResult<T> {
        serde_json::from_slice(&r.body).map_err(|e| ApiError::Unknown(format!("unparseable response: {e}")))
    }
}

/// Error body may be JSON `{message}`, HTML, or empty. Never echo more than a short snippet.
fn message_of(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body)
        && let Some(m) = v.get("message").and_then(|m| m.as_str())
    {
        return m.chars().take(300).collect();
    }
    if body.is_empty() { "(no body: possibly an edge refusal)".into() } else { "(non-JSON body)".into() }
}

fn strip_url(e: reqwest::Error) -> String {
    e.without_url().to_string()
}

#[async_trait]
impl EventsApi for HttpApi {
    async fn list_events(&self) -> ApiResult<Vec<Event>> {
        let r = self.send(Call::read(Op::GetSession, 0, &ops::LIST_EVENTS, &[])).await?;
        Ok(self.json::<ListEventsResponse>(r).await?.items)
    }

    async fn get_event(&self, event_id: &str) -> ApiResult<Event> {
        let r = self.send(Call::read(Op::GetSession, 0, &ops::GET_EVENT, &[event_id])).await?;
        Ok(self.json::<GetEventResponse>(r).await?.event)
    }

    async fn list_sessions_page(&self, event_id: &str, p: &ListSessionsParams) -> ApiResult<SessionPage> {
        let mut q: Vec<(&str, String)> = vec![("includeAbstracts", p.include_abstracts.to_string())];
        if let Some(l) = &p.locale {
            q.push(("locale", l.clone()));
        }
        if let Some(t) = &p.next_token {
            q.push(("nextToken", t.clone()));
        }
        let r = self.send(Call::read(Op::ListSessions, 1, &ops::LIST_SESSIONS, &[event_id]).query(&q)).await?;
        let lang = r.content_language.clone();
        let body: ListSessionsResponse = self.json(r).await?;
        Ok(SessionPage {
            items: body.items,
            total_count: body.total_count.max(0.0) as u64,
            next_token: body.next_token,
            content_language: lang,
        })
    }

    async fn get_session(&self, event_id: &str, session_id: &str, locale: Option<&str>) -> ApiResult<Session> {
        let q: Vec<(&str, String)> = locale.map(|l| vec![("locale", l.to_string())]).unwrap_or_default();
        let r = self.send(Call::read(Op::GetSession, 1, &ops::GET_SESSION, &[event_id, session_id]).query(&q)).await?;
        Ok(self.json::<GetSessionResponse>(r).await?.session)
    }

    async fn get_schedule(&self, event_id: &str) -> ApiResult<Schedule> {
        let r = self.send(Call::read(Op::GetSchedule, 1, &ops::GET_SCHEDULE, &[event_id])).await?;
        Ok(self.json::<GetScheduleResponse>(r).await?.schedule)
    }

    async fn reserve(&self, event_id: &str, ids: &[String]) -> ApiResult<BulkResult> {
        let body = serde_json::json!({ "sessionIds": ids });
        let args = [event_id];
        let call = Call::write(Op::ReserveSessions, ids.len() as u32, &ops::RESERVE_SESSIONS, &args).body(body);
        let r = self.send(call).await?;
        Ok(self.json::<BulkResponse>(r).await?.result)
    }

    async fn cancel_reservation(&self, event_id: &str, session_id: &str) -> ApiResult<()> {
        self.send(Call::write(Op::CancelReservation, 1, &ops::CANCEL_RESERVATION, &[event_id, session_id]))
            .await
            .map(|_| ())
    }

    async fn associate_favorites(&self, event_id: &str, ids: &[String]) -> ApiResult<BulkResult> {
        let body = serde_json::json!({ "sessionIds": ids });
        let args = [event_id];
        let call = Call::write(Op::AssociateFavorites, ids.len() as u32, &ops::ASSOCIATE_FAVORITES, &args).body(body);
        let r = self.send(call).await?;
        Ok(self.json::<BulkResponse>(r).await?.result)
    }

    async fn disassociate_favorite(&self, event_id: &str, session_id: &str) -> ApiResult<()> {
        self.send(Call::write(Op::DisassociateFavorite, 1, &ops::DISASSOCIATE_FAVORITE, &[event_id, session_id]))
            .await
            .map(|_| ())
    }

    async fn create_personal_time(&self, event_id: &str, input: &PersonalTimeInput) -> ApiResult<()> {
        let body = serde_json::to_value(input).map_err(|e| ApiError::BadRequest(e.to_string()))?;
        self.send(Call::write(Op::CreatePersonalTime, 1, &ops::CREATE_PERSONAL_TIME, &[event_id]).body(body))
            .await
            .map(|_| ())
    }

    async fn update_personal_time(&self, event_id: &str, id: &str, input: &PersonalTimeInput) -> ApiResult<()> {
        let body = serde_json::to_value(input).map_err(|e| ApiError::BadRequest(e.to_string()))?;
        self.send(Call::write(Op::UpdatePersonalTime, 1, &ops::UPDATE_PERSONAL_TIME, &[event_id, id]).body(body))
            .await
            .map(|_| ())
    }

    async fn delete_personal_time(&self, event_id: &str, id: &str) -> ApiResult<()> {
        self.send(Call::write(Op::DeletePersonalTime, 1, &ops::DELETE_PERSONAL_TIME, &[event_id, id])).await.map(|_| ())
    }

    fn remaining(&self, op: Op) -> u32 {
        self.quota.remaining(op)
    }

    async fn wait_for_quota(&self, op: Op) {
        if let Some(d) = self.quota.wait_hint(op) {
            self.nap(d.as_secs_f64().min(MAX_WAIT_SECS as f64) + 0.1).await;
        }
    }
}
