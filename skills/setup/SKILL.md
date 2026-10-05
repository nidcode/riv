---
name: setup
description: Install riv, sign in with Builder ID, sync the re:Invent catalog and scaffold the agenda spec. Use when the user is starting with riv or something is not set up.
---

# Set up riv

Work through these steps in order. Stop at the first failure and fix it before continuing.

## Step 1: Check the environment
Run `riv doctor`. It reports the riv version, sign-in state, last catalog sync, API reachability and whether `venues.json` exists.
- `riv: command not found` → tell the user to install riv (shell installer, Homebrew, or `npx -y riv-reinvent`). See the README.

## Step 2: Sign in
Run `riv login`. It prints a URL and opens the browser; the user signs in with their Builder ID. Never ask the user to paste tokens anywhere.
- Ports 8484-8489 must be free (the sign-in callback uses one of them).
- re:Invent 2026 is registration-based: the user must be registered for the event, otherwise the catalog returns 403.

## Step 3: Sync the catalog
Run `riv sync --event reinvent2026` (add `--locale ja-JP` for Japanese text when available). The catalog is stored locally; do NOT read it into the conversation.
- Reservations and cancellations return 409 until the operation opens (re:Invent 2026: 2026-10-08). Browsing and favorites work before that.

## Step 4: Scaffold the spec
Run `riv init` and answer the three questions (purpose, interests, constraints). This creates `.kiro/specs/reinvent-2026/{requirements,design,tasks}.md`.

## Step 5: Find sessions
Use `riv_search` (or `riv search "agents" --level 300 --day tue`) to find session ids, then add them to the YAML block at the end of `design.md`.
Then follow the `replan` skill to plan and apply.
