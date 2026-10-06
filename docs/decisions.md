# Decisions

## D0-1 OpenAPI is authoritative (2026-10-06)
Fetched `openapi/awsevents.v1.json` (OpenAPI 3.1). Differences from the brief:
- Session short code is `abbreviation` (not `code`). There are no `start`/`end`/`allDay`/`code` fields: time is `sessionTime{date,time,length,timezone}` (local date, "HH:MM", minutes as string). `isAllDaySession`, `isReservable`, `seatAvailability` (`available|limited|veryLimited|unavailable|walkUp`; no "full" literal — `unavailable` is treated as full), `venue`, `room`.
- Session list fields: tracks, topics, industries, areasOfInterest, roles, services, segments, features, customerPersonas, experiences, additionalActivities, focusAreas. Speakers carry only `name`.
- ListSessions max page 250; `totalCount` is a number.
- Bulk failure codes: sessionNotReservable, scheduleConflict (with `conflictsWith`), alreadyScheduled, sessionFull, insufficientAccess, timePassed, alreadyFavorited, notFavorited, other. Unknown → generic refusal.
- CreatePersonalTime/UpdatePersonalTime/DeletePersonalTime/CancelReservation/DisassociateFavorite all return 204.
- Schedule: `reserved[]`, `favorites[]` (ids), `personalTime[]` with `personalTimeId`.
- Event has `authenticationRequired`, `timezone`, etc. Event `timezone` is the source for local time conversion.
- GetSchedule can return 409 (closed) in the spec; handled like other 409s.
Brief names (`code`, `walk-up`, `full`) are mapped in `src/api/types.rs`.

## D0-2 Toolchain
Rust installed with rustup (minimal profile) into `~/.cargo`. reqwest 0.13 (`rustls` feature, formerly `rustls-tls`). YAML: `serde_yaml_ng` (maintained fork of deprecated `serde_yaml`). Time: `chrono` + `chrono-tz` unified.

## D2-1 `managed` semantics
`managed(event, session_id, last_want)` is written by apply for every non-pinned desired session whose intent is observed afterwards (so a manual reservation that is then declared `want: reserved` is *adopted*). `want: none` cancels/unfavorites only when the matching last_want (`reserved` / `favorite`) is recorded; otherwise riv warns and does nothing. A reservation counts as "re-reserve" when last_want was `reserved` but it is no longer observed.

## D2-2 Plan ordering
Plan `seq` equals execution order: standalone cancels, unfavorites, replace pairs (cancel then its reserve), reserves, favorites, block.*. The executor batches consecutive same-kind actions (≤10, ≤ remaining per-minute quota); an action with `dependsOn` is always sent alone, right after its cancel.

## D2-3 Hashes
`desiredHash` covers meaning only (event, timezone, id/want/pin/replaces, blocks): editing `note`/`code` or reordering rows does not invalidate a plan. `observedHash` = reserved ids + favorite ids + personal time (title,start,end), each sorted.

## D3-1 Unknown outcomes
On any UNKNOWN the executor stops writing (remaining actions stay PLANNED) and tells the user to `riv apply --resume <runId>`. Resume reads GetSchedule, marks confirmed actions DONE, returns unconfirmed ones to PLANNED and sends only those. Cancel/unfavorite could be retried blindly (404 = done) but we reconcile uniformly, as the brief requires. 429 that survives the client's retries leaves actions PLANNED (not executed). Resume skips the observedHash check (our own writes changed the schedule) but keeps desiredHash/account checks.

## D3-2 Replace failure
If a replacement's reserve fails after its cancel succeeded, riv tries to reserve the original once and reports "restored" or "seat lost". Never guaranteed.

## D3-3 Non-interactive apply
`riv apply` prompts only on a TTY without `--yes`. The approval boundary for MCP/hooks is the planId requirement plus the host's tool-approval UI; `riv apply` without `--plan`/`--resume` fails with exit 4.

## D1-1 Search
FTS5 `unicode61` tokenizer. Natural-language queries try AND of all terms first, then top up with OR (bm25-ranked). Japanese text is indexed per whitespace/char-class boundary only (no morphological analysis), so Japanese recall is limited; a trigram index is a possible follow-up.

## D1-2 Locale sync
`--locale` adds a second full walk with that locale. Localized title/abstract are stored (`title_l10n`, `abstract_l10n`, `l10n_locale`) only when `Content-Language` equals the requested locale; otherwise a warning is printed and en-US only is kept. (Generalizes the brief's `title_ja` columns.)

## D1-3 Auth
`ulid` 3.0 targets a different `rand` major than our `rand` 0.10, so ULIDs are built with `Ulid::from_parts` (src/ids.rs). OAuth endpoints are injectable (`RIV_OAUTH_BASE`, `StoredTokens::with_oauth_base`) so refresh is tested against the mock without `set_var`.

## D4-1 MCP
rmcp 3.5 (`ServerHandler` + `#[tool_router]`/`#[tool_handler]`). Tool logic is in `mcp/tools.rs` as plain async functions (testable without the protocol); the rmcp shell converts to `CallToolResult` (errors → `isError`). Tools that touch SQLite run their non-`Send` futures on a blocking thread (`on_blocking`: `spawn_blocking` + `Handle::block_on`). Tool arguments are camelCase (`planId`, `acceptSeatLoss`, `freeBetween`, `resumeRunId`, `specPath`). Added `resumeRunId` to `riv_apply` (needed for the UNKNOWN workflow). Replies are capped near 8 KB. `riv_apply` cannot show its own confirmation: approval is the host's tool-call approval, plus the planId/hash checks.

## D4-2 Hooks & Power layout
Hooks use Kiro's current schema (`version: v1`, `trigger: PostFileSave|PreToolUse|PostToolUse`, `matcher`, `action.type: command`). They live in `dev.kiro/hooks/` as examples as the brief asks, with a README telling users to copy them to `.kiro/hooks/`. Guard/verify scripts grep the whole stdin payload, so they do not depend on payload field names (unverified; marked TODO). `plugin.json` author/repository are placeholders (`niida`, `https://github.com/OWNER/riv`) until the repo exists.

## D5-1 today
Walking time is looked up only in the user's `venues.json` (estimates); unknown pairs and the first item of a day (no origin) print "unknown" — never guessed. `sameVenueMinutes` is a config value, not assumed. GetSchedule is called once per `today` (60/min quota); no cache.

## D5-2 prep
`priorYearCandidates` are strings (hints/search material), not facts: the catalog holds one year. Code matching across years is a heuristic and the README says so.

## D6-1 Product / package name (confirmed by the user)
Product and package name: `riv-reinvent` (crates.io, npm, Homebrew formula, Power name). The command stays `riv`. `riv` alone is taken on crates.io and npm; `riv-reinvent` was free on both on 2026-10-06 (re-check right before the first publish).
