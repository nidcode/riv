# Progress

Resume here. Phases follow the brief (§17).

- [x] Phase 0 scaffold: crate, OpenAPI saved + conformance tests, `EventsApi`, mock server, `riv events`
- [x] Phase 1 auth + sync: PKCE/refresh/token store, HTTP client (401/429/409/5xx), `riv sync|search|show`, FTS, `scripts/check-locale.sh`, real-API read-only tests (`RIV_REAL_API=1`)
- [x] Phase 2 desired state + plan: YAML parse/validate, pure plan builder, `riv init|plan`, semantics tests (tests/plan_semantics.rs)
- [x] Phase 3 apply / verify / journal: executor state machine, batching/quota/429/409, UNKNOWN + `--resume`, replace + restore, tasks.md, `riv apply|verify|schedule`; DoD tests in tests/apply_dod.rs
- [x] Phase 4 MCP + Power: `riv mcp` (rmcp, 9 tools, camelCase args), plugin.json/mcp.json/mcp.local.json, skills, steering, hooks (examples), `riv doctor`, docs/crew.md. NOT verified by a human: Kiro IDE Power import, MCP Inspector
- [x] Phase 5 today / prep / i18n: `riv today` (phone/ide), venues.example.json, `riv prep` (+ --attach), riv_today/riv_prep_pack, en/ja dictionary
- [x] Phase 6 bench + docs + dist: `riv bench` (mock results in bench/results), dist-workspace.toml + release workflow (shell/powershell/npm), README en/ja, architecture, rules-compliance, article outline, spec.md, CHANGELOG. Package renamed `riv-reinvent` (`riv` taken on crates.io/npm)
- [ ] Phase 7 (stretch)

Remaining / needs a human (deferred):
- Check repository owner/author in plugin.json/Cargo.toml/dist-workspace.toml/README; create the GitHub repo + tap; add `NPM_TOKEN` and `HOMEBREW_TAP_TOKEN` secrets; tag `v0.1.0`; then verify `npx -y riv-reinvent mcp` (package has a single bin `riv`; checked locally with `dist build --artifacts=global`).
- Real `riv login` + `riv sync` on reinvent2026; fill `bench/fixtures/queries.json` (30 real questions); run `riv bench protocol` with a real token (remote MCP part may need manual measurement).
- Import the Power in Kiro IDE and run MCP Inspector against `riv mcp`; verify hook payload fields (see dev.kiro/hooks/README.md) and Crew registration steps.
- Official hackathon rules check (docs/rules-compliance.md TODO).
- Stretch (Phase 7): personal-time delete of removed blocks.

Earlier note — needs a human (deferred): real sign-in (`riv login`) against oauth.awsevents.com, Kiro IDE import check, npm/crates name availability, GitHub repo URL / author in plugin.json, dist release tag.

## Submission checklist (Catalog API Builder Challenge, deadline 2026-11-06 23:59 PT)
- [x] Article published on builder.aws.com — https://builder.aws.com/project/3KOnbvKWkMlzzcInr6wKnZsJV0x/managing-reinvent-sessions-as-code-riv
- [x] Builder Center project page created (same URL)
- [ ] README: add an explicit "Requirements" section (Rust stable for source builds, Node only for `npx`, SQLite is bundled, registered Builder ID)
- [x] Real benchmarks: first-run, warm, protocol against the real API (bench/results/2026-10-08.md)
- [x] API surface: personal-time delete (`want: none`), GetSession (`show --live`), remote MCP measured
- [x] Demo transcript (docs/demo.md); screenshots/recording still optional
- [x] Tag v0.1.0 released 2026-10-08: GitHub Releases (5 targets + shell/PowerShell installers) and npm `riv-reinvent@0.1.0` (first publish by hand with 2FA OTP; the CI `publish-npm` job got 403 because the token lacked publish rights)
- [ ] npm Trusted Publishing for the next release (so CI can publish without a token)
- [x] Hermetic tests merged; [ ] CI workflow still open
- [x] Contest entry submitted (confirmed by the participant, 2026-10-08)
- [ ] Eligibility attestations (Hero/Community Builder, registered, 18+, not an Amazon employee) — participant only
