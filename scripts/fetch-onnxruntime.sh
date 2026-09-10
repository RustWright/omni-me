#!/usr/bin/env bash
# Fetch the ONNX Runtime that `fastembed`/`ort` link against, into a gitignored
# `.ort/` at the repo root, and print the environment that points a build at it.
#
#   source scripts/fetch-onnxruntime.sh      # fetch + export into this shell
#   scripts/fetch-onnxruntime.sh --print     # just print the exports
#
# ⚠️ **We deliberately do NOT use ort's own prebuilt binaries.** `ort`'s
# `download-binaries` feature pulls a build from pyke's CDN that references
# `__isoc23_strtol` and libstdc++ 13 symbols — i.e. it requires **glibc >= 2.38**.
# The dev machine is Ubuntu 22.04 (glibc 2.35) and `server/Dockerfile` targets
# Debian bookworm (glibc 2.36), so that build links on neither. Microsoft's own
# release requires at most **glibc 2.27 / GLIBCXX 3.4.21** and works on both.
#
# The failure this avoids is a link error naming C++ symbols, which reads like a
# missing compiler package and is not one.
set -euo pipefail

# Must stay in step with the ONNX Runtime version `ort-sys` expects (its
# `build/download/dist.tsv` pins `ms@1.28.0`); a patch-level difference is ABI
# compatible, a minor one is not.
ORT_VERSION="1.28.1"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ORT_HOME="$REPO_ROOT/.ort/onnxruntime-linux-x64-$ORT_VERSION"

if [ "${1:-}" != "--print" ] && [ ! -f "$ORT_HOME/lib/libonnxruntime.so" ]; then
	echo "fetching ONNX Runtime $ORT_VERSION into .ort/ ..." >&2
	mkdir -p "$REPO_ROOT/.ort"
	tmp="$(mktemp -d)"
	trap 'rm -rf "$tmp"' EXIT
	curl -fsSL -o "$tmp/ort.tgz" \
		"https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VERSION/onnxruntime-linux-x64-$ORT_VERSION.tgz"
	tar xzf "$tmp/ort.tgz" -C "$REPO_ROOT/.ort"
	echo "fetched to $ORT_HOME" >&2
fi

# `ORT_LIB_LOCATION` must name the directory holding the .so **itself**, not the
# distribution root — pointing at the root gets past ort-sys and then fails the
# final link with `unable to find library -lonnxruntime`.
export ORT_LIB_LOCATION="$ORT_HOME/lib"
# Microsoft ship no static archive, so the default static link cannot succeed.
export ORT_PREFER_DYNAMIC_LINK=1
# Needed at *run* time as well as link time; a built binary cannot find the .so
# without it.
export LD_LIBRARY_PATH="$ORT_HOME/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# Where fastembed caches downloaded models, kept beside the runtime so both are
# reclaimed by deleting one directory.
export FASTEMBED_CACHE_DIR="${FASTEMBED_CACHE_DIR:-$REPO_ROOT/.ort/models}"

if [ "${1:-}" = "--print" ] || [ "${BASH_SOURCE[0]}" = "${0}" ]; then
	echo "export ORT_LIB_LOCATION=\"$ORT_LIB_LOCATION\""
	echo "export ORT_PREFER_DYNAMIC_LINK=1"
	echo "export LD_LIBRARY_PATH=\"$ORT_HOME/lib:\$LD_LIBRARY_PATH\""
	echo "export FASTEMBED_CACHE_DIR=\"$FASTEMBED_CACHE_DIR\""
fi
