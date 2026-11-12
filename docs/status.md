# Status

2026-11-12. **The ladder passes: all four rungs.** A board may adopt this crate.

Reference: gb-apu-python `b8cab61eb5392f0024dc3acc565c802e3b168258` (DMG, CGB and AGB rows,
odd-T hold in `run()`). Linux x86_64, rustc 1.93.1, the reference on PyPy 7.3.20.

| Rung | What | Result | Command |
| --- | --- | --- | --- |
| 1 | Every test in gb-apu-python's `tests/test_apu.py`, transcribed (`tests/apu.rs`; parametrized cases folded into loops) | 20 of 20 test functions pass (Python's 24 cases) | `scripts/rung1.sh` |
| 2 | Register-stream lockstep: 192 seeded scripts (64 per row), random and scripted writes, reads, ticks, `run()` by odd amounts, sequencer edges from a modelled DIV, power cycles, CGB double speed; both cores' traces (every read, PCM read, the outputs a host reads, `capture_state()`, every sample) compared byte for byte | 192 of 192 identical: 401,476 reads, 10,840,728 sample values, 720,993 trace lines, 112.8 s of APU time. Also seed 2 (96 scripts, 6,000 steps): 96 of 96, 483,931 reads, 12,466,164 samples | `scripts/rung2.sh [count] [seed] [steps]` |
| 3 | The 94 board streams (`tests/streams/`, the reference's at the pin, checked by the script) replayed: every read, PCM read and post-step output as the board saw them, and the samples' SHA-256 | 94 of 94 identical | `scripts/rung3.sh` |
| 4 | The 30 seeded faults of gb-apu-python's `scripts/mutation.py`, the same rules broken in `src/lib.rs` (`scripts/seeded_faults.py`) | 30 of 30 caught. Rung 2 alone catches all 30; rung 3 catches 19 (not the 9 AGB faults, the 7-bit taps or the odd-T hold, which the board streams never exercise); rung 1 catches 16 | `scripts/rung4.sh` |

`cargo test --release` is rungs 1 and 3 and needs nothing else; CI runs it with `cargo fmt
--check` and `cargo clippy -D warnings`. Rungs 2 and 4 need the reference:
`scripts/fetch_gbapupython.sh` clones it at the pin into `external/` (PyPy is used when found;
`PYTHON=` overrides, `JOBS=` sets the parallelism, default 8).

## The brief's claim about f0a7f48

gameboy-python pins gb-apu-python `f0a7f48`; this crate transcribes `b8cab61`. The brief says the
two differ only by the AGB row and the odd-T hold, which a board ticking even T never reaches.
Checked against `git diff f0a7f48 b8cab61 -- src`: **true**. The other changes are
refactors with the same results on the DMG and CGB rows: `READ_MASKS` is chosen per row (the DMG
and CGB keep the same table), and `wave_output()` became `(buffer * quarters) >> 2` with quarters
(0, 4, 2, 1), which equals the old `0` / `buffer >> 0, 1, 2` for every digit. A board that calls
`tick()` with even T, or `run()` with even T, never holds a T.

## Suspected errors in gb-apu-python

None found: no case where the reference looked wrong. Observations only (not changed, not
errors in the contract's domain):

- Addresses outside $FF10-$FF3F: Python indexes from the end of its register array (below $FF10)
  or raises (above $FF3F). Here they read $FF and writes are ignored. No host passes them.
- `write()` values above $FF: Python's `bytearray` raises; the Rust API takes a `u8`.

## The API a board uses

Crate `gb_apu` (package `gb-apu-rust`; `gb-apu-rust = { path = "../gb-apu-rust" }`, then
`use gb_apu::...`). The library has no dependencies. The names are gb-apu-python's
(`docs/contract.md` of the reference is the contract):

```rust
use gb_apu::{APU, Model, DMG, CGB, AGB, MODELS, model, APUState};
use gb_apu::{NR10, NR11, /* ... */ NR52, WAVE_RAM};      // u16 Game Boy addresses

let mut apu = APU::new(44100, CGB);   // APU(sample_rate, model); APU::default() = 44,100 Hz DMG
apu.skip_boot();                      // the boot ROM's hand-off state

// the clock
apu.tick(4);                          // tick(t: u32): t T-cycles, even (4 per M-cycle, 2 in double speed)
apu.run(70224);                       // run(t: u64): any t, in 4-T steps; an odd T is held

// the frame sequencer (the host's: DIV bit 12 falling / rising, bit 13 in double speed)
apu.frame_clock_high = true;          // the clock's level, set before an NR52 write
apu.frame_tick();
apu.secondary_tick();

// registers
apu.write(NR52, 0x80);                // write(address: u16, value: u8), $FF10-$FF26 and $FF30-$FF3F
let v: u8 = apu.read(NR52);           // unused addresses read $FF
let pcm12: u8 = apu.pcm(0);           // pcm(pair: usize): PCM12 (0) / PCM34 (1), CGB $FF76/$FF77

// outputs
let d: [u8; 4] = apu.digital();       // each channel's DAC input, 0-15
let on: [bool; 4] = apu.dacs();
let (nr50, nr51) = (apu.nr50(), apu.nr51());
let ch: [bool; 4] = apu.channel_on(); // Python's channel_on property
let vols: [i64; 3] = apu.volumes();   // CH1, CH2, CH4 envelopes
let (digit, q) = (apu.wave_digit(), apu.wave_quarters());
let loud: bool = apu.audible();
apu.capture = true;                   // then, at sample_rate:
let samples: Vec<i16> = apu.take_samples();  // interleaved stereo since the last call

// state
let s: APUState = apu.capture_state(); // model, t, power, registers, wave_ram, divider,
                                       // enabled, lengths, volumes, digital; PartialEq + Clone
```

Python properties are methods here (`channel_on()`, `nr50()`, `nr51()`); `frame_clock_high`,
`capture`, `sample_rate`, `t` and the channels (`ch1`-`ch4`, with their Python field names) are
public fields, as they are public attributes in Python. Everything else on `APU` is internal, as
in the reference. Restoring state is not offered (nor in Python).

## Open

- Speed is not measured; correct, not fast (a `-fast` fork is the place).
- Local `cargo clippy` is too old for this toolchain; the library, `tests/apu.rs` and the example
  were linted with the local nightly `clippy-driver` directly, CI runs the real thing.
- The wasm32 build is untried here (the target is not installed locally); the library is plain
  `std` with no dependencies.
