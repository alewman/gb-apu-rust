#!/usr/bin/env bash
# Rung 1: every test in gb-apu-python's tests/test_apu.py, transcribed (tests/apu.rs).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
out=$(nice cargo test --release -q --test apu 2>&1) || { echo "$out"; echo "rung 1: FAIL"; exit 1; }
echo "$out" | grep -E "^test result"
echo "rung 1: PASS ($(echo "$out" | grep -oE '[0-9]+ passed') of tests/apu.rs)"
