---
name: replan
description: Change the re:Invent agenda safely when plans change (new goals, a clash, a full session). Updates the spec, shows the diff, applies only after explicit approval, then verifies.
---

# Replan the agenda

Use this whenever the user's plans or constraints change. Never skip the approval step.

## Rules
- Do NOT use `/yolo`, auto-approve, or "trust" settings for `riv_apply`. A person approves every apply.
- Do NOT apply anything the user has not seen as a diff.
- Do NOT poll for open seats or retry reservations in a loop.
- Show times in the venue's local time zone.

## Steps
1. **Update `requirements.md`** with what changed (purpose, constraints), in the user's words.
2. **Update the desired state** in the YAML block at the end of `design.md`:
   - `want: reserved | favorite | none`; deleting a row never cancels anything, use `want: none`.
   - `pin: true` for sessions riv must not touch.
   - `replaces: <sessionId>` to swap a reservation (cancel first, then reserve).
3. **Plan**: call `riv_plan`. Show the user the diff in plain words (what will be reserved, cancelled, favorited; what is untouched; any warnings and alternatives).
4. **Say the risk out loud** for every replacement: "this cancels your current seat first; if the new session is full, you may lose the seat. riv will try once to restore it, but that is not guaranteed." Replacements need `acceptSeatLoss`.
5. **Wait for explicit approval.** Then call `riv_apply` with the `planId` from the plan you just showed (and `acceptSeatLoss: true` only if the user accepted the risk).
6. **Verify**: read the result; if any action is UNKNOWN, call `riv_apply` with `resumeRunId` (it reconciles with the schedule first and never blindly resends). Then call `riv_verify` and summarize: what is reserved, what failed and why (full / clash / closed), what to do next.

## Failure vocabulary
- `FAILED full` → offer another run of the same title from the plan's alternatives.
- `FAILED schedule conflict` → name the clashing session and ask what to prefer.
- `SKIPPED operation closed (409)` → reservations are not open yet; try again later (do not retry now).
- `plan rejected: spec/schedule changed` → run `riv_plan` again.
