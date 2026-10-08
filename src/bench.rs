//! `riv bench`: measurements for the article. Claims are limited to what these functions measure.
//! This module talks to the network on purpose (it measures it); it is not used by any other command.

use crate::api::http::HttpApi;
use crate::auth::{StaticToken, TokenProvider};
use crate::db::Db;
use crate::error::{Result, RivError};
use crate::search::SearchQuery;
use crate::sync::{AbstractsMode, SyncOptions};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct BenchEnv {
    pub api_base: String,
    pub tokens: Arc<dyn TokenProvider>,
    pub event: String,
    pub mock: bool,
    pub runs: usize,
    pub fixture: Option<std::path::PathBuf>,
    /// `--generate N`: build N questions from the synced catalog instead of reading a fixture.
    pub generate: Option<usize>,
    /// Keeps the in-process mock alive for the bench's duration.
    pub _mock_server: Option<crate::mock::MockServer>,
}

impl BenchEnv {
    /// `--mock`: an in-process mock server (synthetic catalog), no sign-in needed.
    pub async fn mock(runs: usize, fixture: Option<std::path::PathBuf>) -> Result<Self> {
        let s = crate::mock::MockServer::start(0, Default::default()).await.map_err(RivError::from)?;
        Ok(Self {
            api_base: s.base_url.clone(),
            tokens: Arc::new(StaticToken(crate::mock::MOCK_TOKEN.into())),
            event: "demo-reinvent".into(),
            mock: true,
            runs,
            fixture,
            generate: None,
            _mock_server: Some(s),
        })
    }

    fn api(&self) -> HttpApi {
        HttpApi::new(self.api_base.clone(), self.tokens.clone())
    }
}

pub struct Report {
    pub name: &'static str,
    pub procedure: String,
    pub table: String,
    pub raw: serde_json::Value,
}

pub fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let rank = ((p / 100.0) * sorted_ms.len() as f64).ceil().max(1.0) as usize;
    sorted_ms[rank.min(sorted_ms.len()) - 1]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub fn environment(env: &BenchEnv) -> String {
    format!(
        "- date: {}\n- riv: {} ({} {})\n- target: {} ({})\n- event: {}\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        env.api_base,
        if env.mock { "in-process mock, synthetic catalog" } else { "real API" },
        env.event
    )
}

async fn temp_db(name: &str) -> Result<(Db, tempfile::TempDir)> {
    let dir = tempfile::tempdir().map_err(RivError::from)?;
    let db = Db::open(&dir.path().join(format!("{name}.db")))?;
    Ok((db, dir))
}

/// REST vs remote MCP for the same ListSessions page: time and bytes.
pub async fn protocol(env: &BenchEnv) -> Result<Report> {
    let url = format!("{}/v1/events/{}/sessions?includeAbstracts=true", env.api_base, env.event);
    let token = env.tokens.token().await;
    let client = reqwest::Client::new();
    let (mut times, mut bytes) = (Vec::new(), 0usize);
    for _ in 0..env.runs.max(1) {
        let t = Instant::now();
        let mut rb = client.get(&url);
        if let Some(tok) = &token {
            rb = rb.bearer_auth(tok);
        }
        let resp =
            rb.send().await.map_err(|e| RivError::general(format!("REST request failed: {}", e.without_url())))?;
        if !resp.status().is_success() {
            return Err(RivError::general(format!("REST returned {}", resp.status())));
        }
        let body = resp.bytes().await.map_err(|e| RivError::general(format!("REST body: {}", e.without_url())))?;
        times.push(ms(t.elapsed()));
        bytes = body.len();
    }
    times.sort_by(f64::total_cmp);
    let mut table = format!(
        "| path | p50 ms | p95 ms | bytes (1 page) |\n|---|---|---|---|\n| REST ListSessions | {:.0} | {:.0} | {bytes} |\n",
        percentile(&times, 50.0),
        percentile(&times, 95.0)
    );
    let mcp = remote_mcp(env, token.as_deref()).await;
    let mut raw = serde_json::json!({ "rest_ms": times, "rest_bytes": bytes });
    match mcp {
        Ok((mut t, b, name)) => {
            t.sort_by(f64::total_cmp);
            table.push_str(&format!(
                "| remote MCP `{name}` | {:.0} | {:.0} | {b} |\n",
                percentile(&t, 50.0),
                percentile(&t, 95.0)
            ));
            raw["mcp_ms"] = serde_json::json!(t);
            raw["mcp_bytes"] = serde_json::json!(b);
        }
        Err(why) => {
            table.push_str(&format!("| remote MCP | n/a | n/a | n/a |\n\nRemote MCP was **not measured automatically**: {why}\n\nManual measurement: connect Kiro or Claude Code to `{}/mcp`, sign in, ask for one page of sessions, and record wall-clock time and the size of the tool result shown in the transcript.\n", env.api_base));
            raw["mcp_skipped"] = serde_json::json!(why);
        }
    }
    Ok(Report {
        name: "protocol",
        procedure: format!(
            "Same ListSessions page (includeAbstracts=true, first page) fetched {} times over REST, and over the remote MCP endpoint via rmcp's streamable-HTTP client with the same Bearer token. Bytes = REST body / MCP tool-result text.",
            env.runs
        ),
        table,
        raw,
    })
}

async fn remote_mcp(env: &BenchEnv, token: Option<&str>) -> std::result::Result<(Vec<f64>, usize, String), String> {
    use rmcp::ServiceExt;
    use rmcp::model::CallToolRequestParams;
    use rmcp::transport::StreamableHttpClientTransport;
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    if env.mock {
        return Err("the mock server has no /mcp endpoint".into());
    }
    let token = token.ok_or("no access token (run `riv login` or set RIV_TOKEN)")?;
    let cfg =
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", env.api_base)).auth_header(token.to_string());
    let client = ().serve(StreamableHttpClientTransport::from_config(cfg)).await.map_err(|e| format!("connect/initialize failed (a 401 means the PKCE access token was not accepted; use the client's OAuth flow): {e}"))?;
    let tools = client.list_all_tools().await.map_err(|e| format!("tools/list failed: {e}"))?;
    let tool = tools
        .iter()
        .find(|t| t.name.to_lowercase().replace(['_', '-'], "").contains("listsessions"))
        .ok_or("no list-sessions tool found")?;
    let name = tool.name.to_string();
    let mut times = Vec::new();
    let mut bytes = 0;
    for _ in 0..env.runs.max(1) {
        let args = serde_json::json!({ "eventId": env.event, "includeAbstracts": true });
        let t = Instant::now();
        let r = client
            .call_tool(
                CallToolRequestParams::new(name.clone()).with_arguments(args.as_object().cloned().unwrap_or_default()),
            )
            .await
            .map_err(|e| format!("tool call failed: {e}"))?;
        times.push(ms(t.elapsed()));
        bytes = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.len()).sum();
    }
    Ok((times, bytes, name))
}

/// Initial sync time without and with abstracts, into throw-away databases.
pub async fn first_run(env: &BenchEnv) -> Result<Report> {
    let api = env.api();
    let mut rows = String::from("| mode | seconds | sessions |\n|---|---|---|\n");
    let mut raw = serde_json::Map::new();
    for (label, mode) in [("without abstracts", AbstractsMode::Never), ("with abstracts", AbstractsMode::Always)] {
        let (db, _dir) = temp_db("first-run").await?;
        let t = Instant::now();
        let r = crate::sync::sync(
            &api,
            &db,
            &SyncOptions { event_id: env.event.clone(), locale: None, abstracts: mode },
            &|_| {},
        )
        .await?;
        let secs = t.elapsed().as_secs_f64();
        rows.push_str(&format!("| {label} | {secs:.1} | {} |\n", r.sessions));
        raw.insert(label.into(), serde_json::json!({ "seconds": secs, "sessions": r.sessions }));
    }
    Ok(Report {
        name: "first-run",
        procedure: "Full `riv sync` into an empty database, once per mode. ListSessions is limited to 120 requests/minute; no fixed sleeps, only Retry-After on 429.".into(),
        table: rows,
        raw: raw.into(),
    })
}

const WARM_WORDS: &[&str] =
    &["agents", "serverless", "security", "bedrock", "lambda", "data", "kubernetes", "cost", "platform", "streaming"];

/// 50 local searches on a synced database: p50/p95.
pub async fn warm(env: &BenchEnv) -> Result<Report> {
    let api = env.api();
    let (db, _dir) = temp_db("warm").await?;
    crate::sync::sync(
        &api,
        &db,
        &SyncOptions { event_id: env.event.clone(), locale: None, abstracts: AbstractsMode::Always },
        &|_| {},
    )
    .await?;
    let mut times = Vec::new();
    for n in 0..50 {
        let text = format!("{} {}", WARM_WORDS[n % WARM_WORDS.len()], WARM_WORDS[(n * 7 + 3) % WARM_WORDS.len()]);
        let t = Instant::now();
        let _ = db.search(&env.event, &SearchQuery { text, limit: 10, ..Default::default() }, &[])?;
        times.push(ms(t.elapsed()));
    }
    times.sort_by(f64::total_cmp);
    let count = db.session_count(&env.event)?;
    Ok(Report {
        name: "warm",
        procedure: "After a full sync (with abstracts), 50 two-word searches (limit 10) on a warm SQLite database, timed in-process.".into(),
        table: format!("| sessions | p50 ms | p95 ms |\n|---|---|---|\n| {count} | {:.2} | {:.2} |\n", percentile(&times, 50.0), percentile(&times, 95.0)),
        raw: serde_json::json!({ "ms": times }),
    })
}

#[derive(Debug, Deserialize)]
pub struct Question {
    pub query: String,
    pub expected: Vec<String>,
    #[serde(default)]
    pub level: Option<u32>,
}

fn recall_at_10(db: &Db, event: &str, qs: &[Question]) -> Result<f64> {
    let mut total = 0.0;
    for q in qs {
        let hits = db.search(
            event,
            &SearchQuery { text: q.query.clone(), level: q.level, limit: 10, ..Default::default() },
            &[],
        )?;
        let found = q.expected.iter().filter(|e| hits.iter().any(|h| &h.session.session_id == *e)).count();
        total += found as f64 / q.expected.len().max(1) as f64;
    }
    Ok(total / qs.len().max(1) as f64)
}

/// recall@10 over a fixture of questions, with and without abstracts.
pub async fn search_quality(env: &BenchEnv) -> Result<Report> {
    if let Some(n) = env.generate {
        return search_quality_generated(env, n).await;
    }
    let path = env.fixture.clone().unwrap_or_else(|| {
        std::path::PathBuf::from(if env.mock {
            "bench/fixtures/queries.mock.json"
        } else {
            "bench/fixtures/queries.json"
        })
    });
    let qs: Vec<Question> = serde_json::from_slice(
        &std::fs::read(&path).map_err(|e| RivError::validation(format!("fixture {}: {e}", path.display())))?,
    )?;
    if qs.is_empty() {
        return Err(RivError::validation(
            "the fixture has no questions (fill bench/fixtures/queries.json from the real catalog)",
        ));
    }
    let api = env.api();
    let mut rows = String::from("| index | recall@10 | questions |\n|---|---|---|\n");
    let mut raw = serde_json::Map::new();
    for (label, mode) in [("without abstracts", AbstractsMode::Never), ("with abstracts", AbstractsMode::Always)] {
        let (db, _dir) = temp_db("quality").await?;
        crate::sync::sync(
            &api,
            &db,
            &SyncOptions { event_id: env.event.clone(), locale: None, abstracts: mode },
            &|_| {},
        )
        .await?;
        let r = recall_at_10(&db, &env.event, &qs)?;
        rows.push_str(&format!("| {label} | {r:.3} | {} |\n", qs.len()));
        raw.insert(label.into(), serde_json::json!(r));
    }
    Ok(Report {
        name: "search-quality",
        procedure: format!(
            "{} questions from {}; per question, recall@10 = fraction of its expected sessionIds found in the top 10, averaged. On the mock catalog this shows the harness works, not real-world quality.",
            qs.len(),
            path.display()
        ),
        table: rows,
        raw: raw.into(),
    })
}

const GEN_STOPWORDS: &[&str] = &[
    "about", "their", "these", "those", "which", "while", "where", "using", "should", "would", "other", "build",
    "learn", "session", "sessions", "will", "from", "with", "that", "this", "your", "have", "into", "more", "than",
    "what", "when",
];

/// N deterministic questions from sessions that have an abstract and a service. The query uses words that appear in the
/// abstract but not in the title, so it only works if the abstract is indexed. These are synthetic questions: they show
/// what abstracts add to retrieval, not how real attendees search.
pub fn generate_questions(db: &Db, event: &str, n: usize) -> Result<Vec<Question>> {
    let mut all = db.all_sessions(event)?;
    all.sort_by(|a, b| a.session.session_id.cmp(&b.session.session_id));
    let usable: Vec<_> = all
        .into_iter()
        .filter(|s| s.session.abstract_.as_deref().is_some_and(|a| a.len() > 80) && !s.session.services.is_empty())
        .collect();
    if usable.is_empty() || n == 0 {
        return Ok(vec![]);
    }
    let step = (usable.len() / n).max(1);
    let mut out = Vec::new();
    for s in usable.iter().step_by(step).take(n) {
        let title: std::collections::HashSet<String> =
            s.session.title.split(|c: char| !c.is_alphanumeric()).map(str::to_lowercase).collect();
        let mut words: Vec<String> = Vec::new();
        for w in s.session.abstract_.as_deref().unwrap_or("").split(|c: char| !c.is_alphanumeric()) {
            let w = w.to_lowercase();
            if w.len() >= 6 && !title.contains(&w) && !GEN_STOPWORDS.contains(&w.as_str()) && !words.contains(&w) {
                words.push(w);
            }
            if words.len() == 4 {
                break;
            }
        }
        if words.is_empty() {
            continue;
        }
        out.push(Question {
            query: format!("{} {}", words.join(" "), s.session.services[0].to_lowercase()),
            expected: vec![s.session.session_id.clone()],
            level: None,
        });
    }
    Ok(out)
}

async fn search_quality_generated(env: &BenchEnv, n: usize) -> Result<Report> {
    let api = env.api();
    let mut dbs = Vec::new();
    for (label, mode) in [("without abstracts", AbstractsMode::Never), ("with abstracts", AbstractsMode::Always)] {
        let (db, dir) = temp_db("quality-gen").await?;
        crate::sync::sync(
            &api,
            &db,
            &SyncOptions { event_id: env.event.clone(), locale: None, abstracts: mode },
            &|_| {},
        )
        .await?;
        dbs.push((label, db, dir));
    }
    let qs = generate_questions(&dbs[1].1, &env.event, n)?;
    if qs.is_empty() {
        return Err(RivError::general("no session with an abstract and a service to build questions from"));
    }
    let mut rows = String::from("| index | recall@10 | questions |\n|---|---|---|\n");
    let mut raw = serde_json::Map::new();
    for (label, db, _) in &dbs {
        let r = recall_at_10(db, &env.event, &qs)?;
        rows.push_str(&format!("| {label} | {r:.3} | {} |\n", qs.len()));
        raw.insert((*label).into(), serde_json::json!(r));
    }
    // The generated queries are not stored: they are derived from catalog text.
    raw.insert("questions".into(), serde_json::json!(qs.len()));
    Ok(Report {
        name: "search-quality (generated questions)",
        procedure: format!(
            "{} questions generated from the synced catalog: for each sampled session, 4 words from its abstract that are not in its title plus its first service; expected = that session. Synthetic, so it measures what indexing abstracts adds, not real attendee queries.",
            qs.len()
        ),
        table: rows,
        raw: raw.into(),
    })
}

pub fn write_report(dir: &std::path::Path, env: &BenchEnv, r: &Report) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.md", chrono::Utc::now().format("%Y-%m-%d")));
    let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.is_empty() {
        existing.push_str(&format!("# riv bench results\n\n## Environment\n{}\n", environment(env)));
    }
    existing.push_str(&format!(
        "\n## {}\n\n**Procedure.** {}\n\n{}\n<details><summary>raw data</summary>\n\n```json\n{}\n```\n</details>\n",
        r.name,
        r.procedure,
        r.table,
        serde_json::to_string_pretty(&r.raw)?
    ));
    std::fs::write(&path, existing)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn nearest_rank_percentiles() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), 50.0);
        assert_eq!(percentile(&v, 95.0), 95.0);
        assert_eq!(percentile(&[], 50.0), 0.0);
    }
}
