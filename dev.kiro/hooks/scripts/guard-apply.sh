#!/usr/bin/env bash
# PreToolUse guard: the hook payload (JSON) arrives on stdin. Block `riv apply` without --plan / --resume.
payload="$(cat)"
if printf '%s' "$payload" | grep -Eq 'riv[[:space:]]+apply'; then
  if ! printf '%s' "$payload" | grep -Eq -- '--(plan|resume)'; then
    echo "blocked: 'riv apply' needs --plan <planId> from an approved 'riv plan'" >&2
    exit 2
  fi
fi
exit 0
