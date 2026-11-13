# gb-apu-rust

The Game Boy APU (the two pulse channels, the wave channel, the noise channel, the frame
sequencer and a mixer) for the DMG, the CGB and the Game Boy Advance's four Game Boy channels,
transcribed from [gb-apu-python](https://github.com/alewman/gb-apu-python) at `b8cab61` and
proven equivalent to it: the same register reads at the same T and the same output stream, for
every row. It is the sound unit of [gameboy-rust](https://github.com/alewman/gameboy-rust).

`src/lib.rs` is gb-apu-python's `__init__.py` function for function, in the same order, with the
same names and hardware comments. Nothing is redesigned; where Rust forced a difference the
module header says what it is. The library has no dependencies. The package is `gb-apu-rust`,
the crate `gb_apu`; it is not published to crates.io (use it by path or git).

## Using the core

```rust
use gb_apu::{APU, CGB, NR52};

let mut apu = APU::new(44100, CGB);
apu.write(NR52, 0x80);      // registers by their Game Boy address
apu.run(70224);             // the clock: T-cycles of 4,194,304 Hz
apu.frame_tick();           // the frame sequencer: the host's 512 Hz clock
apu.digital();              // per-channel DAC inputs (0-15)
apu.dacs();                 // DAC enables
apu.capture_state();        // an immutable snapshot
```

The contract is the reference's `docs/contract.md`; [docs/status.md](docs/status.md) lists the
whole Rust API a board uses.

**Save states: the `serde` feature.** With `features = ["serde"]` the whole `APU` is
`Serialize`/`Deserialize` (every field; the model row by its name), so a host's save states can
carry it and restore it exactly; `cargo test --features serde` replays all 94 board streams
through a save state half way. gb-apu-python has no restore: this is the Rust crate's own.
Without the feature the core has no dependencies. `capture_state()` stays a comparison
snapshot.

## Status

All four rungs pass. Results and exact commands: [docs/status.md](docs/status.md).

| Rung | What | Result |
| --- | --- | --- |
| 1 | gb-apu-python's seam tests, transcribed | 20 of 20 |
| 2 | Register-stream lockstep, 192 seeded scripts over the three rows, every read, output and sample | 192 of 192 identical |
| 3 | The 94 board streams recorded from gameboy-python | 94 of 94 identical |
| 4 | gb-apu-python's 30 seeded faults, broken the same way here | 30 of 30 caught |

```text
cargo test --release           # rungs 1 and 3; needs nothing else (CI runs it with fmt and clippy)
scripts/fetch_gbapupython.sh   # the reference at the pin into external/ (for rungs 2 and 4)
scripts/rung1.sh
scripts/rung2.sh [count] [seed] [steps]
scripts/rung3.sh
scripts/rung4.sh
```

`tests/streams/` are gb-apu-python's board streams at the pin: the calls gameboy-python's board
made into its APU while running APU test ROMs (register traffic and sample hashes, no ROM
content). [docs/handoff-brief.md](docs/handoff-brief.md) is the brief the crate was built from.

## License

MIT (`LICENSE`), like the reference.
