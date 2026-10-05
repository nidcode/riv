//! The 12 REST operations: method + path template. The OpenAPI conformance test checks this table
//! against `openapi/awsevents.v1.json`, and `HttpApi` builds every URL from it (no hand-typed paths).

pub struct OpSpec {
    pub id: &'static str,
    pub method: &'static str,
    pub template: &'static str,
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

impl OpSpec {
    /// Fill `{placeholders}` in order with percent-encoded `args`.
    pub fn path(&self, args: &[&str]) -> String {
        let mut out = String::new();
        let mut it = args.iter();
        let mut rest = self.template;
        while let Some(i) = rest.find('{') {
            out.push_str(&rest[..i]);
            let j = rest[i..].find('}').map_or(rest.len(), |j| i + j + 1);
            out.push_str(&enc(it.next().copied().unwrap_or("")));
            rest = &rest[j..];
        }
        out.push_str(rest);
        out
    }
}

const fn op(id: &'static str, method: &'static str, template: &'static str) -> OpSpec {
    OpSpec { id, method, template }
}

pub const LIST_EVENTS: OpSpec = op("ListEvents", "GET", "/v1/events");
pub const GET_EVENT: OpSpec = op("GetEvent", "GET", "/v1/events/{eventId}");
pub const LIST_SESSIONS: OpSpec = op("ListSessions", "GET", "/v1/events/{eventId}/sessions");
pub const GET_SESSION: OpSpec = op("GetSession", "GET", "/v1/events/{eventId}/sessions/{sessionId}");
pub const GET_SCHEDULE: OpSpec = op("GetSchedule", "GET", "/v1/events/{eventId}/schedule");
pub const RESERVE_SESSIONS: OpSpec = op("ReserveSessions", "POST", "/v1/events/{eventId}/reservations");
pub const CANCEL_RESERVATION: OpSpec =
    op("CancelReservation", "DELETE", "/v1/events/{eventId}/reservations/{sessionId}");
pub const ASSOCIATE_FAVORITES: OpSpec = op("AssociateFavorites", "POST", "/v1/events/{eventId}/favorites");
pub const DISASSOCIATE_FAVORITE: OpSpec =
    op("DisassociateFavorite", "DELETE", "/v1/events/{eventId}/favorites/{sessionId}");
pub const CREATE_PERSONAL_TIME: OpSpec = op("CreatePersonalTime", "POST", "/v1/events/{eventId}/personal-time");
pub const UPDATE_PERSONAL_TIME: OpSpec =
    op("UpdatePersonalTime", "PUT", "/v1/events/{eventId}/personal-time/{personalTimeId}");
pub const DELETE_PERSONAL_TIME: OpSpec =
    op("DeletePersonalTime", "DELETE", "/v1/events/{eventId}/personal-time/{personalTimeId}");

pub const ALL: [&OpSpec; 12] = [
    &LIST_EVENTS,
    &GET_EVENT,
    &LIST_SESSIONS,
    &GET_SESSION,
    &GET_SCHEDULE,
    &RESERVE_SESSIONS,
    &CANCEL_RESERVATION,
    &ASSOCIATE_FAVORITES,
    &DISASSOCIATE_FAVORITE,
    &CREATE_PERSONAL_TIME,
    &UPDATE_PERSONAL_TIME,
    &DELETE_PERSONAL_TIME,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_and_encodes() {
        assert_eq!(GET_SESSION.path(&["e1", "a/b"]), "/v1/events/e1/sessions/a%2Fb");
        assert_eq!(LIST_EVENTS.path(&[]), "/v1/events");
    }
}
