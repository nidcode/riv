//! Hand-written API types. Verified against `openapi/awsevents.v1.json` by tests/openapi_conformance.rs.
//! Every field is read defensively (optional) unless the OpenAPI marks it required.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventAddress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub event_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub is_online: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone_abbreviation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<EventAddress>,
    #[serde(default)]
    pub supported_language_codes: Vec<String>,
    #[serde(default)]
    pub authentication_required: bool,
}

/// How full a session is. Unknown values are kept as `Other` (defensive read).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeatAvailability {
    Available,
    Limited,
    VeryLimited,
    Unavailable,
    WalkUp,
    #[serde(other)]
    Other,
}

impl SeatAvailability {
    /// `unavailable` is the API's only "no seats" band; the brief calls it "full".
    pub fn is_full(self) -> bool {
        matches!(self, SeatAvailability::Unavailable)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Speaker {
    #[serde(default)]
    pub name: Option<String>,
}

macro_rules! session_struct {
    ($($list:ident),*) => {
        #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct Session {
            pub session_id: String,
            #[serde(default)]
            pub title: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub abbreviation: Option<String>,
            #[serde(default, rename = "abstract", skip_serializing_if = "Option::is_none")]
            pub abstract_: Option<String>,
            #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
            pub type_: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub level: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub venue: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub room: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub is_all_day_session: Option<bool>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub is_reservable: Option<bool>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub seat_availability: Option<SeatAvailability>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub session_time: Option<SessionTime>,
            #[serde(default)]
            pub speakers: Vec<Speaker>,
            $(#[serde(default)] pub $list: Vec<String>,)*
        }
    };
}
session_struct!(
    tracks, topics, industries, areas_of_interest, roles, services, segments, features,
    customer_personas, experiences, additional_activities, focus_areas
);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonalTime {
    pub personal_time_id: String,
    pub start_date_time: String,
    pub end_date_time: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonalTimeInput {
    pub start_date_time: String,
    pub end_date_time: String,
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    #[serde(default)]
    pub reserved: Vec<String>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub personal_time: Vec<PersonalTime>,
}

/// Why one session in a bulk request was refused. Unknown values map to `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BulkFailureCode {
    SessionNotReservable,
    ScheduleConflict,
    AlreadyScheduled,
    SessionFull,
    InsufficientAccess,
    TimePassed,
    AlreadyFavorited,
    NotFavorited,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BulkFailure {
    pub session_id: String,
    pub code: BulkFailureCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflicts_with: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BulkResult {
    #[serde(default)]
    pub successful: Vec<String>,
    #[serde(default)]
    pub failed: Vec<BulkFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdsRequest {
    pub session_ids: Vec<String>,
}

// ---- response envelopes ----
#[derive(Debug, Deserialize, Serialize)]
pub struct ListEventsResponse {
    #[serde(default)]
    pub items: Vec<Event>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct GetEventResponse {
    pub event: Event,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct GetSessionResponse {
    pub session: Session,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct GetScheduleResponse {
    pub schedule: Schedule,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct BulkResponse {
    pub result: BulkResult,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsResponse {
    #[serde(default)]
    pub items: Vec<Session>,
    #[serde(default)]
    pub total_count: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
}

/// One page of ListSessions plus the `Content-Language` the server answered with.
#[derive(Debug, Clone, Default)]
pub struct SessionPage {
    pub items: Vec<Session>,
    pub total_count: u64,
    pub next_token: Option<String>,
    pub content_language: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListSessionsParams {
    pub locale: Option<String>,
    pub include_abstracts: bool,
    pub next_token: Option<String>,
}

impl Session {
    /// UTC start/end from `sessionTime`. `default_tz` (the event's zone) applies when the session names none.
    pub fn range_utc(&self, default_tz: chrono_tz::Tz) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
        let st = self.session_time.as_ref()?;
        let tz = st.timezone.as_deref().and_then(crate::timeutil::parse_tz).unwrap_or(default_tz);
        crate::timeutil::session_range(st.date.as_deref()?, st.time.as_deref()?, st.length.as_deref(), tz)
    }

    /// Display code: the abbreviation, falling back to the session id.
    pub fn code(&self) -> &str {
        self.abbreviation.as_deref().unwrap_or(&self.session_id)
    }
}
