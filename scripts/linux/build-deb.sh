#!/bin/sh
# Builds dist/pkey-ibus_<version>_amd64.deb: the IBus engine as one static binary (musl), so the
# same package installs on every Ubuntu/Debian release; there is no glibc or library requirement
# beyond ibus itself.
#
#   scripts/linux/build-deb.sh
set -eu
here=$(cd "$(dirname "$0")/../.." && pwd)
cd "$here"

target=x86_64-unknown-linux-musl
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --profile dist -p pkey-ibus --target "$target"
bin=target/$target/dist/ibus-engine-pkey

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
install -Dm755 "$bin" "$root/usr/libexec/ibus-engine-pkey"
install -Dm644 platform/linux-ibus/data/pkey.png "$root/usr/share/pkey/pkey.png"
install -Dm644 LICENSE "$root/usr/share/doc/pkey-ibus/copyright"
mkdir -p "$root/usr/share/ibus/component"
sed "s#@EXEC@#/usr/libexec/ibus-engine-pkey#; s#@ICON@#/usr/share/pkey/pkey.png#; s#@VERSION@#$version#" \
    platform/linux-ibus/data/pkey.xml.in > "$root/usr/share/ibus/component/pkey.xml"
find "$root" -type d -exec chmod 755 {} +
chmod 644 "$root/usr/share/ibus/component/pkey.xml"

mkdir -p "$root/DEBIAN"
size=$(du -sk "$root/usr" | cut -f1)
cat > "$root/DEBIAN/control" <<CTL
Package: pkey-ibus
Version: $version
Architecture: amd64
Maintainer: pKey <noreply@github.com>
Installed-Size: $size
Depends: ibus
Section: utils
Priority: optional
Homepage: https://github.com/phucbtt/pKey
Description: pKey Vietnamese input method for IBus
 Telex, VNI and VIQR typing with tone placement, spelling check and macros.
 Statically linked: runs on any Ubuntu or Debian release that has IBus.
CTL
install -m755 platform/linux-ibus/data/postinst "$root/DEBIAN/postinst"
install -m755 platform/linux-ibus/data/postrm "$root/DEBIAN/postrm"

mkdir -p dist
out=dist/pkey-ibus_${version}_amd64.deb
dpkg-deb --root-owner-group --build "$root" "$out" >/dev/null
echo "$out"
