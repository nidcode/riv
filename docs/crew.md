# Using riv from your phone with Kiro Crew

Kiro Crew is one Gateway reachable from the desktop app, a web dashboard, or Slack/Telegram/Discord/etc. riv plugs into it as a local MCP server plus skills. **riv has no hosted part**: your access token may only be sent to `api.awsevents.com`, so the Gateway must run on *your* machine (your hotel laptop or a machine at home), and riv runs next to it.

```
Phone (Slack / Telegram)  ->  Crew channel  ->  Gateway (your PC)  ->  riv mcp (stdio)  ->  api.awsevents.com
                                   ^ approval buttons
```

## 1. Prepare the machine
1. Install riv and run `riv login`, then `riv sync --event reinvent2026` once (see README). Credentials are stored in `~/.config/riv/credentials.json` (0600). Do this *before* you travel; sign-in needs a browser on that machine.
2. Install Kiro Crew and start the Gateway (`kirocrew gateway`). Keep it running (see Crew "Running 24/7"). Channels connect outbound from the Gateway, so no port needs to be exposed publicly.
3. Create a project folder for the agenda with `riv init` so `.kiro/specs/reinvent-2026/` exists on that machine.

## 2. Register riv's MCP server and skills
Add the MCP server to the Gateway's MCP configuration (Crew dashboard → MCP panel, or its config file) with the same definition as `mcp.local.json`:

```json
{ "mcpServers": { "riv": { "type": "stdio", "command": "riv", "args": ["mcp"], "env": { "RIV_SPEC": "/path/to/.kiro/specs/reinvent-2026/design.md" } } }
}
```

Per the Crew docs (checked 2026-10-08):
- **MCP**: dashboard → *Agent Capabilities → Integrations (MCP)* → add a server by command (`riv` with args `mcp`), then *Probe* it. *Discover & Sync* also picks up servers from existing MCP config files.
- **Skills**: copy the folders into `~/.kiro/crew/skills/` (e.g. `cp -r skills/* ~/.kiro/crew/skills/`; each needs `SKILL.md`), or import from GitHub via *Settings → Skills → Discover* (`nidcode/riv:skills/replan`). Imports are snapshots; re-import to update. Optional frontmatter keys: `triggers`, `always`.
- Settings live in `~/.kiro/crew/` (or `KIROCREW_HOME`); `kirocrew config get|set|edit`.
- Not verified here: how Crew shows tool approvals for a stdio MCP tool — test `riv_apply` once and confirm you get an approval prompt before relying on it.

## 3. Approvals (important)
`riv_apply` is the only tool that writes. Keep it on **manual approval**: when the agent calls it, Crew shows an approval button in the chat; tap it only after you read the diff the agent showed you.
- Do **not** use `/yolo` or per-session Trust for riv. The `replan` skill and steering say the same.
- `riv_apply` needs a `planId` from the previous `riv_plan`; a plan expires after 30 minutes and is refused if your schedule or spec changed in between.

## 4. Phone-friendly output
Every riv tool takes `format: "phone"`: at most 12 short lines, no tables, local times, and for `riv_today` a "leave by HH:MM" hint per item. Ask the agent, for example:
- "What's on today? (phone format)" → `riv_today`
- "Find level 300 agentic sessions on Tuesday afternoon" → `riv_search` with filters
- "Swap AIM301-R1 for SVS302" → `replan` skill: spec edit → `riv_plan` → diff in chat → your approval → `riv_apply` → `riv_verify`

## 5. What is not here
riv ships no Slack/Telegram code. Channel setup is Crew's: follow Crew's own Slack/Telegram guides and its allow-list/security settings, and restrict the bot to your own account.
