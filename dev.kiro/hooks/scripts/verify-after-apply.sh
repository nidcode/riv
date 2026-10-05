#!/usr/bin/env bash
# PostToolUse: if the shell command that just ran was `riv apply`, print the verification.
payload="$(cat)"
if printf '%s' "$payload" | grep -Eq 'riv[[:space:]]+apply'; then
  riv verify --json
fi
exit 0
