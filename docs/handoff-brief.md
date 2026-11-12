> Historical: the brief this crate was built from, 2026-11-12; current state in [status.md](status.md).

# Handoff brief: gb-apu-rust

## The request

Aubrey, 2026-11-12: the family's Rust path for the Game Boy (sm83-rust done →
gb-apu-rust and gameboy-rust in parallel → -fast → a play-gb browser page).
This brief is the sound chip core.

## What it is

A Rust transcription of gb-apu-python (`/data/emu/gb-apu-python`, GitHub
`alewman/gb-apu-python`, PRIVATE) at **`b8cab61`** (master: DMG, CGB and AGB
rows, odd-T hold in `run()`). gameboy-python pins `f0a7f48`; the two differ
only by the AGB row and the odd-T hold, which a board ticking even T never
reaches — check that claim and say so in docs if it is wrong.

Equivalent at every sample and every register read: same output stream (left,
right, per tick as the Python core produces it) and same register reads at
the same T, for every row.

Crate `gb_apu` (repo `/data/emu/gb-apu-rust`). Create a **private** GitHub
remote `alewman/gb-apu-rust` when the ladder passes and push; minimal CI
(main + PRs, concurrency cancel; fmt, clippy -D warnings, test, the cheap rungs).

## What to transcribe

`src/gb_apu_python/__init__.py` at `b8cab61`, same names, same structure,
hardware comments carried over. Read `docs/contract.md` (the host API a board
drives: tick/run, register reads/writes, DIV-APU events, PCM12/34, wave RAM
access rules per row) and `docs/validation.md` first.

## The ladder

Each rung has a script in `scripts/` and passes only when its output says so.

1. **Unit tests**: every test in gb-apu-python's `tests/` that exercises the
   core, transcribed to `cargo test`, all pass.
2. **Register-stream lockstep**: random and scripted write/read/tick
   sequences (all three rows) run on both cores; every read value and every
   output sample identical.
3. **Board streams**: the 94 board streams (`scripts/export_board_streams.py`
   produces them from gameboy-python runs) replayed on the Rust core:
   identical samples.
4. **Seeded faults**: the 30 mutations of `scripts/mutation.py` (or their Rust
   equivalents) — report how many the ladder catches; gb-apu-python's own
   ladder caught 30/30.

## Working rules

- Do not modify gb-apu-python. If it looks wrong, stop and show the case.
- Transcribe; do not redesign. Correct, not fast. Model the repo on
  `/data/emu/sn76489-rust` and `/data/emu/ym2612-rust` (sound cores; layout,
  scripts/rungN.sh, README, docs/handoff-brief.md = a copy of this file).
- Never copy GPL code; nothing third-party committed.
- `nice`, at most about 8 processes (shared machine; another agent is building
  gameboy-rust at the same time). Local `cargo clippy` may be too old to build;
  CI runs it.
- No `rm -rf`, `sed -i` or `ln -sfn` one-liners; use the Edit tool and new
  directories instead.
- Commit locally at good points without asking. Commit trailer:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Leave `docs/status.md` with what passed, what is open and the exact
  commands, when you stop.
