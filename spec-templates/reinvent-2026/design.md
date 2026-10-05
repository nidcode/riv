# re:Invent 2026 — Design

Declare the agenda you want. `riv plan` shows the diff against your real schedule; `riv apply` applies an approved plan.

- `id` is the API `sessionId` (find it with `riv search`), not the short code.
- `want`: `reserved` | `favorite` | `none`. Removing a row never cancels anything; use `want: none`.
- `pin: true` freezes a session: riv never touches it but still checks clashes against it.
- `replaces: <sessionId>` swaps an existing reservation (cancel first, then reserve; the old seat may be lost).
- Times in `blocks` are local to `timezone`.

```yaml
# riv:desired-state v1
event: reinvent2026
timezone: America/Los_Angeles
sessions: []
blocks: []
```
