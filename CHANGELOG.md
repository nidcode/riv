# Changelog

## Unreleased
- `riv logout` now also ends the Builder ID and sign-in browser sessions (`--local` keeps token-only); `riv login` shows which account you got and asks before saving (`--yes` skips); `riv login --switch-account` signs the browser out first. Masked emails show two characters (`al***@…`). Fixes being signed in silently as an account you did not choose.

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
