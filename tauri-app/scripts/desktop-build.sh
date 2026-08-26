#!/usr/bin/env bash
# Build the desktop bundle with frontendDist pointed at the RELEASE frontend.
#
# Mirrors android-build.sh, and exists for the same reason. `tauri.conf.json`
# pins `frontendDist` to the DEBUG dir because `cargo tauri dev` needs it there
# — but `beforeBuildCommand` (`npm run build`) populates the RELEASE dir and
# nothing ever refreshes debug. So a plain `cargo tauri build` embeds whatever a
# human last left in the debug directory, which after any UI session is a
# `dx serve --features mock` build. That is exactly how friction 1.13 shipped an
# APK full of mock data; android-build.sh fixed it for Android and left the
# desktop path carrying the same trap.
#
# The bundle is checked for the mock sentinel before and after, so the guard
# holds even if someone bypasses `npm run build`.
set -euo pipefail

cd "$(dirname "$0")/.."

FRONTEND_DIST="../frontend/target/dx/frontend/release/web/public"
BUNDLE_DIR="frontend/target/dx/frontend/release/web/public"

echo ">> building desktop bundle with frontendDist = ${FRONTEND_DIST}"

cargo tauri build \
  --config "{\"build\":{\"frontendDist\":\"${FRONTEND_DIST}\"}}" \
  "$@"

./scripts/assert-no-mock.sh "$BUNDLE_DIR"
