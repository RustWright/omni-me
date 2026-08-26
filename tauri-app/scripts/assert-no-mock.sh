#!/usr/bin/env bash
# Refuse to package a frontend bundle built with `--features mock`.
#
# Guards the failure mode that actually happened (friction 1.13): an APK that
# shipped carrying mock data. The cause was NOT a compile flag — `dx serve
# --features mock` builds in debug, so a `cfg(not(debug_assertions))`
# compile_error never fires — it was a release bundle picking up an
# already-compiled debug directory. So the check belongs here, at the moment a
# directory is about to be shipped, where it catches the leak no matter which
# profile produced it.
#
# Pairs with OMNI_MOCK_BUILD_SENTINEL in frontend/src/bridge.rs. Verified in
# both directions: the string is present in a mock build and absent from a
# non-mock release build.
set -euo pipefail

DIR="${1:?usage: assert-no-mock.sh <bundle-dir>}"
SENTINEL="OMNI_MOCK_BUILD__DO_NOT_SHIP"

if [ ! -d "$DIR" ]; then
  echo "assert-no-mock: $DIR does not exist" >&2
  exit 1
fi

# `grep -r` on binaries needs -a; the sentinel is plain ASCII in the .wasm.
if grep -ral "$SENTINEL" "$DIR" >/dev/null 2>&1; then
  echo "" >&2
  echo "REFUSING TO PACKAGE: $DIR contains a mock build." >&2
  echo "" >&2
  echo "  Offending files:" >&2
  grep -ral "$SENTINEL" "$DIR" | sed 's/^/    /' >&2
  echo "" >&2
  echo "  This is the friction-1.13 failure: a bundle built with --features mock" >&2
  echo "  reaching a release package. Rebuild without the feature:" >&2
  echo "    npm run build:frontend        # release, no mock" >&2
  echo "" >&2
  exit 1
fi

echo "assert-no-mock: $DIR is clean"
