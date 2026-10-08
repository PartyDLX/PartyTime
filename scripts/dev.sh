#!/usr/bin/env bash
# Rebuild and restart the console whenever a source file changes.
#
# Rust has no hot reload: a running process holds the compiled code, so changing the
# application means rebuilding and starting again. This closes that loop — watch, build,
# swap — so the edit-to-window time is one incremental build instead of a manual rebuild
# plus a manual relaunch.
#
#   ./scripts/dev.sh                 # watch and restart
#   ./scripts/dev.sh --stub          # also start the local OAuth stub on :5199
#   ./scripts/dev.sh --origin URL    # point the console somewhere else
#
# Ctrl-C stops the watcher and the console.
#
# Deliberately dependency-free: no cargo-watch, watchexec or inotify-tools. This machine
# has none of them, and a dev loop should not need one installed.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

origin="${PARTYTIME_ORIGIN:-}"
stub=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --stub) stub=1; shift ;;
    --origin) origin="$2"; shift 2 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

# --- display -----------------------------------------------------------------------
# A GPUI app needs a real display. On a GNOME/Xwayland session the server's cookie lives
# in a per-session file that XAUTHORITY has to point at, or X11 refuses the connection
# with "Authorization required, but no authorization protocol specified".
if [[ -z "${DISPLAY:-}" ]]; then
  for socket in /tmp/.X11-unix/X*; do
    [[ -e "$socket" ]] || continue
    export DISPLAY=":${socket##*X}"
    break
  done
fi
if [[ -z "${XAUTHORITY:-}" ]]; then
  export XAUTHORITY="$(ls -1 /run/user/"$(id -u)"/.mutter-Xwaylandauth.* 2>/dev/null | head -1 || true)"
fi
if [[ -z "${DISPLAY:-}" ]]; then
  echo "no X display found; set DISPLAY (and XAUTHORITY) before running this" >&2
  exit 1
fi
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"

# --- console environment -----------------------------------------------------------
export PARTYTIME_CONFIG_DIR="${PARTYTIME_CONFIG_DIR:-$repo_root/.dev}"
mkdir -p "$PARTYTIME_CONFIG_DIR"
[[ -n "$origin" ]] && export PARTYTIME_ORIGIN="$origin"
export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"

console_pid=""
stub_pid=""

stop_children() {
  [[ -n "$console_pid" ]] && kill "$console_pid" 2>/dev/null || true
  [[ -n "$stub_pid" ]] && kill "$stub_pid" 2>/dev/null || true
}
trap 'stop_children; exit 0' INT TERM

build() {
  echo "── building ──"
  if ! cargo build -p partytime-console 2>&1 | sed 's/^/   /'; then
    return 1
  fi
}

start_console() {
  [[ -n "$console_pid" ]] && kill "$console_pid" 2>/dev/null || true
  ./target/debug/partytime &
  console_pid=$!
  echo "── console running (pid $console_pid) ──"
}

if [[ "$stub" == "1" ]]; then
  python3 -u scripts/dev-oauth-stub.py --port 5199 --auto-consent &
  stub_pid=$!
  export PARTYTIME_ORIGIN="${PARTYTIME_ORIGIN:-http://127.0.0.1:5199}"
  echo "── oauth stub on http://127.0.0.1:5199 (pid $stub_pid) ──"
fi

build || exit 1
start_console
# --- watch --------------------------------------------------------------------------
# Poll modification times rather than using inotify: no extra tool, and a poll at this
# interval is well under the time it takes to rebuild anyway.
sources() {
  find crates -name '*.rs' -newermt "@0" -printf '%T@ %p\n' 2>/dev/null | sort
}
previous="$(sources)"

echo "── watching crates/ — edit a file and the console restarts ──"

while true; do
  sleep 0.5
  current="$(sources)"
  [[ "$current" == "$previous" ]] && continue
  previous="$current"
  if build; then
    start_console
  else
    echo "── build failed; the console keeps running the last good build ──"
  fi
done
