# Progress

Resume here. Phases follow the brief (§17).

- [x] Phase 0 scaffold: crate, OpenAPI saved + conformance tests, `EventsApi`, mock server, `riv events`
- [x] Phase 1 auth + sync: PKCE/refresh/token store, HTTP client (401/429/409/5xx), `riv sync|search|show`, FTS, `scripts/check-locale.sh`, real-API read-only tests (`RIV_REAL_API=1`)
- [x] Phase 2 desired state + plan: YAML parse/validate, pure plan builder, `riv init|plan`, semantics tests (tests/plan_semantics.rs)
- [x] Phase 3 apply / verify / journal: executor state machine, batching/quota/429/409, UNKNOWN + `--resume`, replace + restore, tasks.md, `riv apply|verify|schedule`; DoD tests in tests/apply_dod.rs
- [ ] Phase 4 MCP + Power
- [ ] Phase 5 today / prep / i18n
- [ ] Phase 6 bench + docs + dist
- [ ] Phase 7 (stretch)

Needs a human (deferred): real sign-in (`riv login`) against oauth.awsevents.com, Kiro IDE import check, npm/crates name availability, GitHub repo URL / author in plugin.json, dist release tag.
