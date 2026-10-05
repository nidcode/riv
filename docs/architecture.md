# Architecture

## One engine, three faces

```mermaid
flowchart LR
    subgraph Faces
      CLI["CLI<br/>riv plan / apply / ..."]
      MCP["MCP + Kiro Power<br/>riv mcp (stdio), skills, steering, hooks"]
      CREW["Kiro Crew<br/>Slack / Telegram -> Gateway -> riv mcp"]
    end
    CLI --> ENG
    MCP --> ENG
    CREW --> MCP
    subgraph ENG["Engine (lib `riv`)"]
      DESIRED["desired/<br/>parse + validate"]
      PLAN["plan/<br/>pure diff"]
      APPLY["apply/<br/>state machine + journal"]
      SEARCH["search/ + db/<br/>SQLite + FTS5"]
      SYNC["sync/"]
      TODAY["today/ prep/"]
    end
    SYNC --> SEARCH
    DESIRED --> PLAN --> APPLY
    SEARCH --> PLAN
    SEARCH --> TODAY
    APPLY -->|EventsApi trait| API
    SYNC -->|EventsApi trait| API
    subgraph IO["External I/O (only here)"]
      API["api/ HttpApi<br/>401 refresh, 429, 409, 5xx"]
      AUTH["auth/<br/>PKCE, token store"]
    end
    API --> AUTH
    API ==> AWS[("api.awsevents.com")]
    API -.-> MOCK["mock/ (axum, in-process)<br/>synthetic catalog + failure scenarios"]
```

The lib holds the logic; `main.rs`, `mcp/` and `mock/` are thin entry points. Everything that talks to the network goes through the `EventsApi` trait (`api/`) and `auth/`, so tests swap in the mock server on a free port. plan/apply decisions are synchronous pure functions over plain data (`plan::build_plan`, `apply::verify`); only the I/O edges are async.

## Where each rule lives (single source)

| Knowledge | Location |
|---|---|
| REST operations (method + path) | `src/api/ops.rs`, checked against `openapi/awsevents.v1.json` by `tests/openapi_conformance.rs` |
| Per-minute quotas | `src/api/quota.rs` |
| UTC / local / wire time formats | `src/timeutil.rs` |
| Desired-state semantics | `src/plan/build.rs`, fixed by `tests/plan_semantics.rs` |
| Apply semantics | `src/apply/exec.rs`, fixed by `tests/apply_dod.rs` |
| Words shown to users | `src/i18n/` (English text is the key) |

## plan / apply state machine

```mermaid
stateDiagram-v2
    [*] --> Planned: riv plan (saved, 30 min expiry)
    Planned --> Rejected: expired / spec hash changed /<br/>schedule hash changed / other account /<br/>missing --accept-seat-loss
    Planned --> Executing: riv apply --plan (confirmed)
    state Executing {
      [*] --> Action
      Action --> DONE: 2xx, success in bulk result
      Action --> ALREADY: already reserved/favorited, 404 on removal
      Action --> FAILED: full, clash, not reservable, ...
      Action --> SKIPPED: 409 closed (same kind never re-sent), dependency not done
      Action --> UNKNOWN: timeout / drop / 500 / 503
    }
    Executing --> Stopped: any UNKNOWN (writes stop)
    Stopped --> Executing: riv apply --resume (GetSchedule reconcile, send only the missing)
    Executing --> Verified: GetSchedule read-back, tasks.md regenerated
    Verified --> [*]
```

A replacement is `cancel` then `reserve` (the reserve `dependsOn` the cancel). If the reserve fails after the cancel succeeded, riv tries to reserve the original once and reports "restored" or "seat lost" — never promised.

## Data

SQLite (`~/.local/share/riv/riv.db`): `events`, `sessions` (+ raw JSON and hash), `sessions_fts`, `sync_runs`, and the journal: `runs`, `run_actions`, `managed`, `block_ids`, `prep_notes`. Plans are JSON files in `~/.local/share/riv/plans/`.
