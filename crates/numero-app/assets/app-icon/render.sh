#!/usr/bin/env bash
# Renders numero.svg into png/ (16 to 1024) and numero.icns, and into the
# Linux hicolor theme under ../linux/icons.
#
# Chrome draws the SVG, because its filters (blur, offset, composite) are
# what the glass is made of and smaller renderers skip them. ImageMagick
# scales the 1024 render down, and a few lines of Python pack the icns.
#
# Linux icons are the same render cropped closer: Apple's grid leaves a
# 100 margin round the tile, Linux themes fill most of the square, so the
# crop keeps 48 of it (the tile fills 89%) and the shadow still fits. The
# sizes up to 48 are sharpened a little so the crossbars hold.
#
# Usage: render.sh   (or `just icon` from the repository root)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sizes=(16 32 64 128 256 512)
hicolor="$here/../linux/icons/hicolor"
linux_sizes=(16 22 24 32 48 64 128 256 512)

chrome=""
for candidate in google-chrome google-chrome-stable chromium chromium-browser \
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"; do
  if command -v "$candidate" >/dev/null 2>&1 || [[ -x "$candidate" ]]; then
    chrome="$candidate"
    break
  fi
done
if [[ -z "$chrome" ]]; then
  echo "render.sh: needs Chrome or Chromium to draw the SVG" >&2
  exit 1
fi
for tool in magick python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "render.sh: needs $tool" >&2
    exit 1
  fi
done

mkdir -p "$here/png"
"$chrome" --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=1 --default-background-color=00000000 \
  --window-size=1024,1024 --screenshot="$here/png/numero-1024.png" \
  "file://$here/numero.svg" >/dev/null 2>&1
for size in "${sizes[@]}"; do
  magick "$here/png/numero-1024.png" -filter Lanczos -resize "${size}x${size}" \
    -strip "$here/png/numero-$size.png"
done
magick "$here/png/numero-1024.png" -strip "$here/png/numero-1024.png"

python3 - "$here" <<'PY'
"""Packs png/ into numero.icns: each entry is a type, a length and a PNG."""
import struct
import sys
from pathlib import Path

here = Path(sys.argv[1])
entries = [
    (b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128),
    (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024),
    (b"ic11", 32), (b"ic12", 64), (b"ic13", 256), (b"ic14", 512),
]
body = b""
for kind, size in entries:
    png = (here / "png" / f"numero-{size}.png").read_bytes()
    body += kind + struct.pack(">I", len(png) + 8) + png
(here / "numero.icns").write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)
PY

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
linux_master="$scratch/numero-928.png"
magick "$here/png/numero-1024.png" -crop 928x928+48+48 +repage "$linux_master"
for size in "${linux_sizes[@]}"; do
  mkdir -p "$hicolor/${size}x${size}/apps"
  sharpen=""
  if ((size <= 48)); then
    sharpen="0x0.6+0.8+0"
  fi
  magick "$linux_master" -filter Lanczos -resize "${size}x${size}" \
    ${sharpen:+-unsharp "$sharpen"} -strip "$hicolor/${size}x${size}/apps/numero.png"
done

echo "render.sh: wrote png/, numero.icns and ../linux/icons/hicolor"
