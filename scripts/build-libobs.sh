#!/usr/bin/env bash
# Build libobs for the console to link against.
#
# Why this exists: obs-studio's own build wants Qt, KDE's extra-cmake-modules and every
# plugin. We need none of that - just libobs, configured on its own. Publishing goes
# through OBS's bundled obs-webrtc output (ADR-0001), so nothing here is a WHIP module
# of ours; see docs/adr/0004 for the binding layer.
#
#   ./scripts/build-libobs.sh                 # configure + build into .obs-build
#   ./scripts/build-libobs.sh --tag 32.1.2    # a different OBS release
#   ./scripts/build-libobs.sh --clean
#   ./scripts/build-libobs.sh --help
#
# Verified against OBS 32.x's libobs/CMakeLists.txt, which requires:
#   SIMDe  (simde-devel)      FFmpeg 8.0 (libavcodec-free-devel on Fedora 44)
#   ZLIB   (zlib-ng-compat-devel, already present here)
#   Uthash                     jansson    (jansson-devel)
#
# So on Fedora 44 this machine needs one root command:
#   sudo dnf install libavcodec-free-devel jansson-devel simde-devel
#
# cmake and ninja are not packaged the same way and install from PyPI without root:
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

# libobs on its own: no frontend, no scripting, no plugins, no ECM.
#
# OBS vendors uthash at libobs/util/uthash.h, but FindUthash only searches /usr/include
# and /usr/local/include. In the full tree something else puts that directory on the
# include path; configuring libobs on its own loses that, so point at the vendored copy
# instead of installing a second uthash.
echo "configuring libobs"
cmake -S "$src/libobs" -B "$build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DCMAKE_PREFIX_PATH="$prefix" \
  -DUthash_INCLUDE_DIR="$src/libobs/util"

echo "building"
cmake --build "$build" --parallel

cmake --install "$build" >/dev/null

echo
echo "libobs installed under $prefix"
echo "link with: -L$prefix/lib -lobs"
