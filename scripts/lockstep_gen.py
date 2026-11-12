#!/usr/bin/env python3
"""Write register-stream lockstep scripts (rung 2): random and scripted write/read/tick sequences
for the three rows, which both cores run (scripts/lockstep_ref.py, examples/gb-apu-trace.rs).

    python3 scripts/lockstep_gen.py OUT_DIR [--count 64] [--seed 1] [--steps 2500]

``--count`` scripts per row (dmg, cgb, agb), seeded so a run is reproducible. Imports nothing
from either core.

A script is text, one call per line (numbers decimal, addresses and values hex):

    model dmg | rate 44100      the constructor's row and sample rate (first two lines)
    boot                        skip_boot()
    capture 1                   capture = True
    t N K                       K calls of tick(N)
    run N                       run(N), any N (odd ones are held)
    h L                         frame_clock_high = L (0/1)
    w ADDR VAL                  write(ADDR, VAL)
    r ADDR                      read(ADDR)            -> traced
    p PAIR                      pcm(PAIR)             -> traced
    f / s                       frame_tick() / secondary_tick()
    d                           the outputs a host reads -> traced
    state                       capture_state()       -> traced
    take                        take_samples()        -> traced

Time follows a board: the sequencer edges come from a 16-bit system counter's bit 12 (frame_tick
on the fall, secondary_tick on the rise), a DIV write resets it (a fall when the bit was high),
and frame_clock_high is the bit's level before every NR52 write. The scripted parts are the
channels set up and triggered properly, the sweep, NRx2 "zombie" writes while playing, wave RAM
reached while CH3 plays (the DMG's 2-T window and trigger corruption), NR43 changes under the
CGB's noise counter, the AGB's bank and 64-digit bits, power cycles and odd run() steps.
"""

from __future__ import annotations

import argparse
import random
from pathlib import Path

REGS = list(range(0xFF10, 0xFF27))
WAVE = list(range(0xFF30, 0xFF40))
READABLE = list(range(0xFF10, 0xFF40))


class Script:
    def __init__(self, rng: random.Random, model: str) -> None:
        self.rng = rng
        self.model = model
        self.lines: list[str] = []
        self.div = rng.randrange(0x10000)  # the system counter, in T
        self.double = False  # CGB double speed: tick(2) and bit 13

    def emit(self, line: str) -> None:
        self.lines.append(line)

    # ----- time --------------------------------------------------------------------------

    def bit(self) -> int:
        return 13 if self.double else 12

    def advance(self, total: int) -> None:
        """``total`` T in the board's steps, with the sequencer edges where the counter puts them
        and the outputs traced after each batch."""
        step = 2 if self.double else 4
        if self.rng.random() < 0.15:
            step = 2  # the contract allows any even step
        total -= total % step
        while total > 0:
            period = 1 << (self.bit() + 1)
            half = period >> 1
            to_edge = half - (self.div % half)
            n = min(total, to_edge)
            n -= n % step
            if n == 0:
                n = step
            self.emit(f"t {step} {n // step}")
            before = (self.div >> self.bit()) & 1
            self.div = (self.div + n) & 0xFFFF
            after = (self.div >> self.bit()) & 1
            total -= n
            if before and not after:
                self.emit("f")
                self.emit("d")
            elif after and not before:
                self.emit("s")
            if self.rng.random() < 0.3:
                self.emit("d")

    def div_write(self) -> None:
        if (self.div >> self.bit()) & 1:
            self.emit("f")
        self.div = 0

    def level(self) -> int:
        return (self.div >> self.bit()) & 1

    def run_odd(self) -> None:
        """A host on another clock: run() by arbitrary amounts. The counter moves with it, but the
        edges then fall between calls, as they would for such a host."""
        for _ in range(self.rng.randint(1, 8)):
            n = self.rng.choice([1, 3, 5, 7, 9, 2, 6, 11, 13, self.rng.randint(1, 3000)])
            self.emit(f"run {n}")
            before = self.level()
            self.div = (self.div + n) & 0xFFFF
            if before and not self.level():
                self.emit("f")
            elif self.level() and not before:
                self.emit("s")
        self.emit("d")

    # ----- registers ---------------------------------------------------------------------

    def w(self, address: int, value: int) -> None:
        if address == 0xFF26:
            self.emit(f"h {self.level()}")
        self.emit(f"w {address:04X} {value & 0xFF:02X}")

    def r(self, address: int) -> None:
        self.emit(f"r {address:04X}")

    def reads(self) -> None:
        rng = self.rng
        for _ in range(rng.randint(1, 6)):
            self.r(rng.choice(READABLE))
        if rng.random() < 0.3:
            self.emit(f"p {rng.randint(0, 1)}")

    def byte(self) -> int:
        rng = self.rng
        return rng.choice([rng.randrange(256), rng.randrange(256), 0x00, 0xFF, 0x80, 0x08, 0xF8])

    # ----- scripted parts ----------------------------------------------------------------

    def pulse(self, channel: int) -> None:
        rng = self.rng
        base = 0xFF10 if channel == 0 else 0xFF15
        if channel == 0:
            self.w(0xFF10, rng.choice([0x00, rng.randrange(128), 0x11, 0x19, 0x77, 0x7F]))
        self.w(base + 1, rng.randrange(256))
        self.w(base + 2, rng.choice([0xF0, 0xF1, 0x0F, 0x08, 0xA3, 0x1A, rng.randrange(256)]))
        self.w(base + 3, rng.randrange(256))
        self.w(base + 4, 0x80 | rng.choice([0x40, 0x00]) | rng.randrange(8))

    def wave(self) -> None:
        rng = self.rng
        self.w(0xFF1A, 0x00)
        for address in WAVE:
            self.w(address, rng.randrange(256))
        nr30 = 0x80 | (rng.choice([0x00, 0x20, 0x40, 0x60]) if self.model == "agb" else 0)
        self.w(0xFF1A, nr30)
        self.w(0xFF1B, rng.randrange(256))
        self.w(0xFF1C, rng.randrange(256))
        self.w(0xFF1D, rng.choice([rng.randrange(256), 0xFF, 0xFE]))
        self.w(0xFF1E, 0x80 | rng.choice([0x40, 0x00]) | rng.choice([7, 7, rng.randrange(8)]))

    def wave_access(self) -> None:
        """Wave RAM reached while CH3 plays, T by T: the DMG's window and the trigger
        corruption."""
        rng = self.rng
        for _ in range(rng.randint(2, 12)):
            self.emit("t 2 1")
            self.div = (self.div + 2) & 0xFFFF
            choice = rng.random()
            if choice < 0.4:
                self.r(rng.choice(WAVE))
            elif choice < 0.7:
                self.w(rng.choice(WAVE), rng.randrange(256))
            elif choice < 0.85:
                self.w(0xFF1E, 0x80 | rng.randrange(8) | rng.choice([0, 0x40]))
            else:
                self.emit("d")

    def noise(self) -> None:
        rng = self.rng
        self.w(0xFF20, rng.randrange(256))
        self.w(0xFF21, rng.choice([0xF0, 0xF1, 0x0F, 0xA2, rng.randrange(256)]))
        self.w(0xFF22, rng.choice([0x00, 0x08, 0x01, 0x11, 0x47, 0xF0, rng.randrange(256)]))
        self.w(0xFF23, 0x80 | rng.choice([0x40, 0x00]))

    def nr43_changes(self) -> None:
        rng = self.rng
        for _ in range(rng.randint(1, 6)):
            self.w(0xFF22, rng.randrange(256))
            self.emit(f"t 2 {rng.randint(1, 6)}")
            self.div = (self.div + 12) & 0xFFFF  # approximate; edges are not due this close
            self.emit("d")

    def zombie(self) -> None:
        rng = self.rng
        address = rng.choice([0xFF12, 0xFF17, 0xFF21])
        for _ in range(rng.randint(1, 5)):
            self.w(address, rng.choice([0x08, 0x09, 0x0F, 0xF8, 0xF0, 0x18, rng.randrange(256)]))
            self.emit("d")

    def sweep_changes(self) -> None:
        rng = self.rng
        for _ in range(rng.randint(1, 4)):
            self.w(0xFF10, rng.randrange(128))
            self.advance(rng.choice([4, 8, 12, 64, 4096]))
            self.r(0xFF26)

    def frequency_changes(self) -> None:
        rng = self.rng
        base = rng.choice([0xFF13, 0xFF18])
        for _ in range(rng.randint(1, 4)):
            if rng.random() < 0.5:
                self.w(base, rng.randrange(256))
            else:
                self.w(base + 1, rng.choice([0x07, 0x06, 0x47, 0x00, rng.randrange(256)]))
            self.advance(rng.choice([4, 8, 16]))
            self.emit("p 0")

    def power(self) -> None:
        rng = self.rng
        self.w(0xFF26, 0x00)
        if rng.random() < 0.5:  # DMG: the lengths stay writable while off
            self.w(rng.choice([0xFF11, 0xFF16, 0xFF1B, 0xFF20]), rng.randrange(256))
            self.w(rng.choice(REGS), rng.randrange(256))
        self.advance(rng.choice([4, 64, 8192]))
        self.w(0xFF26, 0x80)
        self.w(0xFF24, 0x77)
        self.w(0xFF25, rng.choice([0xFF, 0xF3, rng.randrange(256)]))

    def random_writes(self) -> None:
        rng = self.rng
        for _ in range(rng.randint(1, 4)):
            address = rng.choice(REGS + WAVE[:4])
            if address == 0xFF26:
                value = rng.choice([0x80, 0x80, 0x00])
            else:
                value = self.byte()
            self.w(address, value)

    def agb_banks(self) -> None:
        rng = self.rng
        for _ in range(rng.randint(1, 4)):
            self.w(0xFF1A, rng.choice([0x80, 0xA0, 0xC0, 0xE0, 0x00, 0x40, 0x60, 0x20]))
            if rng.random() < 0.5:
                self.w(0xFF1C, rng.choice([0x80, 0x20, 0x40, 0x60, 0x00, 0xA0]))
            self.advance(rng.choice([4, 8, 12, 64]))
            self.reads()

    # ----- the whole -------------------------------------------------------------------

    def build(self, steps: int, rate: int) -> list[str]:
        rng = self.rng
        self.emit(f"model {self.model}")
        self.emit(f"rate {rate}")
        if rng.random() < 0.5:
            self.emit("boot")
        else:
            self.w(0xFF26, 0x80)
        self.emit("capture 1")
        self.w(0xFF24, rng.choice([0x77, 0x77, rng.randrange(256)]))
        self.w(0xFF25, rng.choice([0xFF, 0xF3, rng.randrange(256)]))
        self.double = self.model != "dmg" and rng.random() < 0.25
        parts = [
            (self.pulse_0, 8), (self.pulse_1, 8), (self.wave, 6), (self.noise, 6),
            (self.wave_access, 4), (self.nr43_changes, 3), (self.zombie, 4),
            (self.sweep_changes, 3), (self.frequency_changes, 4), (self.power, 1),
            (self.random_writes, 10), (self.reads, 10), (self.run_odd, 3), (self.div_write, 2),
            (self.long_wait, 2), (self.short_wait, 20), (self.snap, 2),
        ]  # fmt: skip
        if self.model == "agb":
            parts.append((self.agb_banks, 5))
        funcs, weights = zip(*parts, strict=True)
        for _ in range(steps):
            rng.choices(funcs, weights)[0]()
        self.emit("d")
        self.emit("state")
        self.emit("take")
        return self.lines

    def pulse_0(self) -> None:
        self.pulse(0)

    def pulse_1(self) -> None:
        self.pulse(1)

    def long_wait(self) -> None:
        self.advance(self.rng.randint(8192, 70224))

    def short_wait(self) -> None:
        self.advance(self.rng.choice([4, 8, 12, 16, 32, 100, 456, 1000, self.rng.randint(4, 4000)]))

    def snap(self) -> None:
        self.emit("state")
        if self.rng.random() < 0.5:
            self.emit("take")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("out", type=Path)
    parser.add_argument("--count", type=int, default=64, help="scripts per row")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--steps", type=int, default=2500)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for model in ("dmg", "cgb", "agb"):
        for i in range(args.count):
            rng = random.Random(f"{args.seed}-{model}-{i}")
            rate = rng.choice([44100, 44100, 48000, 32768, 22050, 96000])
            lines = Script(rng, model).build(args.steps, rate)
            (args.out / f"{model}-{i:03d}.txt").write_text("\n".join(lines) + "\n")
    print(f"{3 * args.count} scripts in {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
