#!/usr/bin/env bash
# Render every PeDeeFe PNG, .ico and .icns from the master SVGs in this directory.
#
# Needs: resvg (brew install resvg / cargo install resvg) and python3 (stdlib only, for the .ico
# and .icns). The outputs are committed, so nothing else ever needs these tools. After running it,
# update the sha256 values in ATTRIBUTION.toml, then `cargo xtask assets --write`.
#
#   assets/brand/pedeefe/render.sh          # or RESVG=/path/to/resvg assets/brand/pedeefe/render.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESVG="${RESVG:-resvg}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v "$RESVG" >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

ICON="$DIR/logo/pedeefe-icon.svg"
FULL="$DIR/web/pedeefe-fullbleed.svg"
DOC="$DIR/document/pedeefe-document.svg"

# render SVG SIZE OUT: a square render. renderw SVG WIDTH OUT keeps the aspect ratio.
render() { "$RESVG" -w "$2" -h "$2" "$1" "$3" </dev/null; }
renderw() { "$RESVG" -w "$2" "$1" "$3" </dev/null; }

# write_ico OUT PNG...: PNG-compressed .ico entries (Windows Vista and later, every browser).
write_ico() {
  python3 - "$@" <<'PY'
import struct, sys
out, pngs = sys.argv[1], sys.argv[2:]
blobs = [open(p, "rb").read() for p in pngs]
head = struct.pack("<HHH", 0, 1, len(blobs))
entries, data, offset = b"", b"", 6 + 16 * len(blobs)
for b in blobs:
    w, h = struct.unpack(">II", b[16:24])  # IHDR
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(b), offset)
    data += b
    offset += len(b)
open(out, "wb").write(head + entries + data)
PY
}

# macOS: an 824 px body centred on a transparent 1024 canvas (Apple's icon grid).
MAC="$TMP/macos.svg"
{
  echo '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1024 1024">'
  echo '<image x="100" y="100" width="824" height="824" xlink:href="'"$ICON"'"/>'
  echo '</svg>'
} >"$MAC"

# --- app icon (desktop) ----------------------------------------------------------------------
A="$DIR/app-icon"
mkdir -p "$A"
render "$MAC" 1024 "$A/pedeefe-1024.png"
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$A/hicolor/${s}x${s}/apps"
  render "$ICON" "$s" "$A/hicolor/${s}x${s}/apps/pedeefe.png"
done
mkdir -p "$A/hicolor/scalable/apps"
cp "$ICON" "$A/hicolor/scalable/apps/pedeefe.svg"

PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$ICON" "$s" "$TMP/ico-$s.png"
  PNGS+=("$TMP/ico-$s.png")
done
write_ico "$A/pedeefe.ico" "${PNGS[@]}"

# .icns written directly (no iconutil needed): PNG entries, 16-1024 px, on Apple's grid.
for s in 16 32 64 128 256 512 1024; do render "$MAC" "$s" "$TMP/mac-$s.png"; done
python3 - "$A/pedeefe.icns" "$TMP" <<'PY'
import struct, sys
out, tmp = sys.argv[1], sys.argv[2]
# (OSType, pixel size): icp4/icp5/icp6 and ic07-ic09 are 1x; ic11-ic14 and ic10 are the @2x slots.
types = [("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128), ("ic08", 256), ("ic09", 512),
         ("ic11", 32), ("ic12", 64), ("ic13", 256), ("ic14", 512), ("ic10", 1024)]
body = b""
for t, s in types:
    png = open(f"{tmp}/mac-{s}.png", "rb").read()
    body += t.encode() + struct.pack(">I", 8 + len(png)) + png
open(out, "wb").write(b"icns" + struct.pack(">I", 8 + len(body)) + body)
PY

# --- document (file-type) icon -----------------------------------------------------------------
D="$DIR/document"
render "$DOC" 256 "$D/pedeefe-document-256.png"
PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$DOC" "$s" "$TMP/doc-$s.png"
  PNGS+=("$TMP/doc-$s.png")
done
write_ico "$D/pedeefe-document.ico" "${PNGS[@]}"

# --- web ---------------------------------------------------------------------------------------
W="$DIR/web"
cp "$ICON" "$W/favicon.svg"
for s in 16 32 48; do render "$ICON" "$s" "$TMP/fav-$s.png"; done
write_ico "$W/favicon.ico" "$TMP/fav-16.png" "$TMP/fav-32.png" "$TMP/fav-48.png"
render "$FULL" 180 "$W/apple-touch-icon.png"
render "$ICON" 192 "$W/icon-192.png"
render "$ICON" 512 "$W/icon-512.png"
render "$FULL" 512 "$W/icon-maskable-512.png"

# --- PNG exports of the logos and the social card ------------------------------------------------
P="$DIR/png"
mkdir -p "$P"
renderw "$DIR/logo/pedeefe-logo.svg" 1600 "$P/pedeefe-logo.png"
renderw "$DIR/logo/pedeefe-logo-white.svg" 1600 "$P/pedeefe-logo-white.png"
renderw "$DIR/logo/pedeefe-logo-stacked.svg" 1000 "$P/pedeefe-logo-stacked.png"
render "$ICON" 1024 "$P/pedeefe-icon-1024.png"
renderw "$DIR/social/pedeefe-social.svg" 1280 "$DIR/social/pedeefe-social.png"

echo "PeDeeFe images written to $DIR"
