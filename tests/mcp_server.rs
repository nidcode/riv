//! The local MCP server over an in-memory transport, against the mock API.

use riv::api::http::HttpApi;
use riv::auth::StaticToken;
use riv::db::Db;
use riv::mcp::{RivServer, tools::Tools};
use riv::mock::{MOCK_TOKEN, MockServer, Scenario};
use riv::sync::{AbstractsMode, SyncOptions, sync};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use std::sync::Arc;

const EVENT: &str = "demo-reinvent";

struct Fixture {
    _server: MockServer,
    _dir: tempfile::TempDir,
    spec: std::path::PathBuf,
    client: rmcp::service::RunningService<rmcp::RoleClient, ()>,
}

async fn fixture() -> Fixture {
    riv::i18n::set_override(Some(riv::i18n::Lang::En));
    let server = MockServer::start(0, Scenario::default()).await.expect("mock");
    let api =
        Arc::new(HttpApi::new(server.base_url.clone(), Arc::new(StaticToken(MOCK_TOKEN.into()))).with_sleep_scale(0.0));
    let dir = tempfile::tempdir().expect("tmp");
    let db_path = dir.path().join("riv.db");
    {
        let db = Db::open(&db_path).expect("db");
        sync(
            api.as_ref(),
            &db,
            &SyncOptions { event_id: EVENT.into(), locale: None, abstracts: AbstractsMode::Auto },
            &|_| {},
        )
        .await
        .expect("sync");
    }
    let spec = dir.path().join("design.md");
    let tools = Tools {
        api,
        db_path,
        plans_dir: dir.path().join("plans"),
        default_event: EVENT.into(),
        default_spec: spec.clone(),
        account: "acct".into(),
    };
    let (st, ct) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        if let Ok(s) = RivServer::new(tools).serve(st).await {
            let _ = s.waiting().await;
        }
    });
    let client = ().serve(ct).await.expect("client");
    Fixture { _server: server, _dir: dir, spec, client }
}

fn args(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().cloned().expect("object")
}

async fn call(f: &Fixture, name: &str, a: serde_json::Value) -> (bool, String) {
    let r =
        f.client.call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(args(a))).await.expect("call");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<Vec<_>>().join("\n");
    assert!(text.len() <= 8 * 1024 + 64, "reply too large: {}", text.len());
    (r.is_error.unwrap_or(false), text)
}

#[tokio::test]
async fn lists_the_tools_and_guards_apply() {
    let f = fixture().await;
    let tools = f.client.list_all_tools().await.expect("tools");
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    for n in [
        "riv_status",
        "riv_search",
        "riv_session",
        "riv_schedule",
        "riv_plan",
        "riv_apply",
        "riv_verify",
        "riv_today",
        "riv_prep_pack",
    ] {
        assert!(names.contains(&n.to_string()), "missing {n}: {names:?}");
    }
    let apply = tools.iter().find(|t| t.name == "riv_apply").expect("apply");
    assert!(apply.description.as_deref().unwrap_or("").contains("human approval"));
    let schema = serde_json::to_string(&apply.input_schema).expect("schema");
    assert!(schema.contains("planId") || schema.contains("planId"));
}

#[tokio::test]
async fn search_status_session_and_phone_format() {
    let f = fixture().await;
    let (err, out) = call(&f, "riv_status", serde_json::json!({})).await;
    assert!(!err && out.contains("synced 120 sessions"), "{out}");
    let (err, out) = call(&f, "riv_search", serde_json::json!({"query": "bedrock", "limit": 3})).await;
    assert!(!err && out.contains("| id | code |"), "{out}");
    let (_, out) = call(&f, "riv_search", serde_json::json!({"query": "bedrock", "limit": 3, "format": "phone"})).await;
    assert!(!out.contains('|') && out.lines().count() <= 3, "{out}");
    let (_, out) =
        call(&f, "riv_search", serde_json::json!({"query": "", "filters": {"level": 300}, "limit": 100})).await;
    assert!(out.lines().count() <= 27, "limit is capped at 25 rows + header");
    let (err, out) = call(&f, "riv_session", serde_json::json!({"id": "mock-0001"})).await;
    assert!(!err && out.contains("mock-0001"));
    let (err, out) = call(&f, "riv_session", serde_json::json!({"id": "mock-0001", "live": true})).await;
    assert!(!err && out.contains("seats:"), "{out}");
    let (err, _) = call(&f, "riv_session", serde_json::json!({"id": "nope"})).await;
    assert!(err);
}

#[tokio::test]
async fn plan_then_apply_roundtrip() {
    let f = fixture().await;
    std::fs::write(&f.spec, format!("```yaml\n# riv:desired-state v1\nevent: {EVENT}\ntimezone: America/Los_Angeles\nsessions:\n  - {{id: mock-0001, want: favorite}}\nblocks: []\n```\n")).expect("spec");
    let (err, plan) = call(&f, "riv_plan", serde_json::json!({})).await;
    assert!(!err, "{plan}");
    assert!(plan.contains("NOT been applied") && plan.contains("~ favorite"), "{plan}");
    let id = plan.lines().find_map(|l| l.strip_prefix("planId: ")).expect("planId").trim().to_string();
    // an invented plan id cannot apply
    let (err, out) = call(&f, "riv_apply", serde_json::json!({"planId": "BOGUS"})).await;
    assert!(err && out.contains("no approved plan"), "{out}");
    let (err, out) = call(&f, "riv_apply", serde_json::json!({"planId": id})).await;
    assert!(!err && out.contains("DONE"), "{out}");
    let (_, v) = call(&f, "riv_verify", serde_json::json!({})).await;
    assert!(v.contains("matches"), "{v}");
    let (_, s) = call(&f, "riv_schedule", serde_json::json!({})).await;
    assert!(s.contains("Favorites (1)"), "{s}");
}

#[tokio::test]
async fn today_and_prep_pack_tools() {
    let f = fixture().await;
    let (err, out) = call(&f, "riv_today", serde_json::json!({"date": "2026-12-01", "format": "phone"})).await;
    assert!(!err && out.contains("Nothing scheduled"), "{out}");
    let (err, _) = call(&f, "riv_today", serde_json::json!({"date": "bogus"})).await;
    assert!(err);
    let (err, out) = call(&f, "riv_prep_pack", serde_json::json!({"id": "mock-0001", "lang": "ja"})).await;
    assert!(!err, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("json pack");
    assert!(v["queries"].as_array().is_some_and(|q| !q.is_empty()) && v["savePath"].is_string());
}
