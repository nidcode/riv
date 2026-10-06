# Builder Center article — outline

Working title: **re:Invent as Code: plan your agenda as a spec, without losing the seats you already have**

1. **The problem**
   - The AWS Events API has no catalog search/filter; clients must download everything.
   - Through the remote MCP server every page of the catalog flows through the model's context (and each session needs sign-in); a week of re-planning burns tokens and patience.
   - Changing your goals mid-week is exactly when you must not lose a seat you already hold.
2. **Design**
   - The catalog stays in a local SQLite + FTS5 database; the model only ever sees answers (small replies, `limit`, `format: phone|ide`).
   - Declare intent in a Kiro spec (`design.md` YAML: `want`, `pin`, `replaces`); requirements stay human prose.
   - Terraform-style `plan` / `apply`: diff vocabulary, hashes of spec and observed schedule, 30-minute expiry, account binding.
   - **Irreversibility**: reservations can be lost. Replacement = cancel then reserve, flagged `seat-loss`, needs `--accept-seat-loss`; one restore attempt, never promised. Unmanaged reservations are untouched. No idempotency key → unknown outcomes stop the run and are reconciled against `GetSchedule` before anything is re-sent. 200 is not success (per-session failures), 409/429 are "not executed".
   - Show the DoD tests as the executable form of these promises.
3. **Kiro integration**
   - Power (GitHub import, `npx -y riv-reinvent mcp`), skills (`setup`, `replan`, `prep`), steering, hook examples (and the honest limit: file hooks fire only for agent edits).
   - Kiro Crew: Gateway on your own PC, `format: phone`, approval buttons — never `/yolo`.
4. **Benchmarks** (only what was measured; see `bench/results/`)
   - REST vs remote MCP for one ListSessions page: time and bytes (manual measurement if the OAuth client flow cannot be automated).
   - First sync with/without abstracts; warm search p50/p95; recall@10 on 30 real questions (author fills `bench/fixtures/queries.json`; the mock fixture proves the harness only).
5. **Limits and respect for prior work**
   - Heuristic code matching across years, estimated walking times, agent-only file hooks, FTS without Japanese morphology.
   - Credit and link: **reinvent26-planner**, **re:Plan 2026**, **reinvent-scout**, **reinvent2026-mcp** — what each does well, and what riv adds (declarative spec + safe apply for already-held seats).
6. **Try it**
   - 30-second mock quick start (`riv mock` → `sync` → `search` → `plan` → `apply`), then real sign-in; install via shell/npx; repository link; Apache-2.0.
