---
inclusion: always
---

# riv (re:Invent as Code)

- Answer in the user's language.
- NEVER call `riv_apply` / `riv apply` without showing the diff from `riv_plan` and getting explicit approval. No auto-approve, no `/yolo`.
- Never load the whole session catalog into the context. Use `riv_search` with filters and small limits; fetch details with `riv_session`.
- Show times in the venue's local time (America/Los_Angeles for re:Invent in Las Vegas), not UTC.
- Mention seat-loss risk in words whenever a plan contains a replacement.
- Do not edit `tasks.md`; riv generates it.
- Never print, log or ask for access tokens.
