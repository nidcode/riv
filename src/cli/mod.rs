//! clap definitions and dispatch. Command bodies live in sibling modules.

mod apply_cmds;
mod auth_cmds;
mod catalog_cmds;
pub mod ctx;
mod doctor;
mod mock_cmd;
mod plan_cmds;

use crate::error::Result;
use clap::{Parser, Subcommand};

pub const DEFAULT_EVENT: &str = "reinvent2026";
pub use plan_cmds::DEFAULT_SPEC;

#[derive(Parser, Debug)]
#[command(name = "riv", version, about = "re:Invent as Code: declare your agenda, plan, apply.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Sign in with your Builder ID (OAuth 2.0 + PKCE).
    Login,
    /// Revoke tokens and delete local credentials.
    Logout,
    /// Show the signed-in account (email masked).
    Whoami,
    /// List events (no sign-in needed).
    Events {
        #[arg(long)]
        json: bool,
        /// Include events that already ended.
        #[arg(long)]
        past: bool,
    },
    /// Download the full session catalog into the local database.
    Sync {
        #[arg(long, env = "RIV_EVENT", default_value = DEFAULT_EVENT)]
        event: String,
        #[arg(long)]
        locale: Option<String>,
        #[arg(long, default_value = "auto")]
        abstracts: String,
    },
    /// Search the local catalog.
    Search {
        /// Free-text query (all terms first, then any term).
        query: Vec<String>,
        #[arg(long, env = "RIV_EVENT", default_value = DEFAULT_EVENT)]
        event: String,
        #[arg(long)]
        level: Option<u32>,
        #[arg(long)]
        day: Option<String>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        venue: Option<String>,
        /// e.g. 13:00-15:00 (local time); skips sessions that collide with your reservations.
        #[arg(long)]
        free_between: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
        /// Comma list: id,code,title,start,venue,room,level,type,seat
        #[arg(long)]
        fields: Option<String>,
    },
    /// Show one session by sessionId or short code.
    Show {
        id: String,
        #[arg(long, env = "RIV_EVENT", default_value = DEFAULT_EVENT)]
        event: String,
        #[arg(long)]
        json: bool,
    },
    /// Create requirements.md / design.md / tasks.md for this agenda (3 questions).
    Init {
        #[arg(default_value = ".")]
        dir: std::path::PathBuf,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        interests: Option<String>,
        #[arg(long)]
        constraints: Option<String>,
    },
    /// Diff the spec against your real schedule. Never writes to the API.
    Plan {
        #[arg(long)]
        spec: Option<std::path::PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Apply an approved plan (needs --plan from `riv plan`).
    Apply {
        #[arg(long)]
        plan: Option<String>,
        /// Reconcile an interrupted run with the schedule and send only what is missing.
        #[arg(long)]
        resume: Option<String>,
        #[arg(long)]
        spec: Option<std::path::PathBuf>,
        #[arg(long)]
        accept_seat_loss: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Compare the spec with your real schedule (read-only).
    Verify {
        #[arg(long)]
        spec: Option<std::path::PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Show your reserved sessions, favorites and personal time.
    Schedule {
        #[arg(long, env = "RIV_EVENT", default_value = DEFAULT_EVENT)]
        event: String,
        #[arg(long)]
        json: bool,
    },
    /// Check sign-in, catalog, API reachability and config (read-only).
    Doctor {
        #[arg(long, env = "RIV_EVENT", default_value = DEFAULT_EVENT)]
        event: String,
        #[arg(long)]
        json: bool,
    },
    /// Start the local MCP server on stdio (for Kiro / Claude / Crew).
    Mcp,
    /// Run the mock Events API server (synthetic data).
    Mock {
        #[arg(long, default_value_t = 8787)]
        port: u16,
        /// Failure injection, e.g. closed-reservations,throttle:5,full:<id>,clash,drop-after-write,edge-html-500
        #[arg(long)]
        scenario: Option<String>,
    },
}

pub async fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::Login => auth_cmds::login().await,
        Command::Logout => auth_cmds::logout().await,
        Command::Whoami => auth_cmds::whoami(),
        Command::Events { json, past } => catalog_cmds::events(json, past).await,
        Command::Sync { event, locale, abstracts } => catalog_cmds::sync(&event, locale, &abstracts).await,
        Command::Search { query, event, level, day, topic, service, venue, free_between, limit, json, fields } => {
            catalog_cmds::search(catalog_cmds::SearchArgs {
                query: query.join(" "),
                event,
                level,
                day,
                topic,
                service,
                venue,
                free_between,
                limit,
                json,
                fields,
            })
            .await
        }
        Command::Show { id, event, json } => catalog_cmds::show(&event, &id, json),
        Command::Init { dir, goal, interests, constraints } => plan_cmds::init(dir, goal, interests, constraints),
        Command::Plan { spec, json } => plan_cmds::plan(spec, json).await,
        Command::Apply { plan, resume, spec, accept_seat_loss, yes, json } => {
            apply_cmds::run_apply(apply_cmds::ApplyArgs { plan, resume, spec, accept_seat_loss, yes, json }).await
        }
        Command::Verify { spec, json } => apply_cmds::run_verify(spec, json).await,
        Command::Schedule { event, json } => apply_cmds::run_schedule(&event, json).await,
        Command::Doctor { event, json } => doctor::run(&event, json).await,
        Command::Mcp => crate::mcp::serve_stdio().await.map(|()| 0),
        Command::Mock { port, scenario } => mock_cmd::run(port, scenario).await,
    }
}
