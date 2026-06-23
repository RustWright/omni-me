#!/usr/bin/env bash
#
# Build (and optionally run) the desktop app, embedding a chosen frontend tier.
#
#   ./scripts/desktop-build.sh [debug|release] [--run]   (default: release)
#
# WHY THIS EXISTS
# --------------
# The real trap is `build` vs `dev`, NOT debug vs release frontend:
#
#   * `cargo tauri dev`, when `frontendDist` is a path and no `devUrl` is set,
#     spins up its OWN localhost HTTP server to serve the UI and points the
#     WebView at http://127.0.0.1:PORT. Run the resulting dev binary STANDALONE
#     (no `cargo tauri dev` alive) and that server is gone -> the WebView's
#     main-frame navigation fails with a blank window showing only
#     "Could not connect to 127.0.0.1: Connection refused".
#
#   * `cargo tauri build` (what this script and android-build.sh use) EMBEDS the
#     `frontendDist` dir into the binary at compile time and serves it via the
#     `tauri://` asset protocol -- no server. So the binary runs standalone.
#     This is why an Android debug APK worked with no dev server: it was a
#     *build* (embed), and an embedded debug frontend runs standalone fine. The
#     debug frontend's Dioxus hot-reload WebSocket just fails gracefully.
#
# So both tiers run standalone once embedded. We still override `frontendDist`
# via --config because `tauri.conf.json` pins it to the DEBUG dir for
# `cargo tauri dev`; a build should embed a freshly-built dir (else it bakes in a
# stale frontend) -- the same override android-build.sh does for the APK.
#
# Default tier is `release`: smaller, production-faithful, and drops the inert
# hot-reload/dev scaffolding. `debug` also works (embedded) and builds faster.
#
# Output: a debug-profile binary (fast to build, no LTO) at
#   <workspace>/target/debug/omni-me-app
# Pass --run to launch it immediately (it opens the real local-first DB at
# ~/.local/share/com.omni-me.app/local.db).
set -euo pipefail

TIER="release"; RUN=0
for a in "$@"; do
  case "$a" in
    debug|release) TIER="$a" ;;
    --run)         RUN=1 ;;
    *) echo "usage: $0 [debug|release] [--run]" >&2; exit 2 ;;
  esac
done

cd "$(dirname "$0")/../src-tauri"

FRONTEND_DIST="../frontend/target/dx/frontend/${TIER}/web/public"
# Match the before-build step to the tier so the embedded dir is freshly built.
if [ "$TIER" = "release" ]; then
  BEFORE="npm run build"     # build:frontend (release) + editor + android copies
else
  BEFORE="npm run dev"       # build:frontend:dev (debug) -- needs a dev server to run
fi

echo ">> building desktop app (debug profile) with frontendDist = ${FRONTEND_DIST}"
[ "$TIER" = "debug" ] && echo ">> NOTE: debug frontend (embedded) runs standalone too; it just carries inert hot-reload scaffolding. 'release' is smaller/production-faithful."

cargo tauri build --debug --no-bundle \
  --config "{\"build\":{\"frontendDist\":\"${FRONTEND_DIST}\",\"beforeBuildCommand\":\"${BEFORE}\"}}"

BIN="$(cd ../.. && pwd)/target/debug/omni-me-app"
echo ">> built: ${BIN}"

if [ "$RUN" = "1" ]; then
  echo ">> launching ${BIN}"
  exec "$BIN"
fi
