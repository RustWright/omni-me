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
# Usage:
#   ./install-desktop.sh path/to/omni-me_1.0.4_amd64.AppImage
#   ./install-desktop.sh                 # auto-detect from the release bundle
#
# Override locations with INSTALL_BIN / INSTALL_DESKTOP / INSTALL_ICON if your
# distro puts them elsewhere; the defaults are the XDG user-level paths, which
# need no root.
set -euo pipefail

cd "$(dirname "$0")/.."

INSTALL_BIN="${INSTALL_BIN:-$HOME/.local/bin/omni-me.AppImage}"
INSTALL_DESKTOP="${INSTALL_DESKTOP:-$HOME/.local/share/applications/omni-me.desktop}"
INSTALL_ICON="${INSTALL_ICON:-$HOME/.local/share/icons/hicolor/128x128/apps/omni-me.png}"

APPIMAGE="${1:-}"
if [[ -z "$APPIMAGE" ]]; then
  # Same glob the release workflow uses to collect the artifact.
  APPIMAGE="$(find src-tauri/target -name '*.AppImage' -print -quit 2>/dev/null || true)"
fi
if [[ -z "$APPIMAGE" || ! -f "$APPIMAGE" ]]; then
  echo "error: no AppImage found. Pass one explicitly:" >&2
  echo "  $0 path/to/omni-me_<version>_amd64.AppImage" >&2
  exit 1
fi

mkdir -p "$(dirname "$INSTALL_BIN")" "$(dirname "$INSTALL_DESKTOP")" "$(dirname "$INSTALL_ICON")"

# `install` rather than `cp`: it replaces the target atomically, so doing this
# while an older copy is running cannot leave a half-written binary.
install -m 755 "$APPIMAGE" "$INSTALL_BIN"
install -m 644 src-tauri/icons/128x128.png "$INSTALL_ICON"

# `StartupWMClass` must match the WM class the window actually reports, or the
# running window docks as a second, generic icon instead of lighting up the
# pinned one — the classic "pinned it, but launching spawns a duplicate" symptom.
cat > "$INSTALL_DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=omni-me
Comment=Personal life operating system — journal, routines, finances
Exec=$INSTALL_BIN
Icon=omni-me
Terminal=false
Categories=Utility;Office;
StartupWMClass=omni-me
EOF
chmod 644 "$INSTALL_DESKTOP"

# Best-effort: some desktops pick the entry up immediately, others need the
# cache nudged. Never fail the install over a missing optional tool.
command -v update-desktop-database >/dev/null 2>&1 &&
  update-desktop-database "$(dirname "$INSTALL_DESKTOP")" 2>/dev/null || true

echo "installed:"
echo "  binary   $INSTALL_BIN"
echo "  desktop  $INSTALL_DESKTOP"
echo "  icon     $INSTALL_ICON"
echo
echo "omni-me should now appear in your app launcher and be pinnable."
echo "Future in-app updates replace $INSTALL_BIN in place — the name stays correct."
