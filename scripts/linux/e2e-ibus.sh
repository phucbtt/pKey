#!/bin/sh
# Runs tests/linux/e2e_ibus.py against a private ibus-daemon that starts the freshly built engine
# from a component file, exactly like an installed package would. The user's own IBus session is
# never touched.
#
# ibus-daemon only reads /usr/share/ibus/component, so the script enters a throw-away mount
# namespace (no root needed) and bind-mounts a directory holding the system components plus pKey's.
#
# Usage: scripts/linux/e2e-ibus.sh [path/to/ibus-engine-pkey]
set -eu
here=$(cd "$(dirname "$0")/../.." && pwd)
engine=$(realpath "${1:-$here/target/debug/ibus-engine-pkey}")
[ -x "$engine" ] || { echo "build first: cargo build -p pkey-ibus" >&2; exit 2; }

if [ -z "${PKEY_E2E_INNER:-}" ]; then
    export PKEY_E2E_INNER=1
    exec unshare --user --map-root-user --mount "$0" "$engine"
fi

tmp=$(mktemp -d)
trap 'kill "$daemon" 2>/dev/null || true; rm -rf "$tmp"' EXIT INT TERM
mkdir -p "$tmp/components" "$tmp/config" "$tmp/cache" "$tmp/state"
cp /usr/share/ibus/component/*.xml "$tmp/components/"
sed "s#@EXEC@#$engine#; s#@ICON@#pkey#; s#@VERSION@#test#" \
    "$here/platform/linux-ibus/data/pkey.xml.in" > "$tmp/components/pkey.xml"
mount --bind "$tmp/components" /usr/share/ibus/component

export XDG_CONFIG_HOME="$tmp/config" XDG_CACHE_HOME="$tmp/cache" XDG_STATE_HOME="$tmp/state" \
       PKEY_CONFIG_DIR="$tmp/pkey" IBUS_ADDRESS="unix:path=$tmp/bus"
unset DISPLAY WAYLAND_DISPLAY

ibus-daemon ${PKEY_E2E_DEBUG:+--verbose} --single --panel disable --config disable --cache none \
    --address "$IBUS_ADDRESS" >"$tmp/daemon.log" 2>&1 &
daemon=$!
i=0
while [ ! -S "$tmp/bus" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done

rc=0
python3 "$here/tests/linux/e2e_ibus.py" || rc=$?
if [ -n "${PKEY_E2E_DEBUG:-}" ]; then cat "$tmp/daemon.log"; ps -eo pid,args | grep "[i]bus-engine-pkey"; fi
exit $rc
