#!/usr/bin/env bash
# The reference, gb-apu-python, at the pin, into external/gb-apu-python. It imports nothing, so
# no venv: the scripts run it with PYTHONPATH. PyPy is used when found (several times faster).
# Usage: scripts/fetch_gbapupython.sh [source repo, default ../gb-apu-python or the GitHub remote]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PIN=b8cab61eb5392f0024dc3acc565c802e3b168258
SRC=${1:-}
if [ -z "$SRC" ]; then
    if [ -d "$ROOT/../gb-apu-python/.git" ]; then SRC="$ROOT/../gb-apu-python"
    else SRC=git@github.com:alewman/gb-apu-python.git; fi
fi
DEST="$ROOT/external/gb-apu-python"
mkdir -p "$ROOT/external"
[ -d "$DEST/.git" ] || git clone -q "$SRC" "$DEST"
git -C "$DEST" fetch -q origin 2>/dev/null || true
git -C "$DEST" checkout -q "$PIN"
echo "gb-apu-python at $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
