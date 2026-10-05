# Hook examples

These are **examples** in Kiro's hook file format (`version: v1`, checked against https://kiro.dev/docs/hooks/ on 2026-10-06). Copy the ones you want into your project's `.kiro/hooks/`.

TODO(verify in Kiro IDE): the exact fields of the PreToolUse/PostToolUse stdin payload and whether a Power may ship hooks itself. The scripts match on the whole payload text so they do not depend on field names.

- `riv-plan-on-save`: File triggers fire only for **agent** changes. After a manual save, run `riv plan` or use the `/riv-plan` steering.
- `riv-guard-apply`: blocks `riv apply` without `--plan`/`--resume` (exit code 2 blocks the tool call).
- `riv-verify-after-apply`: reads the schedule back after an apply.
