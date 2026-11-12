#!/usr/bin/env python3
"""A gate that cannot fail proves nothing: break this core one rule at a time and check that the
ladder catches every break.

    python3 scripts/seeded_faults.py [--jobs 4] [NAME ...]

The faults are gb-apu-python's 30 (its ``scripts/mutation.py`` at b8cab61), each the same rule
broken the same way in ``src/lib.rs``. Each is applied to a copy of the repository under
``external/faults/<name>/`` (the working tree is never touched) and judged by every rung:

- rung 1: ``cargo test --test apu`` (the transcribed seam tests);
- rung 2: the lockstep scripts against the reference's traces already written by
  ``scripts/rung2.sh`` with its defaults (run it first; the reference must be fetched);
- rung 3: ``cargo test --test board_streams`` (the 94 board streams).

A fault is caught when any rung fails; the table says which ones did.
"""

from __future__ import annotations

import argparse
import queue
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

#: (gb-apu-python's name for it, the exact text in src/lib.rs, its replacement)
FAULTS = [
    ("duty 75% inverted", "        [0, 1, 1, 1, 1, 1, 1, 0]];", "        [1, 0, 0, 0, 0, 0, 0, 1]];"),
    ("noise divisors from 16", "[8, 16, 32, 48, 64, 80, 96, 112]", "[16, 16, 32, 48, 64, 80, 96, 112]"),
    ("wave volume shifts off by one", "WAVE_QUARTERS: [i64; 4] = [0, 4, 2, 1];", "WAVE_QUARTERS: [i64; 4] = [0, 2, 1, 0];"),
    ("wave nibbles swapped", "                byte >> 4\n            } else {\n                byte & 0xF\n", "                byte & 0xF\n            } else {\n                byte >> 4\n"),
    ("wave period at 1 MHz", "ch3.timer += (2048 - ch3.frequency) * 2;", "ch3.timer += (2048 - ch3.frequency) * 4;"),
    ("lengths clocked on the even steps", "        if divider & 1 != 0 {\n", "        if divider & 1 == 0 {\n"),
    ("sweep on the wrong steps", "        if divider & 3 == 3 {\n", "        if divider & 3 == 1 {\n"),
    ("envelope on the wrong step", "            if divider & 7 == 7 {\n", "            if divider & 7 == 6 {\n"),
    ("DMG noise XOR, not XNOR", "            let bit = ((lfsr ^ (lfsr >> 1)) & 1) ^ 1; // XNOR of bits 0 and 1\n            lfsr = (lfsr >> 1) | (bit << 14);", "            let bit = (lfsr ^ (lfsr >> 1)) & 1;\n            lfsr = (lfsr >> 1) | (bit << 14);"),
    ("7-bit noise taps bit 5", "lfsr = (lfsr & !0x40) | (bit << 6);", "lfsr = (lfsr & !0x40) | (bit << 5);"),
    ("NR52 hides the power bit", "return 0x70 | (if self.power { 0x80 } else { 0 }) | status;", "return 0x70 | status;"),
    ("CH3 length is 6 bits", "0xFF1B => self.ch3.length = 256 - value,", "0xFF1B => self.ch3.length = 256 - (value & 0x3F),"),
    ("DMG lengths read-only while off", "if !self.cgb && matches!(address, 0xFF11 | 0xFF16 | 0xFF1B | 0xFF20) {", "if false && matches!(address, 0xFF11 | 0xFF16 | 0xFF1B | 0xFF20) {"),
    ("CGB power-off keeps the lengths", "            if cgb {\n                for index in 0..4 {", "            if false {\n                for index in 0..4 {"),
    ("a high sequencer clock spends nothing", "                self.skip_event = 1;\n", "                self.skip_event = 0;\n"),
    ("wave starts 2 T early", "WAVE_TRIGGER_DELAY: i64 = 6;", "WAVE_TRIGGER_DELAY: i64 = 4;"),
    ("narrower DMG wave corruption", "CORRUPT_HI: i64 = 3;", "CORRUPT_HI: i64 = 2;"),
    ("DAC polarity flipped", "let analog = (7.5 - digital as f64) / 7.5;", "let analog = (digital as f64 - 7.5) / 7.5;"),
    ("panning sides swapped", "            if nr51 & (0x10 << i) != 0 {\n", "            if nr51 & (0x01 << i) != 0 {\n"),
    ("AGB: the CPU reaches the playing bank", "        if self.registers[0x0A] & 0x40 != 0 {\n            0\n        } else {\n            32\n        }", "        if self.registers[0x0A] & 0x40 != 0 {\n            32\n        } else {\n            0\n        }"),
    ("AGB: bank 0 always plays", "        let start = if nr30 & 0x40 != 0 { 32 } else { 0 };\n", "        let start = 0;\n"),
    ("AGB: 64 digits never chain", "        if nr30 & 0x20 != 0 {\n", "        if false {\n"),
    ("AGB: the 64-digit ring drops a digit", "            digits[other + 31] = out;\n", "            digits[other + 31] = digits[other];\n"),
    ("AGB: a pointer, not a shift register", "            digits.copy_within(start + 1..start + 32, start);\n            digits[start + 31] = out;\n", "            digits[start + 31] = out;\n"),
    ("AGB: force bit ignored", "        if self.wave_banks && nr32 & 0x80 != 0 {\n", "        if false {\n"),
    ("AGB: force gives 50 %", "            return 3;\n", "            return 2;\n"),
    ("AGB: NR30's bank bits unreadable", "    masks[0x0A] = 0x1F;\n", "    masks[0x0A] = 0x7F;\n"),
    ("AGB: wave digits swapped on write", "            self.wave_digits[at] = value >> 4;\n            self.wave_digits[at + 1] = value & 0xF;\n", "            self.wave_digits[at + 1] = value >> 4;\n            self.wave_digits[at] = value & 0xF;\n"),
    ("run() drops an odd T", "        self.held_t = t & 1;\n", "        self.held_t = 0;\n"),
    ("PCM nibbles swapped", "outputs[2 * pair] | (outputs[2 * pair + 1] << 4)", "(outputs[2 * pair] << 4) | outputs[2 * pair + 1]"),
]  # fmt: skip


def slug(name: str) -> str:
    return "".join(c if c.isalnum() else "-" for c in name.lower()).strip("-").replace("--", "-")


def run(cmd: list[str], cwd: Path, env: dict | None = None) -> subprocess.CompletedProcess:
    import os

    return subprocess.run(
        ["nice", *cmd], cwd=cwd, env=dict(os.environ, **(env or {})), capture_output=True, text=True
    )


def judge(name: str, old: str, new: str, targets: queue.Queue) -> tuple[str, list[str], str]:
    copy = ROOT / "external" / "faults" / slug(name)
    copy.mkdir(parents=True, exist_ok=True)
    for item in ("Cargo.toml", "Cargo.lock", "src", "tests", "examples"):
        source = ROOT / item
        if source.is_dir():
            shutil.copytree(source, copy / item, dirs_exist_ok=True)
        else:
            shutil.copy2(source, copy / item)
    text = (ROOT / "src" / "lib.rs").read_text()
    if text.count(old) != 1:
        return name, [], f"fault text found {text.count(old)} times"
    (copy / "src" / "lib.rs").write_text(text.replace(old, new))
    target = targets.get()
    try:
        env = {"CARGO_TARGET_DIR": str(target)}
        build = run(["cargo", "build", "--release", "-q", "--tests", "--examples"], copy, env)
        if build.returncode:
            return name, [], "does not build: " + build.stderr.strip().splitlines()[-1]
        caught = []
        if run(["cargo", "test", "--release", "-q", "--test", "apu"], copy, env).returncode:
            caught.append("1")
        rung2 = run(
            [str(ROOT / "scripts" / "rung2.sh")],
            ROOT,
            {
                "BIN": str(target / "release" / "examples" / "gb-apu-trace"),
                "REUSE": "1",
                "OURS": str(copy / "traces"),
                "JOBS": "2",
            },
        )
        if rung2.returncode == 2:
            return name, caught, "rung 2 could not run: " + rung2.stderr.strip()
        if rung2.returncode:
            caught.append("2")
        if run(["cargo", "test", "--release", "-q", "--test", "board_streams"], copy, env).returncode:
            caught.append("3")
        detail = next((ln for ln in rung2.stdout.splitlines() if " scripts identical" in ln), "")
        return name, caught, "lockstep " + detail.removeprefix("rung 2: ").split(" (")[0]
    finally:
        targets.put(target)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("names", nargs="*", help="only faults whose name contains one of these")
    args = parser.parse_args()
    faults = [f for f in FAULTS if not args.names or any(n in f[0] for n in args.names)]
    targets: queue.Queue = queue.Queue()
    for i in range(args.jobs):
        targets.put(ROOT / "external" / "faults-target" / str(i))
    missed = []
    with ThreadPoolExecutor(args.jobs) as pool:
        futures = [pool.submit(judge, name, old, new, targets) for name, old, new in faults]
        for future in futures:
            name, caught, detail = future.result()
            rungs = "+".join(caught) if caught else "-"
            print(f"{'caught' if caught else 'MISSED'}  rungs {rungs:7} {name}  ({detail})", flush=True)
            if not caught:
                missed.append(name)
    print(f"{len(faults) - len(missed)} of {len(faults)} caught")
    return 1 if missed else 0


if __name__ == "__main__":
    sys.exit(main())
