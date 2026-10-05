# riv — re:Invent as Code

Declare your re:Invent agenda as a Kiro spec (Markdown), review the diff with `riv plan`, and apply it with `riv apply`.
The catalog never enters an LLM context: only answers do. Spec of record: `docs/spec.md` (Japanese original: the pasted brief, summarized in `docs/spec.md`).
When the spec and `openapi/awsevents.v1.json` disagree, the OpenAPI wins; record the delta in `docs/decisions.md`.

## Hard rules
1. Automated tests never write to the real API (api.awsevents.com). Real-API tests need `RIV_REAL_API=1` and are read-only.
2. Never print tokens: not in logs, errors, URLs, or commits.
3. Never ship AWS catalog data. All mock data is synthetic.
4. No polling-based auto-reservation. Writes happen only from a human-approved plan.
5. `apply` without an approved plan must fail.
6. Rust stable, edition 2024, `unsafe` forbidden. Code/comments/README/article in English; `README.ja.md` and `docs/*.ja.md` in Japanese.

## Commands
`cargo build` · `cargo test` · `cargo clippy -- -D warnings` · `cargo fmt` · `cargo run -- mock` · `cargo run -- mcp`

## Structure
The lib holds the logic; `main.rs`, `mcp/`, `mock/` are thin entry points. External I/O is confined to `api/` and `auth/` and swapped through the `EventsApi` trait.
plan/apply logic is synchronous pure functions; only the I/O edges are async.

## Conventions
- No `unsafe`; no `unwrap` in non-test code (`expect` only with a reason); ~400 lines per file; no lifetimes in public APIs; errors are `RivError { code, .. }` (thiserror).
- DRY / single responsibility: domain knowledge (time conversion, rate limits, status vocab) lives in one place.
- Tests: no network; mock server runs in-process; §7/§9 rules are table-driven.
- On every change update `docs/decisions.md` and `docs/progress.md`.
