# Sourced by the rung scripts: the reference and its interpreter.
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
REF=${GB_APU_PYTHON:-"$ROOT/external/gb-apu-python"}
[ -f "$REF/src/gb_apu_python/__init__.py" ] || { echo "no reference at $REF: run scripts/fetch_gbapupython.sh" >&2; exit 2; }
if [ -z "${PYTHON:-}" ]; then
    PYTHON=$(command -v pypy3 || true)
    [ -n "$PYTHON" ] || { [ -x "$HOME/.local/opt/pypy/bin/pypy3" ] && PYTHON="$HOME/.local/opt/pypy/bin/pypy3"; }
    [ -n "$PYTHON" ] || PYTHON=python3
fi
JOBS=${JOBS:-8}
