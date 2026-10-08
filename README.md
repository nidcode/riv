# riv — re:Invent as Code

Declare your re:Invent agenda as a [Kiro](https://kiro.dev) spec (Markdown), review the diff with `riv plan`, apply it with `riv apply`. The session catalog lives in a local SQLite database and **never enters an LLM context** — only answers do.

> Central promise: change your goals for the week and update your agenda **without losing the seats you already hold**. Anything riv did not create is never touched, replacements say out loud that a seat may be lost, and nothing is written without a plan you approved.

For AWS Heroes / Community Builders registered for re:Invent 2026, mostly Kiro users. riv runs **on your own machine** (the API forbids sending your token anywhere but `api.awsevents.com`, so there is no hosted part). Japanese: [README.ja.md](README.ja.md).

Article (AWS Builder Center): https://builder.aws.com/project/3KOnbvKWkMlzzcInr6wKnZsJV0x/managing-reinvent-sessions-as-code-riv  
Demo of a replacement and the seat-loss guard: [docs/demo.md](docs/demo.md)

## Install

Pick one:

```sh
# 1. shell installer (macOS / Linux)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/nidcode/riv/releases/latest/download/riv-reinvent-installer.sh | sh
# Windows: powershell -c "irm https://github.com/nidcode/riv/releases/latest/download/riv-reinvent-installer.ps1 | iex"

# 2. npx (no install; this is what the Kiro Power uses)
npx -y riv-reinvent --help
```

From source (developers): `cargo install --path .` (needs Rust stable, edition 2024).

## 30-second quick start (no sign-in, synthetic data)

```sh
riv mock &                                   # mock Events API on http://localhost:8787 (synthetic catalog)
export RIV_API_BASE=http://localhost:8787 RIV_TOKEN=mock-token RIV_EVENT=demo-reinvent
riv sync                                     # downloads all pages (page sizes vary on purpose)
riv search agents bedrock --fields id,code,title,start,seat --limit 5
riv init --event demo-reinvent --goal "learn agents" --interests "Bedrock" --constraints "none"
```
Open `.kiro/specs/reinvent-2026/design.md`, and in the YAML block at the end replace `sessions: []` with one session whose seat is `available`:
```yaml
sessions:
  - {id: "mock-0001", want: reserved}
```
```sh
riv plan                                     # shows the diff; writes nothing to the API
riv apply --plan <planId printed by riv plan>   # asks to confirm on a TTY (or pass --yes)
riv verify                                   # spec vs real schedule
riv schedule                                 # what you hold now
cat .kiro/specs/reinvent-2026/tasks.md       # generated checklist (do not edit)
```
Try failures: `riv mock --scenario closed-reservations` (409), `throttle:5`, `full:<sessionId>`, `clash`, `drop-after-write` (unknown outcome), `edge-html-500`.

## With the real API (read-only to start)

```sh
riv doctor            # sign-in, catalog, API reachability
riv events            # public listing, no sign-in
riv login             # Builder ID, OAuth 2.0 + PKCE; the callback uses one of ports 8484-8489
riv sync              # event reinvent2026; add --locale ja-JP for localized text when the server provides it
riv search "agentic" --level 300 --day tue --free-between 13:00-15:00
```
re:Invent 2026 is registration-based: catalog reads need sign-in **and** event registration. Reservations/cancellations return `409` until the operation opens (2026-10-08 for re:Invent 2026); browsing and favorites work before that. Credentials are stored in `~/.config/riv/credentials.json` (mode 0600; on Windows under `%APPDATA%\riv`, protected only by your user profile — treat the machine accordingly). `riv logout` revokes and deletes them.

## Kiro Power

This repository root *is* the Power (`plugin.json`, `mcp.json`, `skills/`, `dev.kiro/`).
- **From GitHub**: Kiro IDE → Powers → *Import from GitHub* → this repository's URL. `mcp.json` starts the server with `npx -y riv-reinvent@latest mcp`.
- **Local / before the npm release**: use `mcp.local.json` (runs the installed `riv mcp`), or in a checkout `cargo run --quiet -- mcp`.
- Tools: `riv_status`, `riv_search`, `riv_session`, `riv_schedule`, `riv_plan`, `riv_apply`, `riv_verify`, `riv_today`, `riv_prep_pack` (small replies; every one takes `format: "ide"|"phone"` where it matters).
- Skills: `setup`, `replan`, `prep`. Steering: `dev.kiro/steering/riv.md`. Hook examples: `dev.kiro/hooks/` (copy to `.kiro/hooks/`).
- Check the tools without Kiro: `npx @modelcontextprotocol/inspector riv mcp`.
- From your phone through Kiro Crew: [docs/crew.md](docs/crew.md).

## How a change is applied (safety model)

1. **Spec**: the YAML block in `design.md` says what you want (`reserved | favorite | none`, `pin`, `replaces`).
2. **Plan**: `riv plan` reads your real schedule and writes `plan.json` (30-minute expiry, bound to the spec hash, the observed-schedule hash and your account). Vocabulary: `+ reserve`, `- cancel`, `-/+ replace (seat may be lost)`, `~ favorite`, `# unmanaged (untouched)`, `! re-reserve (may be full)`.
3. **Apply**: only with `--plan`. Refused when the plan expired, the spec or schedule changed, the account differs, or `--accept-seat-loss` is missing for a replacement. Execution order: cancel → unfavorite → reserve → favorite → personal time; batches of ≤10 within the per-minute quota; `429` waits `Retry-After`; `409` skips that kind and goes on.
4. **Unknown outcomes** (timeout, dropped connection, 500/503) are never resent blindly: writes stop, and `riv apply --resume <runId>` reconciles with `GetSchedule` and sends only what is missing.
5. **Verify** after every apply, and `tasks.md` is regenerated.

What riv will **not** do: poll for seats / auto-reserve, apply without an approved plan, or touch reservations it did not create (they show as `unmanaged`; deleting a spec row never cancels anything — write `want: none`). Details: [docs/rules-compliance.md](docs/rules-compliance.md), [docs/architecture.md](docs/architecture.md).

## Switching accounts

Your **browser** can stay signed in to Builder ID and sign you straight back in as an account you did not pick (the authorization endpoint ignores `prompt=login`). So `riv logout` also ends the browser sessions, and `riv login` shows **which account** you got (e.g. `al***@example.com`) and asks `Use this account? [Y/n]` before saving anything; answer `n` and it signs the browser out and lets you choose again. To switch accounts:

```sh
riv logout                # revokes the tokens AND ends the browser sessions (opens your browser)
riv login                 # sign in; check the account shown, answer n to pick another
# or in one step:
riv login --switch-account
```
`riv logout --local` only deletes the tokens (for machines without a browser); `riv login --yes` skips the question. If the redirect chain cannot run, sign out of the browser at https://profile.aws.amazon.com instead.

## Commands

`riv show <id> --live` asks the API (GetSession) for the current seat availability instead of the last sync. Personal-time blocks in the spec can be created, updated and, with `want: none`, deleted.

`init`, `login`, `logout`, `whoami`, `events`, `sync`, `search`, `show`, `schedule`, `plan`, `apply`, `verify`, `today`, `prep`, `doctor`, `mcp`, `mock`, `bench`. `--json` where a machine reads the output; `RIV_LANG=ja|en` switches wording. Exit codes: 0 ok, 1 error, 2 validation, 3 auth, 4 plan rejected, 5 partial failure.

Language: `riv config set lang ja` saves it (`riv config show` explains where the effective value comes from). Order: `RIV_LANG` > saved config > OS locale.

Environment: `RIV_API_BASE` (default `https://api.awsevents.com`), `RIV_TOKEN` (skips PKCE; mock only), `RIV_EVENT`, `RIV_SPEC`, `RIV_LANG`, `RIV_CONFIG_DIR` / `RIV_DATA_DIR`, `RIV_LOG`.

## Known limitations

- Matching a session code across years (`prep`'s prior-year hints) is a **heuristic**; codes change between years.
- Walking times in `config/venues.example.json` are **estimates**. Copy it to `~/.config/riv/venues.json` and edit; pairs you do not list show as "unknown".
- Kiro File Save hooks fire only for **agent** changes, not for your manual saves. After editing `design.md` by hand, run `riv plan` (or `/riv-plan`).
- Search is FTS5 without Japanese morphological analysis; Japanese recall is limited.
- `riv_apply` over MCP relies on the host's tool-approval UI; keep it on manual approval (never `/yolo`).
- Windows credentials are a plain file under your profile.

## Development

`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`. Tests never touch the network except `RIV_REAL_API=1` (read-only). The mock server runs in-process on a free port. Benchmarks: `riv bench {protocol|first-run|warm|search-quality} [--mock]`, results in `bench/results/`. Releases: tag `vX.Y.Z`; the `dist` workflow publishes binaries, the npm package (`NPM_TOKEN` secret).

All catalog data in this repository is synthetic. Not affiliated with AWS. License: Apache-2.0.
