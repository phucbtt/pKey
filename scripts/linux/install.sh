#!/bin/sh
# Installs (or removes) the pKey IBus engine on any distribution, without a package.
# Needs root only to write the component file, which ibus-daemon reads from
# /usr/share/ibus/component and nowhere else.
#
#   scripts/linux/install.sh              build and install under /usr
#   scripts/linux/install.sh --uninstall
set -eu
here=$(cd "$(dirname "$0")/../.." && pwd)
prefix=${PREFIX:-/usr}
as_root() { if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi; }

if [ "${1:-}" = "--uninstall" ]; then
    as_root rm -f "$prefix/libexec/ibus-engine-pkey" "$prefix/share/ibus/component/pkey.xml"
    as_root rm -rf "$prefix/share/pkey"
    echo "removed; run 'ibus restart' (your settings in ~/.config/pkey are kept)"
    exit 0
fi

target=x86_64-unknown-linux-musl
(cd "$here" && cargo build --profile dist -p pkey-ibus --target "$target")
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/Cargo.toml" | head -1)
xml=$(mktemp)
trap 'rm -f "$xml"' EXIT
sed "s#@EXEC@#$prefix/libexec/ibus-engine-pkey#; s#@ICON@#$prefix/share/pkey/pkey.png#; s#@VERSION@#$version#" \
    "$here/platform/linux-ibus/data/pkey.xml.in" > "$xml"
as_root install -Dm755 "$here/target/$target/dist/ibus-engine-pkey" "$prefix/libexec/ibus-engine-pkey"
as_root install -Dm644 "$here/platform/linux-ibus/data/pkey.png" "$prefix/share/pkey/pkey.png"
as_root install -Dm644 "$xml" "$prefix/share/ibus/component/pkey.xml"
echo "installed. Run 'ibus restart', then add pKey in Settings > Keyboard > Input Sources."
