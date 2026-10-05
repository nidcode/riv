---
name: prep
description: Write a short prep note for a re:Invent session (what to know before attending), using the local catalog and web search. Quotes are short, sources are linked, no video transcripts.
---

# Prep note for a session

1. Get the pack: `riv_prep_pack` with the session id or code (`lang: ja` for Japanese queries). It returns the session facts, related sessions, prior-year hints and suggested search queries.
2. Run the suggested `queries` with your web search tool. Prefer official AWS pages, AWS blogs, and the speakers' own posts.
3. Write the note to the pack's `savePath` (`.kiro/specs/reinvent-2026/prep/<code>.md`) using this template:

```
# <code> — <title>
**In 3 lines**: ...
**Last year's version**: ... ([source](url))
**What changed**: ...
**Terms to know**: ...
**Questions to ask**: ...
**Related sessions**: ...
```

## Rules
- Quote at most a sentence or two; paraphrase the rest. Every claim gets a source link.
- Do NOT fetch or reproduce video transcripts.
- Year-over-year matching by session code is a heuristic; say so when you rely on it.
- Then register the note: `riv prep --attach <id> <path>` so `riv today` can show "prep note available".
