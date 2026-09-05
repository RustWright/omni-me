#!/usr/bin/env bash
#
# Pull filed problem reports off the box and write them to a file.
#
# This is the caller side of `GET /feedback`, and it is what makes the
# destination pluggable: the app stores reports in its event log and knows
# nothing about where they end up. Point this at a notes directory, a private
# overlay repo, or pipe the JSON form into an issue tracker — the app does not
# change either way.
#
#   OMNI_SERVER_URL=http://box:3000 \
#   OMNI_SERVER_TOKEN=... \
#     scripts/pull-feedback.sh feedback.md [since]
#
# `since` is an RFC3339 timestamp; only newer reports come back. A caller that
# pulls incrementally should pass the newest timestamp it has already stored.
#
# Nothing here is machine-specific by design: no box address, no repo path, no
# token. All three arrive from the environment or the command line, so this
# script is safe in the public repo and identical on every device.

set -euo pipefail

OUT="${1:-}"
SINCE="${2:-}"

if [[ -z "$OUT" ]]; then
  echo "usage: $0 <output-file> [since-rfc3339]" >&2
  exit 2
fi

: "${OMNI_SERVER_URL:?set OMNI_SERVER_URL to the box's base URL}"

URL="${OMNI_SERVER_URL%/}/feedback"
if [[ -n "$SINCE" ]]; then
  URL="${URL}?since=${SINCE}"
fi

# The token is optional only because the box itself fails open when no
# `[server].auth_token` is configured. Against a configured box a missing token
# is a 401, which the status check below reports rather than writing the error
# body to the output file as though it were reports.
AUTH=()
if [[ -n "${OMNI_SERVER_TOKEN:-}" ]]; then
  AUTH=(-H "Authorization: Bearer ${OMNI_SERVER_TOKEN}")
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

STATUS="$(curl -sS --max-time 30 -o "$TMP" -w '%{http_code}' "${AUTH[@]}" "$URL")"

if [[ "$STATUS" != "200" ]]; then
  echo "feedback pull failed: HTTP $STATUS" >&2
  head -c 400 "$TMP" >&2
  echo >&2
  exit 1
fi

# Write only after a confirmed 200, so a failed pull leaves the previous file
# intact rather than replacing reports with an error page.
mv "$TMP" "$OUT"
trap - EXIT

echo "wrote $OUT ($(wc -l < "$OUT") lines)"
