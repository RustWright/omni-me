#!/usr/bin/env bash
# Install the Linux AppImage as a proper desktop application.
#
# Solves two problems that look separate but are one:
#
#   1. "I can't pin omni-me to my favourites." Nothing ever installed a
#      `.desktop` entry. An AppImage *contains* one, but that copy lives inside
#      the image and no launcher ever sees it — so the app only exists as a file
#      you navigate to and execute by hand.
#
#   2. "The executable still says 1.0.3 after updating to 1.0.4." The Tauri
#      updater replaces the running AppImage **in place**, keeping its original
#      path and filename. Since `cargo tauri build` stamps the version into that
#      filename, an updated image keeps the name it was first downloaded under
#      and reads as stale forever.
#
# Installing to a STABLE, version-less path fixes both: the launcher gets a
# fixed target to pin, and the in-place replacement becomes invisible because
# the filename never claimed a version in the first place.
#
# RUNS STANDALONE. The usual case is a fresh device holding nothing but the
# downloaded AppImage and this script, both fetched from the box's /updates
# store — so nothing here may depend on a repo checkout. The icon is pulled out
# of the AppImage itself; a checkout, when there is one, is only a fallback.
#
# Usage:
#   ./install-desktop.sh path/to/omni-me_1.0.5_amd64.AppImage
#   ./install-desktop.sh                 # auto-detect from a local release build
#
# Override locations with INSTALL_BIN / INSTALL_DESKTOP / INSTALL_ICON if your
# distro puts them elsewhere; the defaults are the XDG user-level paths, which
# need no root.
set -euo pipefail

INSTALL_BIN="${INSTALL_BIN:-$HOME/.local/bin/omni-me.AppImage}"
INSTALL_DESKTOP="${INSTALL_DESKTOP:-$HOME/.local/share/applications/omni-me.desktop}"
ICON_ROOT="${ICON_ROOT:-$HOME/.local/share/icons/hicolor}"

# A checkout is optional. Resolve one only if this script is actually sitting in
# a repo, and never `cd` into it — the standalone case has no repo to cd to, and
# a stray `cd` would also silently reinterpret a relative AppImage argument.
script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root=""
if [ -f "$script_dir/../src-tauri/tauri.conf.json" ]; then
    repo_root="$(cd "$script_dir/../.." && pwd)"
fi

APPIMAGE="${1:-}"
if [[ -z "$APPIMAGE" && -n "$repo_root" ]]; then
    # Bundles land in the WORKSPACE-ROOT target dir, not `src-tauri/target`:
    # `tauri-app/src-tauri` is a member of the root workspace, so cargo puts its
    # artifacts under `<repo>/target`. This is the same path app-release.yml
    # collects the AppImage from.
    APPIMAGE="$(find "$repo_root/target/release/bundle/appimage" -name '*.AppImage' -print -quit 2>/dev/null || true)"
fi
if [[ -z "$APPIMAGE" || ! -f "$APPIMAGE" ]]; then
    echo "error: no AppImage found. Pass one explicitly:" >&2
    echo "  $0 path/to/omni-me_<version>_amd64.AppImage" >&2
    exit 1
fi

mkdir -p "$(dirname "$INSTALL_BIN")" "$(dirname "$INSTALL_DESKTOP")"

# `install` rather than `cp`: it replaces the target atomically, so doing this
# while an older copy is running cannot leave a half-written binary.
install -m 755 "$APPIMAGE" "$INSTALL_BIN"

# --- icon ------------------------------------------------------------------
#
# Preferred source is the AppImage itself, extracted from the copy just
# installed — which is already mode 755, so a file fetched with `curl -O` (mode
# 644, not executable) needs no chmod and is never modified. The runtime handles
# `--appimage-extract` internally, so this needs no FUSE.
#
# The shipped icon is 256x256 and belongs to exactly the build being installed;
# the repo's 128x128.png is only a fallback for a checkout whose AppImage will
# not extract. Neither is fatal: a missing icon leaves a usable entry with a
# generic glyph, so this warns rather than aborting an otherwise good install.
icon_src=""
icon_size=""
tmp_extract="$(mktemp -d)"
trap 'rm -rf "$tmp_extract"' EXIT

if (cd "$tmp_extract" && "$INSTALL_BIN" --appimage-extract omni-me.png >/dev/null 2>&1) \
    && [ -f "$tmp_extract/squashfs-root/omni-me.png" ]; then
    icon_src="$tmp_extract/squashfs-root/omni-me.png"
    icon_size="256x256"
elif [ -n "$repo_root" ] && [ -f "$repo_root/tauri-app/src-tauri/icons/128x128.png" ]; then
    icon_src="$repo_root/tauri-app/src-tauri/icons/128x128.png"
    icon_size="128x128"
fi

icon_installed=""
if [ -n "$icon_src" ]; then
    icon_dest="${INSTALL_ICON:-$ICON_ROOT/$icon_size/apps/omni-me.png}"
    mkdir -p "$(dirname "$icon_dest")"
    install -m 644 "$icon_src" "$icon_dest"
    icon_installed="$icon_dest"
else
    echo "warning: could not read an icon from $INSTALL_BIN and no checkout to" >&2
    echo "         fall back to — installing the launcher entry without one." >&2
fi

# --- desktop entry ---------------------------------------------------------
#
# `StartupWMClass` must match the WM class the window actually reports, or the
# running window docks as a second, generic icon instead of lighting up the
# pinned one — the classic "pinned it, but launching spawns a duplicate" symptom.
#
# `omni-me-app` is not a guess: it is what Tauri's own bundler writes into the
# entry inside the AppImage (`usr/share/applications/omni-me.desktop`), derived
# from the binary name. To re-check on a given desktop, launch the app and run
# `xprop WM_CLASS` on its window.
{
    echo "[Desktop Entry]"
    echo "Type=Application"
    echo "Name=omni-me"
    echo "Comment=Personal life operating system — journal, routines, finances"
    echo "Exec=$INSTALL_BIN"
    [ -n "$icon_installed" ] && echo "Icon=omni-me"
    echo "Terminal=false"
    # ONE main category on purpose. `desktop-file-validate` warns that listing
    # two ("Utility;Office;") can make the app appear twice in the menu — the
    # same duplicate-entry symptom StartupWMClass exists to prevent, one layer up.
    echo "Categories=Office;"
    echo "StartupWMClass=omni-me-app"
} > "$INSTALL_DESKTOP"
chmod 644 "$INSTALL_DESKTOP"

# Best-effort: some desktops pick the entry up immediately, others need the
# cache nudged. Never fail the install over a missing optional tool.
command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$(dirname "$INSTALL_DESKTOP")" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 &&
    gtk-update-icon-cache -f -t "$ICON_ROOT" 2>/dev/null || true

echo "installed:"
echo "  binary   $INSTALL_BIN"
echo "  desktop  $INSTALL_DESKTOP"
echo "  icon     ${icon_installed:-<none>}"
echo
echo "omni-me should now appear in your app launcher and be pinnable."
echo "Launch the INSTALLED copy — in-app updates replace $INSTALL_BIN in place,"
echo "so the downloaded file you passed here can be deleted."
