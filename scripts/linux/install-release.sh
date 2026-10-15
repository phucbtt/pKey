#!/bin/sh
# Installs the pKey IBus engine from a released .deb, like `apt install ibus-bamboo`:
#   curl -fsSL https://raw.githubusercontent.com/phucbtt/pKey/main/scripts/linux/install-release.sh | sh
# Runs as your user; it calls sudo only for apt. Your other input sources are kept.
set -eu
version=${PKEY_VERSION:-0.1.0}
deb=pkey-ibus_${version}_amd64.deb
url=${PKEY_DEB_URL:-https://github.com/phucbtt/pKey/releases/download/v${version}/${deb}}
sha256=${PKEY_DEB_SHA256:-0b9a3452a3a5707ebb92f1a717daeca756637590d8c7124ea1690a1a1d05b5b9}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading $deb ..."
curl -fsSL -o "$tmp/$deb" "$url"
echo "$sha256  $tmp/$deb" | sha256sum -c - >/dev/null || { echo "checksum mismatch, not installing" >&2; exit 1; }

sudo apt-get update -qq
sudo apt-get install -y "$tmp/$deb"

# Add pKey to the input sources once; keep the ones the user already has.
if command -v gsettings >/dev/null 2>&1; then
    current=$(gsettings get org.gnome.desktop.input-sources sources 2>/dev/null || echo "[]")
    updated=$(python3 - "$current" <<'PY'
import ast, sys
raw = sys.argv[1].replace("@a(ss) ", "")
sources = ast.literal_eval(raw)
if ("ibus", "pkey") not in [tuple(s) for s in sources]:
    sources.append(("ibus", "pkey"))
print(repr(sources).replace('"', "'"))
PY
)
    gsettings set org.gnome.desktop.input-sources sources "$updated" 2>/dev/null || true
fi

ibus restart >/dev/null 2>&1 || true
echo "pKey installed. Switch to it with Super+Space (or the input menu in the top bar)."
