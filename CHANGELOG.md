# Changelog

## Unreleased
- `riv logout --browser` and `riv login --switch-account` end the browser sessions so another Builder ID account can sign in.

## 0.1.1 — 2026-10-08
- Fix: `riv sync` no longer deletes the stored catalog when the API answers with an empty list.
- `riv bench search-quality --generate N`, CI workflow, `show --live`, personal-time delete (`want: none`).

## 0.1.0 — 2026-10-08
- Local catalog sync (full walk, FTS5 search), `riv search/show/sync`.
- Desired state in `design.md`, `riv plan`, safe `riv apply` (unknown-outcome reconcile via `--resume`, replace with seat-loss flag, journal, `tasks.md`), `riv verify/schedule`.
- Local MCP server (`riv mcp`) and Kiro Power files (skills, steering, hook examples); Kiro Crew guide.
- `riv today` (phone/ide), `riv prep`, en/ja wording.
- Mock Events API (`riv mock`) with failure scenarios; `riv bench`.
- Releases via `dist` (shell, PowerShell, npm `riv-reinvent`; Homebrew intentionally left out for now).
