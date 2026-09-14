#!/usr/bin/env bash
# The boot truth table of `docs/src/isolation.md`, against the REAL server binary.
#
#   scripts/smoke-isolation.sh [addr]        # default 127.0.0.1:3399
#
# Why a shell smoke on top of the unit tests in `core/src/runtime.rs`: those
# cover the decision, this covers the wiring. A check placed after
# `db::connect` instead of before it would pass every unit test while the
# protection was hollow — case 5 catches that by asserting the refused
# directory is still empty.
#
# Runs entirely in a temp dir. Touches nothing deployed.
set -uo pipefail

ADDR="${1:-127.0.0.1:3399}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$REPO/target/debug/omni-me-server"
ROOT="$(mktemp -d)"

[ -x "$BIN" ] || { echo "build it first: cargo build -p omni-me-server" >&2; exit 2; }

pass=0; fail=0
check() { # check <label> <expected> <actual>
    if [ "$2" = "$3" ]; then
        printf 'PASS  %-52s %s\n' "$1" "$3"; pass=$((pass + 1))
    else
        printf 'FAIL  %-52s want=%s got=%s\n' "$1" "$2" "$3"; fail=$((fail + 1))
    fi
}

stop_server() {
    [ -n "${SRV:-}" ] || return 0
    kill "$SRV" 2>/dev/null; wait "$SRV" 2>/dev/null; SRV=""
}
cleanup() { stop_server; rm -rf "$ROOT"; }
trap cleanup EXIT

# Boot in <dir> and echo what /health reports, or REFUSED if the process died.
boot() { # boot <dir> <label> [env assignments...]
    local dir="$1" label="$2"
    shift 2
    mkdir -p "$dir" "$ROOT/logs"

    # A stale server from the previous case would answer for this one and every
    # assertion would pass against the wrong process.
    if curl -fsS --max-time 2 "http://$ADDR/health" >/dev/null 2>&1; then
        echo "PORT-DIRTY"; return 0
    fi

    # `exec` so $SRV is the server. Without it the PID is the subshell's,
    # killing that orphans the server, and it keeps holding the port.
    ( cd "$dir" && exec env OMNI_LISTEN_ADDR="$ADDR" "$@" "$BIN" ) \
        >"$ROOT/logs/$label.log" 2>&1 &
    SRV=$!

    for _ in $(seq 1 40); do
        local body
        body="$(curl -fsS --max-time 2 "http://$ADDR/health" 2>/dev/null)"
        if [ -n "$body" ]; then
            python3 -c 'import json,sys; print(json.load(sys.stdin)["instance"])' <<<"$body"
            stop_server; return 0
        fi
        kill -0 "$SRV" 2>/dev/null || { echo REFUSED; SRV=""; return 0; }
        sleep 0.25
    done
    echo TIMEOUT; stop_server
}

echo "=== 1. declared dev in an unmarked root -> stamps and reports dev"
check "OMNI_INSTANCE=dev, no marker" dev "$(boot "$ROOT/dev" d1 OMNI_INSTANCE=dev)"
check "  marker written" dev "$(cat "$ROOT/dev/.omni-instance" 2>/dev/null)"

echo "=== 2. same root, restarted claiming production -> MUST refuse"
check "production over a dev marker" REFUSED "$(boot "$ROOT/dev" d2 OMNI_INSTANCE=production)"
check "  marker untouched by the refusal" dev "$(cat "$ROOT/dev/.omni-instance" 2>/dev/null)"
grep -q 'refusing to start' "$ROOT/logs/d2.log" \
    && check "  refusal explained in the log" yes yes \
    || check "  refusal explained in the log" yes no

echo "=== 3. restamp is the deliberate escape"
check "production + RESTAMP=1" production \
    "$(boot "$ROOT/dev" d3 OMNI_INSTANCE=production OMNI_INSTANCE_RESTAMP=1)"
check "  marker flipped" production "$(cat "$ROOT/dev/.omni-instance" 2>/dev/null)"

echo "=== 4. zero-config boot still works, and reports unknown"
check "nothing declared, fresh root" unknown "$(boot "$ROOT/bare" d4)"
[ -e "$ROOT/bare/.omni-instance" ] \
    && check "  nothing stamped" absent present \
    || check "  nothing stamped" absent absent

echo "=== 5. a typo fails loudly, and before the database is opened"
check "OMNI_INSTANCE=prod" REFUSED "$(boot "$ROOT/typo" d5 OMNI_INSTANCE=prod)"
# The ordering guarantee: a refused boot has not taken the storage lock.
[ -e "$ROOT/typo/surreal_data" ] \
    && check "  no database created" absent present \
    || check "  no database created" absent absent

echo "--------------------------------------------------"
echo "pass=$pass fail=$fail"
[ "$fail" -eq 0 ]
