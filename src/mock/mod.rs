//! In-process mock of the AWS Events API (axum). Synthetic data only.
// Handlers return `Err(Response)` early; boxing it would only add noise in test-support code.
#![allow(clippy::result_large_err)]

pub mod catalog;

use crate::api::*;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use base64::Engine;
use catalog::{EVENT_ID_PUBLIC, EVENT_ID_REINVENT, TIMEZONE};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub const MOCK_TOKEN: &str = "mock-token";
const PAGE_SIZES: [usize; 3] = [37, 50, 13];

/// Failure injection. Parsed from `--scenario` and the `X-Riv-Scenario` header (comma separated).
#[derive(Debug, Clone, Default)]
pub struct Scenario {
    pub closed_reservations: bool,
    /// From the Nth request on, answer 429 twice (Retry-After: 1).
    pub throttle_from: Option<u64>,
    pub full: HashSet<String>,
    pub clash: bool,
    /// Apply the next N writes, then cut the connection mid-response.
    pub drop_after_write: u32,
    /// Answer the next N writes with an HTML 500 and do NOT apply them.
    pub edge_html_500: u32,
    /// ListSessions answers 200 with no items and totalCount 0 (a service-side hiccup).
    pub empty_catalog: bool,
}

impl Scenario {
    pub fn parse(s: &str) -> Self {
        let mut sc = Scenario::default();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (name, arg) = part.split_once(':').map_or((part, None), |(a, b)| (a, Some(b)));
            match name {
                "closed-reservations" => sc.closed_reservations = true,
                "throttle" => sc.throttle_from = arg.and_then(|a| a.parse().ok()).or(Some(1)),
                "full" => {
                    if let Some(a) = arg {
                        sc.full.insert(a.to_string());
                    }
                }
                "clash" => sc.clash = true,
                "empty-catalog" => sc.empty_catalog = true,
                "drop-after-write" => sc.drop_after_write = arg.and_then(|a| a.parse().ok()).unwrap_or(1),
                "edge-html-500" => sc.edge_html_500 = arg.and_then(|a| a.parse().ok()).unwrap_or(1),
                _ => {}
            }
        }
        sc
    }
}

pub struct MockState {
    pub sessions: Vec<Session>,
    pub schedule: Mutex<Schedule>,
    pub scenario: Mutex<Scenario>,
    pub requests: AtomicU64,
    throttled_left: Mutex<u32>,
    pub write_log: Mutex<Vec<String>>,
    next_pt: AtomicU64,
}

impl MockState {
    fn new(scenario: Scenario) -> Self {
        Self {
            sessions: catalog::generate(),
            schedule: Mutex::new(Schedule::default()),
            scenario: Mutex::new(scenario),
            requests: AtomicU64::new(0),
            throttled_left: Mutex::new(0),
            write_log: Mutex::new(Vec::new()),
            next_pt: AtomicU64::new(1),
        }
    }
    /// Test hook: put a session on the schedule as if the user reserved it by hand.
    pub fn reserve_manually(&self, id: &str) {
        lock(&self.schedule).reserved.push(id.to_string());
    }
    pub fn writes(&self) -> Vec<String> {
        lock(&self.write_log).clone()
    }
    pub fn set_scenario(&self, s: Scenario) {
        *lock(&self.scenario) = s;
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

type Shared = Arc<MockState>;

pub struct MockServer {
    pub base_url: String,
    pub state: Shared,
    handle: tokio::task::JoinHandle<()>,
}

impl MockServer {
    /// Bind to `127.0.0.1:port` (0 = free port) and serve in the background.
    pub async fn start(port: u16, scenario: Scenario) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let addr = listener.local_addr()?;
        let state = Arc::new(MockState::new(scenario));
        let app = router(state.clone());
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self { base_url: format!("http://{addr}"), state, handle })
    }
    pub fn stop(&self) {
        self.handle.abort();
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/oauth2/token", post(oauth_token))
        .route("/oauth2/revoke", post(|| async { StatusCode::OK }))
        .route("/v1/events", get(list_events))
        .route("/v1/events/{eid}", get(get_event))
        .route("/v1/events/{eid}/sessions", get(list_sessions))
        .route("/v1/events/{eid}/sessions/{sid}", get(get_session))
        .route("/v1/events/{eid}/schedule", get(get_schedule))
        .route("/v1/events/{eid}/reservations", post(reserve))
        .route("/v1/events/{eid}/reservations/{sid}", delete(cancel))
        .route("/v1/events/{eid}/favorites", post(favorite))
        .route("/v1/events/{eid}/favorites/{sid}", delete(unfavorite))
        .route("/v1/events/{eid}/personal-time", post(pt_create))
        .route("/v1/events/{eid}/personal-time/{id}", put(pt_update).delete(pt_delete))
        .with_state(state)
}

fn events() -> Vec<Event> {
    let mk = |id: &str, name: &str, auth: bool| Event {
        event_id: id.into(),
        name: name.into(),
        event_type: "Demo".into(),
        start_date: "2026-11-30T08:00:00-08:00".into(),
        end_date: "2026-12-04T18:00:00-08:00".into(),
        timezone: Some(TIMEZONE.into()),
        timezone_abbreviation: Some("PST".into()),
        time_format: Some("12 hour".into()),
        supported_language_codes: vec!["en-US".into()],
        authentication_required: auth,
        ..Default::default()
    };
    vec![mk(EVENT_ID_PUBLIC, "Demo Public Event", false), mk(EVENT_ID_REINVENT, "Demo re:Invent (registered)", true)]
}

fn json_err(status: u16, msg: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(serde_json::json!({ "message": msg })),
    )
        .into_response()
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Read,
    Write,
    Reservation,
}

/// Shared pre-checks: event existence, auth, throttle, closed operations.
fn gate(st: &MockState, eid: &str, h: &HeaderMap, kind: Kind, public_ok: bool) -> Result<Scenario, Response> {
    let n = st.requests.fetch_add(1, Ordering::SeqCst) + 1;
    let mut sc = lock(&st.scenario).clone();
    if let Some(v) = h.get("x-riv-scenario").and_then(|v| v.to_str().ok()) {
        let extra = Scenario::parse(v);
        sc.closed_reservations |= extra.closed_reservations;
        sc.clash |= extra.clash;
        sc.full.extend(extra.full);
        sc.throttle_from = sc.throttle_from.or(extra.throttle_from);
    }
    let Some(ev) = events().into_iter().find(|e| e.event_id == eid) else {
        return Err(json_err(404, "event not found"));
    };
    if let Some(from) = sc.throttle_from {
        let mut left = lock(&st.throttled_left);
        if n == from {
            *left = 2;
        }
        if *left > 0 {
            *left -= 1;
            let mut r = json_err(429, "throttled");
            r.headers_mut().insert("retry-after", HeaderValue::from_static("1"));
            return Err(r);
        }
    }
    let needs_auth = ev.authentication_required || kind != Kind::Read || !public_ok;
    if needs_auth {
        let ok = h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(&format!("Bearer {MOCK_TOKEN}"));
        if !ok {
            return Err(json_err(401, "sign in required"));
        }
    }
    if kind == Kind::Reservation && sc.closed_reservations {
        return Err(json_err(409, "reservations are not open"));
    }
    Ok(sc)
}

enum Fault {
    None,
    /// HTML 500 from the edge; the write is NOT applied.
    EdgeHtml500,
    /// The write IS applied, then the connection breaks mid-response (outcome unknown to the client).
    DropAfter,
}

fn next_fault(st: &MockState) -> Fault {
    let mut sc = lock(&st.scenario);
    if sc.edge_html_500 > 0 {
        sc.edge_html_500 -= 1;
        return Fault::EdgeHtml500;
    }
    if sc.drop_after_write > 0 {
        sc.drop_after_write -= 1;
        return Fault::DropAfter;
    }
    Fault::None
}

fn html_500() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, [(header::CONTENT_TYPE, "text/html")], "<html><body>edge error</body></html>")
        .into_response()
}

/// Headers promise a body, then the stream errors: the client sees a broken connection.
fn broken_response() -> Response {
    struct Broken(bool);
    impl futures_core::Stream for Broken {
        type Item = Result<Vec<u8>, std::io::Error>;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            if self.0 {
                return std::task::Poll::Ready(None);
            }
            self.0 = true;
            std::task::Poll::Ready(Some(Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "dropped"))))
        }
    }
    (StatusCode::OK, Body::from_stream(Broken(false))).into_response()
}

/// Wrap a write: log it, honor fault injection, then return the normal reply.
fn finish_write(st: &MockState, label: String, apply: impl FnOnce() -> Response) -> Response {
    match next_fault(st) {
        Fault::EdgeHtml500 => html_500(),
        fault => {
            lock(&st.write_log).push(label);
            let normal = apply();
            if matches!(fault, Fault::DropAfter) { broken_response() } else { normal }
        }
    }
}

async fn list_events(State(st): State<Shared>) -> Response {
    st.requests.fetch_add(1, Ordering::SeqCst);
    Json(ListEventsResponse { items: events() }).into_response()
}

async fn get_event(State(st): State<Shared>, Path(eid): Path<String>) -> Response {
    st.requests.fetch_add(1, Ordering::SeqCst);
    match events().into_iter().find(|e| e.event_id == eid) {
        Some(event) => Json(GetEventResponse { event }).into_response(),
        None => json_err(404, "event not found"),
    }
}

fn enc_token(offset: usize) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("off:{offset}"))
}
fn dec_token(t: &str) -> Option<usize> {
    let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(t).ok()?;
    String::from_utf8(b).ok()?.strip_prefix("off:")?.parse().ok()
}

async fn list_sessions(
    State(st): State<Shared>,
    Path(eid): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    h: HeaderMap,
) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Read, true) {
        return r;
    }
    if lock(&st.scenario).empty_catalog {
        return Json(ListSessionsResponse { items: vec![], total_count: 0.0, next_token: None }).into_response();
    }
    // Variable page sizes: the size is derived from the offset so a token fully determines the next page.
    let offset = match q.get("nextToken") {
        None => 0,
        Some(t) => match dec_token(t) {
            Some(o) if o > 0 && o < st.sessions.len() => o,
            _ => return json_err(400, "invalid nextToken"),
        },
    };
    let size = PAGE_SIZES[(offset / 13) % PAGE_SIZES.len()];
    let end = (offset + size).min(st.sessions.len());
    let abstracts = q.get("includeAbstracts").map(|v| v != "false").unwrap_or(true);
    let items: Vec<Session> = st.sessions[offset..end]
        .iter()
        .map(|s| {
            let mut s = s.clone();
            if !abstracts {
                s.abstract_ = None;
            }
            s
        })
        .collect();
    let body = ListSessionsResponse {
        items,
        total_count: st.sessions.len() as f64,
        next_token: (end < st.sessions.len()).then(|| enc_token(end)),
    };
    let mut r = Json(body).into_response();
    // The mock only has en-US text; it answers in en-US whatever was requested (like the real fallback).
    r.headers_mut().insert("content-language", HeaderValue::from_static("en-US"));
    r
}

async fn get_session(State(st): State<Shared>, Path((eid, sid)): Path<(String, String)>, h: HeaderMap) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Read, true) {
        return r;
    }
    match st.sessions.iter().find(|s| s.session_id == sid) {
        Some(session) => Json(GetSessionResponse { session: session.clone() }).into_response(),
        None => json_err(404, "session not found"),
    }
}

async fn get_schedule(State(st): State<Shared>, Path(eid): Path<String>, h: HeaderMap) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Read, false) {
        return r;
    }
    if eid == EVENT_ID_PUBLIC {
        return json_err(404, "this event has no schedule");
    }
    Json(GetScheduleResponse { schedule: lock(&st.schedule).clone() }).into_response()
}

fn tz() -> chrono_tz::Tz {
    crate::timeutil::parse_tz(TIMEZONE).unwrap_or(chrono_tz::UTC)
}

fn overlaps(a: &Session, b: &Session) -> bool {
    match (a.range_utc(tz()), b.range_utc(tz())) {
        (Some((s1, e1)), Some((s2, e2))) => s1 < e2 && s2 < e1,
        _ => false,
    }
}

fn ok_bulk(result: BulkResult) -> Response {
    Json(BulkResponse { result }).into_response()
}

async fn reserve(
    State(st): State<Shared>,
    Path(eid): Path<String>,
    h: HeaderMap,
    Json(req): Json<SessionIdsRequest>,
) -> Response {
    let sc = match gate(&st, &eid, &h, Kind::Reservation, false) {
        Ok(sc) => sc,
        Err(r) => return r,
    };
    if req.session_ids.is_empty() || req.session_ids.len() > 10 {
        return json_err(400, "sessionIds must hold 1 to 10 ids");
    }
    finish_write(&st, format!("reserve {}", req.session_ids.join(",")), || {
        let mut result = BulkResult::default();
        let mut sched = lock(&st.schedule);
        let mut seen = HashSet::new();
        for id in &req.session_ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            let mut fail = |code, conflicts: Option<Vec<String>>| {
                result.failed.push(BulkFailure { session_id: id.clone(), code, conflicts_with: conflicts })
            };
            let Some(s) = st.sessions.iter().find(|s| &s.session_id == id) else {
                fail(BulkFailureCode::SessionNotReservable, None);
                continue;
            };
            if sched.reserved.contains(id) {
                fail(BulkFailureCode::AlreadyScheduled, None);
            } else if s.is_reservable != Some(true) {
                fail(BulkFailureCode::SessionNotReservable, None);
            } else if sc.full.contains(id) || s.seat_availability.is_some_and(|a| a.is_full()) {
                fail(BulkFailureCode::SessionFull, None);
            } else {
                let mut clashes: Vec<String> = sched
                    .reserved
                    .iter()
                    .filter(|rid| st.sessions.iter().any(|o| &o.session_id == *rid && overlaps(o, s)))
                    .cloned()
                    .collect();
                if clashes.is_empty() && sc.clash {
                    clashes = sched.reserved.first().cloned().into_iter().collect();
                    if clashes.is_empty() {
                        fail(BulkFailureCode::ScheduleConflict, None);
                        continue;
                    }
                }
                if clashes.is_empty() {
                    sched.reserved.push(id.clone());
                    result.successful.push(id.clone());
                } else {
                    fail(BulkFailureCode::ScheduleConflict, Some(clashes));
                }
            }
        }
        ok_bulk(result)
    })
}

async fn cancel(State(st): State<Shared>, Path((eid, sid)): Path<(String, String)>, h: HeaderMap) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Reservation, false) {
        return r;
    }
    finish_write(&st, format!("cancel {sid}"), || {
        let mut sched = lock(&st.schedule);
        match sched.reserved.iter().position(|r| r == &sid) {
            Some(i) => {
                sched.reserved.remove(i);
                StatusCode::NO_CONTENT.into_response()
            }
            None => json_err(404, "no such reservation"),
        }
    })
}

async fn favorite(
    State(st): State<Shared>,
    Path(eid): Path<String>,
    h: HeaderMap,
    Json(req): Json<SessionIdsRequest>,
) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Write, false) {
        return r;
    }
    if req.session_ids.is_empty() || req.session_ids.len() > 10 {
        return json_err(400, "sessionIds must hold 1 to 10 ids");
    }
    finish_write(&st, format!("favorite {}", req.session_ids.join(",")), || {
        let mut result = BulkResult::default();
        let mut sched = lock(&st.schedule);
        for id in &req.session_ids {
            if !st.sessions.iter().any(|s| &s.session_id == id) {
                result.failed.push(BulkFailure {
                    session_id: id.clone(),
                    code: BulkFailureCode::Other,
                    conflicts_with: None,
                });
            } else if sched.favorites.contains(id) {
                result.failed.push(BulkFailure {
                    session_id: id.clone(),
                    code: BulkFailureCode::AlreadyFavorited,
                    conflicts_with: None,
                });
            } else {
                sched.favorites.push(id.clone());
                result.successful.push(id.clone());
            }
        }
        ok_bulk(result)
    })
}

async fn unfavorite(State(st): State<Shared>, Path((eid, sid)): Path<(String, String)>, h: HeaderMap) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Write, false) {
        return r;
    }
    finish_write(&st, format!("unfavorite {sid}"), || {
        let mut sched = lock(&st.schedule);
        match sched.favorites.iter().position(|r| r == &sid) {
            Some(i) => {
                sched.favorites.remove(i);
                StatusCode::NO_CONTENT.into_response()
            }
            None => json_err(404, "not a favorite"),
        }
    })
}

fn validate_pt(i: &PersonalTimeInput) -> Result<(), Response> {
    use crate::timeutil::from_wire;
    let (Some(s), Some(e)) = (from_wire(&i.start_date_time), from_wire(&i.end_date_time)) else {
        return Err(json_err(400, "dateTime must be YYYY-MM-DDTHH:MM:SS in UTC with no offset"));
    };
    let mins = (e - s).num_minutes();
    if s.timestamp() % 60 != 0 || mins <= 0 || mins % 5 != 0 {
        return Err(json_err(400, "block must be positive, seconds 00, in 5-minute increments"));
    }
    if i.title.is_empty() || i.title.len() > 128 || i.description.is_empty() || i.description.len() > 250 {
        return Err(json_err(400, "title 1-128 and description 1-250 are required"));
    }
    Ok(())
}

/// Personal time lives in the mock's own list (kept in the write log + schedule).
async fn pt_create(
    State(st): State<Shared>,
    Path(eid): Path<String>,
    h: HeaderMap,
    Json(i): Json<PersonalTimeInput>,
) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Write, false) {
        return r;
    }
    if let Err(r) = validate_pt(&i) {
        return r;
    }
    finish_write(&st, format!("pt.create {}", i.title), || {
        let id = format!("pt-{}", st.next_pt.fetch_add(1, Ordering::SeqCst));
        lock(&st.schedule).personal_time.push(PersonalTime {
            personal_time_id: id,
            start_date_time: i.start_date_time.clone(),
            end_date_time: i.end_date_time.clone(),
            title: i.title.clone(),
            description: i.description.clone(),
            location: i.location.clone(),
        });
        StatusCode::NO_CONTENT.into_response()
    })
}

async fn pt_update(
    State(st): State<Shared>,
    Path((eid, id)): Path<(String, String)>,
    h: HeaderMap,
    Json(i): Json<PersonalTimeInput>,
) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Write, false) {
        return r;
    }
    if let Err(r) = validate_pt(&i) {
        return r;
    }
    finish_write(&st, format!("pt.update {id}"), || {
        let mut sched = lock(&st.schedule);
        match sched.personal_time.iter_mut().find(|p| p.personal_time_id == id) {
            Some(p) => {
                *p = PersonalTime {
                    personal_time_id: id.clone(),
                    start_date_time: i.start_date_time.clone(),
                    end_date_time: i.end_date_time.clone(),
                    title: i.title.clone(),
                    description: i.description.clone(),
                    location: i.location.clone(),
                };
                StatusCode::NO_CONTENT.into_response()
            }
            None => json_err(404, "no such personal time"),
        }
    })
}

async fn pt_delete(State(st): State<Shared>, Path((eid, id)): Path<(String, String)>, h: HeaderMap) -> Response {
    if let Err(r) = gate(&st, &eid, &h, Kind::Write, false) {
        return r;
    }
    finish_write(&st, format!("pt.delete {id}"), || {
        let mut sched = lock(&st.schedule);
        let before = sched.personal_time.len();
        sched.personal_time.retain(|p| p.personal_time_id != id);
        if sched.personal_time.len() < before {
            StatusCode::NO_CONTENT.into_response()
        } else {
            json_err(404, "no such personal time")
        }
    })
}

/// Minimal token endpoint so refresh can be tested: `mock-refresh` -> a fresh `mock-token`.
async fn oauth_token(axum::Form(f): axum::Form<HashMap<String, String>>) -> Response {
    if f.get("grant_type").map(String::as_str) == Some("refresh_token")
        && f.get("refresh_token").map(String::as_str) == Some("mock-refresh")
    {
        Json(serde_json::json!({"access_token": MOCK_TOKEN, "refresh_token": "mock-refresh-2", "expires_in": 3600}))
            .into_response()
    } else {
        json_err(400, "invalid_grant")
    }
}
