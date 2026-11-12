#!/usr/bin/env bash
# Rung 2: register-stream lockstep. Random and scripted write/read/tick scripts for the three
# rows (scripts/lockstep_gen.py) run on both cores; every read value, every traced output and
# every sample must be identical.
# Usage: scripts/rung2.sh [count per row, default 64] [seed, default 1] [steps, default 2500]
# BIN: another build of examples/gb-apu-trace to judge (scripts/seeded_faults.py uses it);
# REUSE=1 keeps the reference traces already in external/lockstep.
set -euo pipefail
source "$(dirname "$0")/env.sh"
COUNT=${1:-64}; SEED=${2:-1}; STEPS=${3:-2500}
DIR="$ROOT/external/lockstep/seed$SEED-n$COUNT-s$STEPS"
mkdir -p "$DIR/scripts" "$DIR/ref" "$DIR/ours"
if [ -z "${BIN:-}" ]; then
    (cd "$ROOT" && nice cargo build --release -q --example gb-apu-trace)
    BIN="$ROOT/target/release/examples/gb-apu-trace"
fi
if [ "${REUSE:-0}" != 1 ] || [ ! -f "$DIR/ref.done" ]; then
    python3 "$ROOT/scripts/lockstep_gen.py" "$DIR/scripts" --count "$COUNT" --seed "$SEED" --steps "$STEPS" >/dev/null
    ls "$DIR"/scripts/*.txt | nice xargs -P "$JOBS" -n 8 env PYTHONPATH="$REF/src" "$PYTHON" "$ROOT/scripts/lockstep_ref.py" --out-dir "$DIR/ref"
    touch "$DIR/ref.done"
fi
OURS=${OURS:-"$DIR/ours"}  # this core's traces (seeded_faults.py points it into its copy)
mkdir -p "$OURS"
ls "$DIR"/scripts/*.txt | nice xargs -P "$JOBS" -n 16 "$BIN" --out-dir "$OURS"
status=0; same=0; total=0; lines=0
for ref in "$DIR"/ref/*.trace; do
    total=$((total + 1))
    name=$(basename "$ref")
    if cmp -s "$ref" "$OURS/$name"; then
        same=$((same + 1)); lines=$((lines + $(wc -l < "$ref")))
    else
        status=1
        first=$(diff <(cut -c1-160 "$ref") <(cut -c1-160 "$OURS/$name") | head -4 | tr '\n' ' ' || true)
        echo "DIFFERENT ${name%.trace}: $first"
    fi
done
samples=$(cat "$DIR"/ref/*.trace | awk '$1=="take"{n+=$2} END{print n+0}')
reads=$(cat "$DIR"/ref/*.trace | grep -cE '^(r|p) ' || true)
echo "rung 2: $same of $total scripts identical ($reads reads, $samples sample values, $lines trace lines)"
[ $status = 0 ] && echo "rung 2: PASS" || echo "rung 2: FAIL"
exit $status
