//! Hand-written types and the operation table must agree with the saved OpenAPI document.

use riv::api::*;
use serde_json::Value;
use std::collections::BTreeSet;

fn spec() -> Value {
    serde_json::from_str(include_str!("../openapi/awsevents.v1.json")).expect("openapi parses")
}

fn schema(name: &str) -> Value {
    spec()["components"]["schemas"][name].clone()
}

fn props(name: &str) -> BTreeSet<String> {
    schema(name)["properties"].as_object().expect("properties").keys().cloned().collect()
}

fn required(name: &str) -> BTreeSet<String> {
    schema(name)["required"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn keys(v: &impl serde::Serialize) -> BTreeSet<String> {
    serde_json::to_value(v).expect("serialize").as_object().expect("object").keys().cloned().collect()
}

#[test]
fn operations_match_openapi() {
    let s = spec();
    let mut n = 0;
    for op in ops::ALL {
        let item = &s["paths"][op.template][op.method.to_lowercase()];
        assert_eq!(item["operationId"], op.id, "{} {}", op.method, op.template);
        n += 1;
    }
    let total: usize =
        s["paths"].as_object().expect("paths").values().map(|m| m.as_object().map_or(0, |o| o.len())).sum();
    assert_eq!((n, total), (12, 12), "operation count drifted");
}

#[test]
fn session_event_personal_time_fields_match() {
    let full_session = Session {
        session_id: "a".into(),
        abbreviation: Some("x".into()),
        abstract_: Some("x".into()),
        type_: Some("x".into()),
        level: Some("x".into()),
        venue: Some("x".into()),
        room: Some("x".into()),
        is_all_day_session: Some(false),
        is_reservable: Some(true),
        seat_availability: Some(SeatAvailability::Available),
        session_time: Some(SessionTime::default()),
        ..Default::default()
    };
    assert_eq!(keys(&full_session), props("Session"));

    let full_event = Event {
        timezone: Some("x".into()),
        timezone_abbreviation: Some("x".into()),
        time_format: Some("x".into()),
        address: Some(EventAddress::default()),
        ..Default::default()
    };
    assert_eq!(keys(&full_event), props("Event"));

    let pt = PersonalTime { location: Some("x".into()), ..Default::default() };
    assert_eq!(keys(&pt), props("PersonalTime"));
    let pti = PersonalTimeInput { location: Some("x".into()), ..Default::default() };
    assert_eq!(keys(&pti), props("PersonalTimeInput"));
    assert_eq!(keys(&Schedule::default()), props("Schedule"));
    assert_eq!(keys(&BulkResult::default()), props("BulkResult"));
    let bf = BulkFailure { session_id: "a".into(), code: BulkFailureCode::Other, conflicts_with: Some(vec![]) };
    assert_eq!(keys(&bf), props("BulkFailure"));
}

#[test]
fn required_fields_are_always_serialized() {
    let checks: Vec<(&str, BTreeSet<String>)> = vec![
        ("Session", keys(&Session::default())),
        ("Event", keys(&Event::default())),
        ("PersonalTime", keys(&PersonalTime::default())),
        ("PersonalTimeInput", keys(&PersonalTimeInput::default())),
        ("Schedule", keys(&Schedule::default())),
        ("BulkResult", keys(&BulkResult::default())),
    ];
    for (name, got) in checks {
        let missing: Vec<_> = required(name).difference(&got).cloned().collect();
        assert!(missing.is_empty(), "{name} lacks required {missing:?}");
    }
}

#[test]
fn enums_are_fully_known() {
    for (name, check) in [
        (
            "BulkFailureCode",
            Box::new(|v: &str| {
                serde_json::from_value::<BulkFailureCode>(Value::String(v.into()))
                    .map(|c| c != BulkFailureCode::Other)
                    .unwrap_or(false)
            }) as Box<dyn Fn(&str) -> bool>,
        ),
        (
            "SeatAvailability",
            Box::new(|v: &str| {
                serde_json::from_value::<SeatAvailability>(Value::String(v.into()))
                    .map(|c| c != SeatAvailability::Other)
                    .unwrap_or(false)
            }),
        ),
    ] {
        for v in schema(name)["enum"].as_array().expect("enum") {
            let v = v.as_str().expect("string enum");
            // `other` is the one value allowed to map to the catch-all variant.
            assert!(check(v) || v == "other", "{name}::{v} not modelled");
        }
    }
}

#[test]
fn bulk_request_limits_match() {
    assert_eq!(schema("ReserveSessionsRequestContent")["properties"]["sessionIds"]["maxItems"], 10);
    assert_eq!(schema("PersonalTimeInput")["properties"]["title"]["maxLength"], 128);
}
