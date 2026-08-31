#!/usr/bin/env bash
#
# Build the Android APK, embedding a chosen frontend tier.
#
#   ./scripts/android-build.sh [debug|release]   (default: release)
#
# WHY THIS EXISTS
# --------------
# `tauri-build` embeds the directory named by `frontendDist` INTO the native
# binary (libomni_me_app.so) at compile time, and the WebView serves the app
# from those embedded assets at runtime. `tauri.conf.json` keeps
# `frontendDist` pointed at the DEBUG dir because `cargo tauri dev` needs it
# there. But a release APK must embed the freshly-built RELEASE dir — otherwise
# it bakes in a stale debug frontend (the `beforeBuildCommand` builds release,
# but nothing ever refreshes the debug dir, so it ships frozen). We override
# `frontendDist` for the build only, via `--config`, so the dev flow is
# untouched. The two tier dirs differ by exactly one path segment, so the tier
# is the only parameter.
#
# MEMORY-SAFE PROFILE (OMNI_BUILD_MEM_SAFE)
# -----------------------------------------
# The full release profile (LTO + codegen-units=1 + opt-level=3, from the
# workspace Cargo.toml) OOMs a low-RAM laptop, so by default this script pins a
# throttled profile (1 job, no LTO, opt-level=1). That keeps local builds alive
# but ships a SLOWER, BLOATED APK. CI runners have plenty of RAM, so the release
# pipeline sets `OMNI_BUILD_MEM_SAFE=0` to build un-throttled — the canonical
# shipped APK is the fully-optimized CI build. Default is ON (1) so the
# constrained-laptop fallback stays the local default.
set -euo pipefail

TIER="${1:-release}"
case "$TIER" in
  debug|release) ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

# Default ON locally; the release workflow exports OMNI_BUILD_MEM_SAFE=0.
MEM_SAFE="${OMNI_BUILD_MEM_SAFE:-1}"

cd "$(dirname "$0")/../src-tauri"

FRONTEND_DIST="../frontend/target/dx/frontend/${TIER}/web/public"
echo ">> building Android APK with frontendDist = ${FRONTEND_DIST}"

if [ "$MEM_SAFE" = "1" ]; then
  echo ">> OMNI_BUILD_MEM_SAFE=1 — throttled profile (no LTO, opt-level=1); APK will be larger/slower"
  export CARGO_BUILD_JOBS=1
  export CARGO_PROFILE_RELEASE_LTO=false
  export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
  export CARGO_PROFILE_RELEASE_OPT_LEVEL=1
else
  echo ">> OMNI_BUILD_MEM_SAFE=0 — full release profile (LTO, opt-level=3); needs RAM"
fi

# Regenerate the launcher icons before building.
#
# `cargo tauri android init` writes Tauri's DEFAULT icons into
# `gen/android/app/src/main/res/mipmap-*/`, and that whole tree is gitignored —
# so the enso icons only ever existed on whichever machine last ran
# `cargo tauri icon`. CI inits the Android project from scratch on every run and
# therefore shipped the **generic Tauri logo** in every release APK (found
# on-device 2026-08-31, v1.0.0/v1.0.1).
#
# `android-overrides/` can't solve this the way it does for MainActivity/themes:
# it copies single files, and this is ~63 generated PNGs across every density.
# Regenerating from the tracked SVG source is better than committing binaries —
# `branding/logo-manifest.json` + its SVGs are in git, so the icons are derived,
# not stored, and can never drift from the source of truth.
#
# Runs here rather than in the CI workflow because CI invokes THIS script after
# `android init`, so one call fixes the local and CI paths together. Idempotent
# and quick; it also refreshes the tracked desktop icons under `icons/`, which
# is harmless (identical bytes unless the branding SVGs changed).
echo ">> regenerating launcher icons from branding/logo-manifest.json"

# `cargo tauri icon` also rewrites the TRACKED desktop icons under `icons/`, and
# `icon.icns` does not pack deterministically — identical input, identical size,
# different bytes every run. Left alone this dirties a tracked file on every
# build, which is the same trap `gen/schemas/` set: churn that invites an
# accidental commit. Only `gen/android/` (gitignored) actually needs refreshing
# here, so snapshot the tracked dir and put it back afterwards.
ICONS_BACKUP="$(mktemp -d)"
trap 'rm -rf "$ICONS_BACKUP"' EXIT
cp -a icons/. "$ICONS_BACKUP"/

cargo tauri icon ../branding/logo-manifest.json

rm -rf icons && cp -a "$ICONS_BACKUP" icons

# Fail loudly rather than ship the default robot again: after a real icon
# generation the adaptive-icon foreground must exist. A silent miss here is
# exactly how v1.0.0 and v1.0.1 shipped the stock Tauri logo.
ICON_CHECK="gen/android/app/src/main/res/mipmap-hdpi/ic_launcher_foreground.png"
if [ -d gen/android ] && [ ! -f "$ICON_CHECK" ]; then
  echo "icon generation did not produce $ICON_CHECK" >&2
  exit 1
fi

cargo tauri android build --apk --target aarch64 \
  --config "{\"build\":{\"frontendDist\":\"${FRONTEND_DIST}\"}}"
