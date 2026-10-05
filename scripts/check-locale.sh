#!/usr/bin/env bash
# Print the Content-Language header ListSessions answers with for locale=ja-JP on a registration-free event.
# Read-only. Usage: scripts/check-locale.sh [eventId]
set -euo pipefail
BASE="${RIV_API_BASE:-https://api.awsevents.com}"
EVENT="${1:-}"
if [ -z "$EVENT" ]; then
  EVENT=$(curl -fsS "$BASE/v1/events" | python3 -c 'import json,sys; ev=[e["eventId"] for e in json.load(sys.stdin)["items"] if not e.get("authenticationRequired")]; print(ev[0] if ev else "")')
fi
[ -n "$EVENT" ] || { echo "no registration-free event found" >&2; exit 1; }
echo "event: $EVENT"
curl -sS -D - -o /dev/null "$BASE/v1/events/$EVENT/sessions?locale=ja-JP&includeAbstracts=false" | grep -i -E '^(HTTP|content-language)' || true
