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
  -DENABLE_PLUGINS=ON \
  -DENABLE_AJA=OFF \
  -DENABLE_DECKLINK=OFF \
  -DENABLE_VLC=OFF \
  -DENABLE_VST=OFF \
  -DENABLE_BROWSER=OFF \
  -DENABLE_WEBSOCKET=OFF \
  -DENABLE_ALSA=OFF \
  -DENABLE_JACK=OFF \
  -DENABLE_OSS=OFF \
  -DENABLE_PIPEWIRE=OFF \
  -DENABLE_SNDIO=OFF \
  -DENABLE_V4L2=OFF \
  -DENABLE_UDEV=OFF \
  -DENABLE_VIRTUALCAM=OFF \
  -DENABLE_QSV11=OFF \
  -DENABLE_NVENC=OFF \
  -DENABLE_LIBFDK=OFF \
  -DENABLE_SYPHON=OFF \
  -DENABLE_SERVICE_UPDATES=OFF \
  -DENABLE_NEW_MPEGTS_OUTPUT=OFF \
  -DENABLE_HEVC=ON

# x264 is a hard dependency of OBS's plugin catalog and has no off switch. Build the
# commit OBS pins for x264 (build-aux/modules/20-x264.json) into the same prefix, so the
# plugin links against a known version without requiring RPM Fusion.
x264_src="${PARTYTIME_X264_SRC:-$HOME/.cache/partytime-x264-src}"
x264_commit="$(sed -n 's/.*"commit": "\([0-9a-f]*\)".*/\1/p' "$src/build-aux/modules/20-x264.json")"
if [[ ! -d "$x264_src/.git" ]]; then
  git clone https://code.videolan.org/videolan/x264.git "$x264_src"
fi
git -C "$x264_src" checkout --quiet "$x264_commit"
(
  cd "$x264_src"
  ./configure --prefix="$prefix" --disable-cli --enable-shared >/dev/null
  make -j"$(nproc)" >/dev/null && make install >/dev/null
)

echo "building OBS: libobs, OpenGL backend, frontend API, obs-webrtc and the plugin set"
cmake --build "$build" --parallel

cmake --install "$build" >/dev/null

echo
echo "libobs installed under $prefix"
echo "link with: -L$prefix/lib -lobs"
