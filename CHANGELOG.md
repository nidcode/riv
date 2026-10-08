# Changelog

## 0.1.0 — unreleased
- Local catalog sync (full walk, FTS5 search), `riv search/show/sync`.
- Desired state in `design.md`, `riv plan`, safe `riv apply` (unknown-outcome reconcile via `--resume`, replace with seat-loss flag, journal, `tasks.md`), `riv verify/schedule`.
- `blocks[].want: none` deletes personal time riv created (DeletePersonalTime); `riv show --live` / `riv_session live` use GetSession for current seat availability.
- `riv bench search-quality --generate N` builds questions from the synced catalog (no hand-written fixture). CI workflow (fmt, clippy, test).
- Fix: `riv sync` no longer deletes the stored catalog when the API answers with an empty list.
- Local MCP server (`riv mcp`) and Kiro Power files (skills, steering, hook examples); Kiro Crew guide.
- `riv today` (phone/ide), `riv prep`, en/ja wording.
- Mock Events API (`riv mock`) with failure scenarios; `riv bench`.
- Releases via `dist` (shell, PowerShell, npm `riv-reinvent`; Homebrew intentionally left out for now).
