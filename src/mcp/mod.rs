//! Local MCP server (stdio). A thin shell over `tools::Tools`.

pub mod tools;

use crate::api::http::HttpApi;
use crate::format::Format;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use tools::{SearchParams, Tools};

fn text(r: crate::error::Result<String>) -> CallToolResult {
    match r {
        Ok(s) => CallToolResult::success(vec![ContentBlock::text(s)]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("error: {e}"))]),
    }
}

fn fmt(f: &Option<String>) -> Format {
    f.as_deref().and_then(|f| f.parse().ok()).unwrap_or_default()
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct Empty {
    /// `ide` (default; Markdown tables allowed) or `phone` (short lines, no tables).
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct Filters {
    pub level: Option<u32>,
    /// Weekday (`tue`) or ISO date.
    pub day: Option<String>,
    pub topic: Option<String>,
    pub service: Option<String>,
    pub venue: Option<String>,
    /// Local window such as `13:00-15:00`; skips sessions colliding with your reservations.
    pub free_between: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    pub query: String,
    pub filters: Option<Filters>,
    /// Default 10, max 25.
    pub limit: Option<usize>,
    /// Subset of: id, code, title, start, venue, room, level, type, seat.
    pub fields: Option<Vec<String>>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionArgs {
    /// sessionId or short code.
    pub id: String,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct PlanArgs {
    /// Path to design.md (default `.kiro/specs/reinvent-2026/design.md`).
    pub spec_path: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyArgs {
    /// The planId returned by the immediately preceding riv_plan call. Required.
    pub plan_id: String,
    /// Needed when the plan replaces reservations (a seat may be lost). Only set after telling the user.
    pub accept_seat_loss: Option<bool>,
    /// Instead of planId: resume an interrupted run (reconciles unknown outcomes first).
    pub resume_run_id: Option<String>,
    pub spec_path: Option<String>,
    pub format: Option<String>,
}

#[derive(Clone)]
pub struct RivServer {
    tools: Tools,
}

#[tool_router]
impl RivServer {
    #[tool(name = "riv_status", description = "Sync state, sign-in state (never the token) and the default event.")]
    async fn riv_status(&self, Parameters(_): Parameters<Empty>) -> CallToolResult {
        text(self.tools.status().await)
    }

    #[tool(
        name = "riv_search",
        description = "Search the local session catalog (full-text + filters). Returns at most `limit` short rows; the catalog is never sent whole."
    )]
    async fn riv_search(&self, Parameters(a): Parameters<SearchArgs>) -> CallToolResult {
        let f = a.filters.unwrap_or_default();
        let p = SearchParams {
            query: a.query,
            level: f.level,
            day: f.day,
            topic: f.topic,
            service: f.service,
            venue: f.venue,
            free_between: f.free_between,
            limit: a.limit,
            fields: a.fields,
            event: None,
        };
        text(self.tools.search(p, fmt(&a.format)).await)
    }

    #[tool(name = "riv_session", description = "One session's details by sessionId or short code.")]
    async fn riv_session(&self, Parameters(a): Parameters<SessionArgs>) -> CallToolResult {
        text(self.tools.session(&a.id, None, fmt(&a.format)).await)
    }

    #[tool(
        name = "riv_schedule",
        description = "Summary of your reserved sessions, favorites and personal time (live from the API)."
    )]
    async fn riv_schedule(&self, Parameters(_): Parameters<Empty>) -> CallToolResult {
        text(self.tools.schedule(None).await)
    }

    #[tool(
        name = "riv_plan",
        description = "Diff the spec (design.md desired state) against the real schedule. Writes nothing to the API. Returns planId, a human-readable diff and requiredFlags. Show the diff to the user."
    )]
    async fn riv_plan(&self, Parameters(a): Parameters<PlanArgs>) -> CallToolResult {
        text(self.tools.plan(a.spec_path).await)
    }

    #[tool(
        name = "riv_apply",
        description = "Apply an approved plan (reserve/cancel/favorite). Requires human approval: approve via the Kiro/Crew approval button, never auto-approve. Take planId from the immediately preceding riv_plan. Returns per-action results and a verification."
    )]
    async fn riv_apply(&self, Parameters(a): Parameters<ApplyArgs>) -> CallToolResult {
        text(self.tools.apply(Some(a.plan_id), a.resume_run_id, a.accept_seat_loss.unwrap_or(false), a.spec_path).await)
    }

    #[tool(
        name = "riv_verify",
        description = "Compare the spec with the real schedule and list differences (read-only)."
    )]
    async fn riv_verify(&self, Parameters(a): Parameters<PlanArgs>) -> CallToolResult {
        text(self.tools.verify(a.spec_path).await)
    }
}

#[tool_handler]
impl ServerHandler for RivServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("riv", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "riv: plan and safely apply a re:Invent agenda from a Kiro spec. Search the local catalog with riv_search \
                 (never ask for the whole catalog). Writes only happen through riv_apply with a planId from riv_plan, \
                 after the user approved the diff. Say in words when a replacement may lose a seat.",
            )
    }
}

impl RivServer {
    pub fn new(tools: Tools) -> Self {
        Self { tools }
    }
}

/// Build the production tool set from the environment.
pub fn tools_from_env() -> Tools {
    Tools {
        api: Arc::new(HttpApi::new(crate::cli::ctx::api_base(), crate::auth::provider_from_env())),
        db_path: crate::paths::db_path(),
        plans_dir: crate::paths::plans_dir(),
        default_event: std::env::var("RIV_EVENT")
            .ok()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| crate::cli::DEFAULT_EVENT.to_string()),
        default_spec: std::env::var("RIV_SPEC")
            .ok()
            .filter(|e| !e.is_empty())
            .map(Into::into)
            .unwrap_or_else(|| crate::cli::DEFAULT_SPEC.into()),
        account: crate::auth::account_id(),
    }
}

/// Serve over stdio until the client disconnects.
pub async fn serve_stdio() -> crate::error::Result<()> {
    let server = RivServer::new(tools_from_env());
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| crate::error::RivError::general(format!("mcp: {e}")))?;
    running.waiting().await.map_err(|e| crate::error::RivError::general(format!("mcp: {e}")))?;
    Ok(())
}
