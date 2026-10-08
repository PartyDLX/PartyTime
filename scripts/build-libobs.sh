#!/usr/bin/env bash
# Build libobs for the console to link against.
#
# OBS's top-level CMake file defines the helpers libobs expects. Configure that tree
# with the frontend and scripting disabled, then build only libobs and its OpenGL
# graphics backend - not the UI or the rest of the plugin catalog. Publishing still
# uses OBS's bundled obs-webrtc output (ADR-0001); this script does not build a WHIP
# module of ours. See ADR-0004 for the binding layer.
#
#   ./scripts/build-libobs.sh                 # configure + build into .obs-build
#   ./scripts/build-libobs.sh --tag 32.1.2    # a different OBS release
#   ./scripts/build-libobs.sh --clean
#   ./scripts/build-libobs.sh --help
#
# Verified against OBS 32.0.4's libobs/CMakeLists.txt. The top-level configuration also
# needs extra-cmake-modules (ECM), even with the frontend disabled.
#   SIMDe + FFmpeg 8.0 dev headers + ZLIB + jansson + uthash-devel.
# Fedora 44 package names are installed by `pt box-setup` inside the Distrobox.
# cmake and ninja are part of that same setup; they can also be installed from PyPI:
#   python3 -m pip install --user cmake ninja
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# The version is not a preference. The `libobs` bindings we link against are
# `5.0.1+32.0.4`, and that suffix is the OBS release those bindings were generated
# against - see docs/adr/0004-raw-libobs-bindings.md. Building a different tag
# compiles fine and then disagrees with us at link time.
tag="32.0.4"
src="${PARTYTIME_OBS_SRC:-$repo_root/.obs-src}"
build="${PARTYTIME_OBS_BUILD:-$repo_root/.obs-build}"
prefix="${PARTYTIME_OBS_PREFIX:-$HOME/.cache/partytime-obs}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag) tag="$2"; shift 2 ;;
    --clean) rm -rf "$src" "$build"; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

for tool in cmake ninja git; do
  command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done

if [[ ! -d "$src/.git" ]]; then
  echo "cloning obs-studio $tag"
  git clone --depth 1 --branch "$tag" https://github.com/obsproject/obs-studio.git "$src"
fi

# This script used to configure libobs/ standalone. If that older cache is present,
# CMake refuses to reuse it for the correct OBS source root.
if [[ -f "$build/CMakeCache.txt" ]] &&
   { ! grep -Fxq "CMAKE_HOME_DIRECTORY:INTERNAL=$src" "$build/CMakeCache.txt" ||
     grep -Eq "^Uthash_INCLUDE_DIR:[^=]+=$src/libobs/util$" "$build/CMakeCache.txt"; }; then
  echo "discarding CMake cache with stale source or Uthash paths"
  rm -rf "$build"
fi

# Configure OBS at the source root so its helper functions and bundled dependencies
# resolve correctly. Disable the UI, scripting and plugin catalog, then build libobs,
# its OpenGL graphics backend, and the small frontend API target present in the install
# manifest (but not the UI executable).
echo "configuring OBS $tag"
cmake -S "$src" -B "$build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DCMAKE_INSTALL_LIBDIR=lib \
  -DENABLE_UI=OFF \
  -DENABLE_FRONTEND=OFF \
  -DENABLE_SCRIPTING=OFF \
  -DENABLE_PLUGINS=OFF \
  -DENABLE_HEVC=OFF \
  -U Uthash_INCLUDE_DIR

echo "building libobs, OpenGL backend, and frontend API library"
cmake --build "$build" --target libobs libobs-opengl obs-frontend-api --parallel

cmake --install "$build" >/dev/null

echo
echo "libobs installed under $prefix"
echo "link with: -L$prefix/lib -lobs"
