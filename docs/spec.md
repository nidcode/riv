# riv — re:Invent as Code: implementation spec (English)

English rendering of the Japanese implementation brief that governs this repository. Precedence when documents disagree: **this spec → AWS Events API official documentation → OpenAPI definition**, except that where an API *fact* in this spec contradicts the saved OpenAPI (`openapi/awsevents.v1.json`), the OpenAPI wins and the delta is recorded in `docs/decisions.md` (see D0-1: e.g. `abbreviation` instead of `code`, `sessionTime` instead of `start/end`, `seatAvailability` enum).

## 0. Working rules
- Implement in phases (§17). At the end of each phase: `cargo test` green, commit, update `docs/progress.md` (state) and `docs/decisions.md` (reasons).
- Decide yourself and record it; ask only for irreversible decisions.
- First task: fetch `https://api.awsevents.com/v1/openapi.json` into `openapi/awsevents.v1.json`; hand-write types (12 operations) and test that paths, methods and required fields match. Never guess REST paths or field names.
- Official docs: `https://docs.aws.amazon.com/events/latest/devguide/` (each page has a `.md` variant). Kiro specs: `kiro.dev/docs/{powers/create,hooks/types,hooks,crew/interfaces}`.

### Absolute rules
1. Automated tests never write to the real API. Real-API tests need `RIV_REAL_API=1` and are read-only.
2. Never emit tokens in logs, error messages, URLs or commits.
3. Never ship AWS catalog data; all mock data is synthetic.
4. No polling-based auto-reservation. Writes come only from a human-approved plan.
5. `apply` without an approved plan fails.
6. Rust stable, edition 2024, no `unsafe`. Code, comments, `README.md`, article outline in English; `README.ja.md` and `docs/*.ja.md` in Japanese.

## 1. Product
**One sentence**: declare the re:Invent agenda as a Kiro spec; `riv plan` shows the diff; `riv apply` reserves/favorites. The catalog stays out of LLM context; only answers return.
**Core value**: change your goals safely while keeping seats already held.
**Users**: AWS Heroes / Community Builders registered for re:Invent 2026, mainly Kiro users. **Runs on the attendee's PC** (tokens may only go to api.awsevents.com; no hosted component).
**Non-goals**: team-wide coverage planning, a custom Slack bot (Crew configuration and docs only), video transcripts, full personal-time CRUD (create/update done; delete is stretch), implementing all 12 operations for their own sake.

## 2. Stack (decided)
Rust stable/edition 2024 single crate (lib `riv` + bin `riv`); `tokio` only at I/O edges, plan/apply logic synchronous; `clap` derive; `reqwest` (rustls, json); `rusqlite` bundled with FTS5 (blocking calls via `spawn_blocking` in the MCP server); `serde`/`serde_json`, YAML via a maintained crate (`serde_yaml_ng`); hand-written PKCE (`sha2`+`base64`+`rand`), callback via `axum`; `rmcp` (official MCP SDK; server + stdio, client for benches); mock server on `axum`; one date-time crate (`chrono` + `chrono-tz`); `ulid`; `directories`; `thiserror` (library, `RivError { code, .. }`) + `anyhow` (bin only); `tracing` (never tokens); tests with `cargo test`, `assert_cmd`; `cargo fmt`, `cargo clippy -- -D warnings`, deny `clippy::unwrap_used` outside tests; distribution via `dist` (shell/powershell/homebrew/npm; npm package `riv-reinvent`, run as `npx -y riv-reinvent mcp`); Apache-2.0. Config in `$XDG_CONFIG_HOME/riv` or `~/.config/riv`; data in `$XDG_DATA_HOME/riv` or `~/.local/share/riv`; Windows `%APPDATA%\riv`.
Conventions: no lifetimes in public APIs; state as enums; traits only for external I/O (`EventsApi`); simplify rather than pile up `Arc<Mutex>`.

## 3. Layout
Power files at the repository root (`plugin.json`, `mcp.json`, `skills/{setup,replan,prep}`, `dev.kiro/{steering,hooks}`), `spec-templates/reinvent-2026/`, `openapi/`, `src/{api,auth,db,sync,search,desired,plan,apply,today,prep,i18n,format,cli,mcp,mock}`, `tests/`, `bench/`, `config/venues.example.json`, `docs/`, `README(.ja).md`, `CLAUDE.md`, `LICENSE`.

## 4. External API facts (AWS Events API)
- Base `https://api.awsevents.com`, paths under `/v1`, JSON; OpenAPI at `/v1/openapi.json`. 12 operations: ListEvents, GetEvent, ListSessions, GetSession, GetSchedule, ReserveSessions, CancelReservation, AssociateFavorites, DisassociateFavorite, CreatePersonalTime, UpdatePersonalTime, DeletePersonalTime.
- ListEvents/GetEvent never need sign-in. Registration-free events expose the catalog anonymously; registration events (`reinvent2026`) need sign-in + registration even for reading.
- No catalog search/filter in the API: download everything, filter locally.
- ListSessions pages: size chosen by the service; follow `nextToken` until it is absent (a short page is not the last page); `totalCount` on every page; `includeAbstracts=false` omits abstracts; `locale` selects language, the real language is the `Content-Language` header (default en-US). Read every field defensively.
- ReserveSessions: 1–10 unique ids; per-session success/failure with reason codes (full, schedule conflict with the conflicting items, already scheduled, ...); unknown codes = unactionable refusal. **200 is not success**; "already reserved" is a failure, so resending a request is not a safe retry.
- Quotas per minute: GetSession 120, ListSessions 120, GetSchedule 60, ReserveSessions 30 *sessions*, CancelReservation 30, AssociateFavorites 30 *sessions*, DisassociateFavorite 30, Create/Update/DeletePersonalTime 30 each.
- 429: wait `Retry-After`, retry; a batch larger than the remaining quota is shrunk and retried at once (a refused request spends no quota). 409: operation closed — do not retry immediately (re:Invent 2026 reserve/cancel: 409 until 2026-10-08).
- No idempotency key: after an unknown outcome (timeout, drop, 500, 503) never resend as is — compare `GetSchedule` with intent and send only what is missing. 404 on cancel/unfavorite = done. 409/429 = not executed.
- Personal time: `startDateTime`/`endDateTime` are UTC `YYYY-MM-DDTHH:mm:ss`, no offset/`Z`, seconds `00`, multiples of 5 minutes; title 1–128, description 1–250 required, location ≤255 optional; Create returns no body (read the id from GetSchedule); Update replaces all fields.
- Error bodies need not be JSON (edge may return empty/HTML). Remote MCP at `https://api.awsevents.com/mcp` is only a benchmark comparison target.

## 5. Authentication
OAuth 2.0 authorization code + PKCE (S256), no client secret. Authorize `https://oauth.awsevents.com/oauth2/authorize`, token `.../oauth2/token`, revoke `.../oauth2/revoke`; client id `7vmom55m1qstvq8i71ph127bfq`; scope `openid email events/access`; `identity_provider=AWSBuilderID`, `code_challenge_method=S256`, `state`. Redirect `http://localhost:{port}/callback` (or 127.0.0.1), ports **8484–8489 only**, identical in authorize and token calls. Access 60 min (JWT), refresh 30 days (opaque); the id token is never sent to the API.
Verifier 43–128 unreserved chars from a CSPRNG, fresh per attempt; challenge = base64url(SHA-256) without padding. Probe ports from 8484; error if all busy; print the URL when no browser. Tokens in `~/.config/riv/credentials.json` mode 0600 (behind a `TokenStore` trait for future keychains). 401 → refresh once → retry once. `riv logout` revokes then deletes locally and points to the Builder ID logout URL. `riv whoami` masks the email. `RIV_API_BASE`, `RIV_TOKEN` (skips PKCE; mock).

## 6. Sync and local DB
`riv sync [--event] [--locale] [--abstracts auto|always|never]`: auto = a no-abstract walk then a walk with abstracts; progress from `totalCount`; no fixed sleeps, wait only on 429. Localized columns are stored only when `Content-Language` matches the request. Tables: `events`, `sessions` (raw JSON + extracted columns), `sessions_fts`, `sync_runs` (times, counts, locale, `catalog_version`). Always a full walk; changes counted by raw-hash comparison. `scripts/check-locale.sh` prints `Content-Language` for a registration-free event.

## 7. Desired state
One YAML block at the end of `design.md`, first line `# riv:desired-state v1`: `event`, `timezone`, `sessions[] {id, code?, want: reserved|favorite|none, pin, replaces?, note?}`, `blocks[] {key, title, description, start, end, location?}` (local times, converted to UTC/seconds 00/no `Z` when sent).
Semantics (fixed by tests): observed reservation absent from the spec = **unmanaged** (untouched, counted for clashes, listed); deleting a row only returns it to unmanaged (cancel needs `want: none`); `want: reserved` but not observed = **re-reserve (may be full)**; `want: none` on an observed reservation cancels only if managed, else warns; `pin: true` = excluded from the plan, used only for clash checks; `replaces` = cancel then reserve, `risk: seat-loss`, apply needs `--accept-seat-loss`; same-title repeat runs are addressed by sessionId and the plan offers other runs as `alternatives` when full; re-applying the same state writes nothing; validation errors (unknown sessionId, bad `want`, duplicate id, self-`replaces`, block not a 5-minute multiple) reject the plan.

## 8. plan
`riv plan [--spec] [--json]`: parse → GetSchedule → normalize → diff → save `plan.json` under `~/.local/share/riv/plans/<planId>.json` → show. Always dry-run. Vocabulary: `+ reserve`, `- cancel`, `-/+ replace (seat may be lost)`, `~ favorite`, `# unmanaged (untouched)`, `! re-reserve (may be full)`. `planId` ULID, `expiresAt` = +30 min, `observedHash` over the normalized schedule, `desiredHash` over the normalized YAML. JSON fields: schemaVersion, planId, createdAt, expiresAt, event, account, desiredHash, observedHash, catalogVersion, actions[{seq, kind, sessionId, code, reason, risk, dependsOn, ...}], unmanaged, alternatives, warnings, requiredFlags.

## 9. apply / verify / journal
`riv apply --plan <planId> [--accept-seat-loss] [--yes]`: (1) reject expired plans; (2) reject if `desiredHash` differs; (3) reject if `observedHash` differs; (4) enforce `requiredFlags`; (5) confirm on a TTY without `--yes`; (6) order cancel → unfavorite → reserve → favorite → block.* with replace pairs back to back; (7) batches ≤10 and ≤ remaining quota, 429 waits, a 409 skips that kind; (8) statuses `PLANNED|DONE|ALREADY|FAILED|UNKNOWN|SKIPPED`; (9) any UNKNOWN stops further writes; reconcile with GetSchedule and resend only the missing (`riv apply --resume <runId>`); (10) a failed replacement after a successful cancel tries to restore the original **once** and says whether the seat was lost or restored (never guaranteed); (11) always verify with GetSchedule at the end and regenerate `tasks.md` (display only, "This file is generated by riv. Do not edit.").
Journal (SQLite): `runs`, `run_actions`, `managed`, `block_ids`. `riv verify` only compares and prints.
**Definition of done** (mock-server tests): manual reservations untouched; deleting a row never cancels; re-applying writes 0; unknown outcomes never resent without reconciliation; replacements show the seat-loss risk and cannot be applied without the flag.

## 10. CLI
`init, login, logout, whoami, events, sync, search [--level --day --topic --service --free-between --limit --json --fields], show, schedule, plan, apply, verify, today [--date --format phone|ide], prep [--lang ja|en --json | --attach], doctor, mcp, mock [--port 8787 --scenario], bench`. `--json` is machine output; `RIV_LANG=ja|en` switches wording. Exit codes 0 ok, 1 error, 2 validation, 3 auth, 4 plan rejected, 5 partial failure.

## 11. Local MCP server (stdio)
Few tools, small replies (default `limit: 10`, ~8 KB), `format: ide|phone` everywhere: `riv_status`, `riv_search`, `riv_session`, `riv_schedule`, `riv_plan`, `riv_apply` (planId required; description says it needs human approval via the Kiro/Crew button and the planId comes from the preceding `riv_plan`), `riv_verify`, `riv_today`, `riv_prep_pack`.

## 12. Kiro Power, skills, hooks, steering, Crew
`plugin.json` + `mcp.json` (`npx -y riv-reinvent@latest mcp`) at the repository root; `mcp.local.json` for installed/dev use. Skills `setup`, `replan` (never `/yolo`, always state seat-loss risk), `prep`. Steering: answer in the user's language, apply needs approval, never load the whole catalog, show venue-local time. Hooks (examples, per Kiro's current schema): `riv-plan-on-save` (File Save on design.md — fires only for agent changes), `riv-guard-apply` (Pre Tool Use: block `riv apply` without `--plan`), `riv-verify-after-apply` (Post Tool Use). `docs/crew.md`: register the MCP + skills in a local Crew Gateway on the attendee's PC; approval by Crew buttons; `format: phone`; no Slack code.

## 13. today / prep / i18n
- today: GetSchedule + local catalog + `config/venues.json` (estimates the user edits; unknown pairs print "unknown", never invented). `phone`: ≤12 lines, no tables, local times, "walk N min · leave by HH:MM". `ide`: Markdown tables allowed.
- prep: JSON with session facts, `related` (same day/topic, ≤5), `priorYearCandidates` (local hints only), `queries` (e.g. `"<code> re:Invent 2025"`; for `lang=ja` also report/DevelopersIO/Qiita queries), `noteTemplate`, `savePath` (`.kiro/specs/reinvent-2026/prep/<code>.md`); `riv prep --attach` records a note so `today` shows "prep note available". Year-to-year code matching is a heuristic (say so in the README).
- i18n: `RIV_LANG` (default OS locale); en/ja dictionaries; localized catalog text only when the sync obtained it.

## 14. Mock API server
Synthetic only. Implements the operations with OpenAPI-shaped responses; events `demo-public` (no auth, no schedule) and `demo-reinvent` (Bearer `mock-token`); ~120 deterministic sessions over 5 days / 4 venues, levels 100–400, tags, several `-R1/-R2` pairs, generated abstracts, availability mix, deliberately variable page sizes with `nextToken`; mutable in-process schedule; scenarios via `--scenario` and `X-Riv-Scenario`: `closed-reservations`, `throttle:N`, `full:<sessionId>`, `clash`, `drop-after-write`, `edge-html-500`.

## 15. Benchmarks
`riv bench protocol|first-run|warm|search-quality`, results in `bench/results/<date>.md` with environment, procedure and raw data; claim only what was measured. `protocol`: REST vs remote MCP for one page (rmcp streamable-HTTP client; fall back to a documented manual measurement if sign-in cannot be automated). `search-quality`: recall@10 over 30 user-supplied questions (`bench/fixtures/queries.json`) and 10 mock questions, with and without abstracts. No repeated real reservation benchmarks.

## 16. Documentation
README (EN/JA): three install routes, 30-second mock quick start, real-API steps, Kiro Power import, safety model, known limits (year-to-year code matching heuristic, estimated walking times, File Save hooks only for agent changes). `docs/architecture.md` (Mermaid: one engine, three faces; plan/apply states), `docs/rules-compliance.md` (officially forbidden vs self-imposed), `docs/article-outline.md` (problem → design → Kiro integration → benchmarks → limits and credit to reinvent26-planner, re:Plan 2026, reinvent-scout, reinvent2026-mcp → usage), `docs/decisions.md`, `docs/progress.md`.

## 17. Phases and acceptance
0 scaffold (`riv events` works against the mock) · 1 auth + sync + search (full variable-page sync on the mock; read-only real-API checks) · 2 desired state + plan (every §7 case unit-tested; plan never writes) · 3 apply/verify/journal (the five DoD cases) · 4 MCP + Power (tools callable from MCP Inspector; Kiro import documented) · 5 today/prep/i18n · 6 benches + docs + dist (a stranger completes the mock demo from the README; a tag publishes binaries and the npm wrapper) · 7 stretch (personal-time delete, `--locale ja-JP`, detailed Crew verification).

## 18. CLAUDE.md
One-line product definition and the six absolute rules; commands; structure (lib = logic, bin/mcp/mock thin; I/O confined to `api/` and `auth/` behind `EventsApi`); coding rules (no `unsafe`, no `unwrap` outside tests, ≤400 lines/file as a guide, no lifetimes in public APIs, errors as `RivError { code, .. }`); testing rules (no network, in-process mock, table-driven §7/§9); update `docs/decisions.md` and `docs/progress.md` on every change.
