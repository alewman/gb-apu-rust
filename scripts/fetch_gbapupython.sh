#!/usr/bin/env bash
# The reference, gb-apu-python, at the pin, into external/gb-apu-python. It imports nothing, so
# no venv: the scripts run it with PYTHONPATH. PyPy is used when found (several times faster).
# Usage: scripts/fetch_gbapupython.sh [source repo, default ../gb-apu-python or the GitHub remote]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
# e5aeb5f = b8cab61 after gb-apu-python's history rewrite of 2026-11-21 (same tree); a98fb94 (branch
# cleanroom-agb-wave, local) is that plus the AGB row's clean-room rewrite (src/agb.rs follows it).
PIN=a98fb948197451c3abcfdb0e4bebf756c0081482
SRC=${1:-}
if [ -z "$SRC" ]; then
    if [ -d "$ROOT/../gb-apu-python/.git" ]; then SRC="$ROOT/../gb-apu-python"
    else SRC=https://github.com/alewman/gb-apu-python; fi
fi
DEST="$ROOT/external/gb-apu-python"
mkdir -p "$ROOT/external"
[ -d "$DEST/.git" ] || git clone -q "$SRC" "$DEST"
git -C "$DEST" fetch -q origin 2>/dev/null || true
git -C "$DEST" checkout -q "$PIN"
echo "gb-apu-python at $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
