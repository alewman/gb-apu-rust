# gb-apu-rust

The Game Boy APU (the two pulse channels, the wave channel, the noise channel, the frame
sequencer and a mixer) for the DMG, the CGB and the Game Boy Advance's four Game Boy channels,
transcribed from [gb-apu-python](https://github.com/alewman/gb-apu-python) at `b8cab61` and
proven equivalent to it: the same register reads at the same T and the same output stream, for
every row.

`src/lib.rs` is gb-apu-python's `__init__.py` function for function, in the same order, with the
same names and hardware comments. Nothing is redesigned; where Rust forced a difference the
module header says what it is. The library has no dependencies.

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

## The ladder

| Rung | What | Result |
| --- | --- | --- |
| 1 | gb-apu-python's seam tests, transcribed | 20 of 20 |
| 2 | Register-stream lockstep, 192 seeded scripts over the three rows, every read, output and sample | 192 of 192 identical |
| 3 | The 94 board streams recorded from gameboy-python | 94 of 94 identical |
| 4 | gb-apu-python's 30 seeded faults | 30 of 30 caught |

Exact numbers and commands: [docs/status.md](docs/status.md).

```text
scripts/fetch_gbapupython.sh   # the reference at the pin into external/
scripts/rung1.sh
scripts/rung2.sh [count] [seed] [steps]
scripts/rung3.sh
scripts/rung4.sh
```

`cargo test --release` runs rungs 1 and 3 without the reference (CI runs it with fmt and
clippy).

## Where the brief is

[docs/handoff-brief.md](docs/handoff-brief.md) is the request this repository was built from.

## License

MIT, like the reference.
