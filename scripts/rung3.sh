#!/usr/bin/env bash
# Rung 3: the 94 board streams (gb-apu-python's tests/streams, recorded from gameboy-python by
# its scripts/export_board_streams.py) replayed on this core: every read, PCM read and step's
# outputs as the board saw them, and the samples' SHA-256 (tests/board_streams.rs). When the
# reference is fetched, the copies here are first checked to be its files at the pin.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REF=${GB_APU_PYTHON:-"$ROOT/external/gb-apu-python"}
cd "$ROOT"
if [ -d "$REF/tests/streams" ]; then
    diff -rq "$REF/tests/streams" tests/streams >/dev/null || { echo "tests/streams differ from the reference's"; exit 1; }
    echo "streams: $(ls tests/streams | wc -l) files, the reference's at the pin"
fi
out=$(nice cargo test --release -q --test board_streams -- --nocapture 2>&1) || { echo "$out" | grep -E "FAIL|differ"; echo "rung 3: FAIL"; exit 1; }
echo "$out" | grep -E "board streams identical"
echo "rung 3: PASS"
