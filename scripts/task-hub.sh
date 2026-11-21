#!/usr/bin/env bash
#
# The session hooks' side of the task hub (rulings 91 and 121, RULINGS.md).
#
#   scripts/task-hub.sh push <path/to/.omni/tasks.toml>
#       Send one index to the server. Unchanged files are skipped server-side.
#   scripts/task-hub.sh taken-over <project-slug>
#       Print the hub ids omni-me took over, one per line, for the session to mark `cut`.
#
# Needs OMNI_SERVER_URL (and OMNI_SERVER_TOKEN against a box with auth on), like
# pull-feedback.sh. Short timeouts: a session must never wait on an unreachable box, so a
# failure prints one line to stderr and exits 0.

set -euo pipefail

CMD="${1:-}"
ARG="${2:-}"
if [[ -z "$CMD" || -z "$ARG" ]]; then
  echo "usage: $0 push <tasks.toml> | taken-over <project>" >&2
  exit 2
fi
if [[ -z "${OMNI_SERVER_URL:-}" ]]; then
  echo "task-hub: OMNI_SERVER_URL unset; skipped" >&2
  exit 0
fi
BASE="${OMNI_SERVER_URL%/}"
AUTH=()
if [[ -n "${OMNI_SERVER_TOKEN:-}" ]]; then
  AUTH=(-H "Authorization: Bearer ${OMNI_SERVER_TOKEN}")
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

case "$CMD" in
  push)
    STATUS="$(curl -sS --max-time 5 -o "$TMP" -w '%{http_code}' "${AUTH[@]}" \
      -H 'Content-Type: text/plain' --data-binary "@${ARG}" "${BASE}/tasks/hub_index" \
      2>/dev/null || true)"
    ;;
  taken-over)
    STATUS="$(curl -sS --max-time 2 -o "$TMP" -w '%{http_code}' "${AUTH[@]}" \
      --get --data-urlencode "project=${ARG}" "${BASE}/tasks/taken_over" \
      2>/dev/null || true)"
    ;;
  *)
    echo "task-hub: unknown command $CMD" >&2
    exit 2
    ;;
esac

if [[ "$STATUS" != "200" ]]; then
  echo "task-hub: $CMD skipped (server answered ${STATUS}): $(head -c 200 "$TMP")" >&2
  exit 0
fi

python3 - "$CMD" "$TMP" <<'PY'
import json, sys
cmd, path = sys.argv[1], sys.argv[2]
data = json.load(open(path))
if cmd == "push":
    state = "recorded" if data["recorded"] else "unchanged"
    print("task-hub: %s %s, %d tasks, %d need you"
          % (data["project"], state, data["tasks"], data["needs_you"]))
    for p in data["problems"]:
        print("task-hub:   " + p)
else:
    for r in data:
        print(r["hub_id"] + "\t" + r["title"])
PY
