#!/usr/bin/env python3
"""Run lockstep scripts (scripts/lockstep_gen.py) on gb-apu-python and print the trace that
examples/gb-apu-trace.rs prints for this core.

    PYTHONPATH=REFERENCE/src python3 scripts/lockstep_ref.py SCRIPT [SCRIPT ...] [--out-dir DIR]

With one script and no --out-dir the trace goes to stdout; otherwise each script's trace is
written to DIR/<name>.trace. Trace lines:

    r ADDR VAL | p PAIR VAL
    d ch1 ch2 ch3 ch4 | dacs | volumes | wave_digit wave_quarters | audible | channel_on | nr50 nr51
    state model t power registers wave_ram divider enabled lengths volumes digital
    take COUNT samples...
"""

from __future__ import annotations

import sys
from pathlib import Path

from gb_apu_python import APU, MODELS


def b(values) -> str:
    return "".join("1" if v else "0" for v in values)


def trace(lines: list[str]) -> list[str]:
    out: list[str] = []
    model = lines[0].split()[1]
    rate = int(lines[1].split()[1])
    apu = APU(sample_rate=rate, model=MODELS[model])
    for line in lines[2:]:
        op, *args = line.split()
        if op == "t":
            n, k = int(args[0]), int(args[1])
            for _ in range(k):
                apu.tick(n)
        elif op == "w":
            apu.write(int(args[0], 16), int(args[1], 16))
        elif op == "r":
            address = int(args[0], 16)
            out.append(f"r {address:04X} {apu.read(address):02X}")
        elif op == "d":
            out.append(
                "d {} | {} | {} | {} {} | {} | {} | {:02X} {:02X}".format(
                    " ".join(map(str, apu.digital())),
                    b(apu.dacs()),
                    " ".join(map(str, apu.volumes())),
                    apu.wave_digit(),
                    apu.wave_quarters(),
                    int(apu.audible()),
                    b(apu.channel_on),
                    apu.nr50,
                    apu.nr51,
                )
            )
        elif op == "f":
            apu.frame_tick()
        elif op == "s":
            apu.secondary_tick()
        elif op == "run":
            apu.run(int(args[0]))
        elif op == "h":
            apu.frame_clock_high = args[0] == "1"
        elif op == "p":
            pair = int(args[0])
            out.append(f"p {pair} {apu.pcm(pair):02X}")
        elif op == "state":
            s = apu.capture_state()
            out.append(
                "state {} {} {} {} {} {} {} {} {} {}".format(
                    s.model,
                    s.t,
                    int(s.power),
                    s.registers.hex(),
                    s.wave_ram.hex(),
                    s.divider,
                    b(s.enabled),
                    ",".join(map(str, s.lengths)),
                    ",".join(map(str, s.volumes)),
                    ",".join(map(str, s.digital)),
                )
            )
        elif op == "take":
            samples = apu.take_samples()
            out.append(f"take {len(samples)} " + " ".join(map(str, samples)))
        elif op == "boot":
            apu.skip_boot()
        elif op == "capture":
            apu.capture = args[0] == "1"
        else:
            raise ValueError(f"unknown op {op!r}")
    return out


def main() -> int:
    args = sys.argv[1:]
    out_dir = None
    if "--out-dir" in args:
        i = args.index("--out-dir")
        out_dir = Path(args[i + 1])
        del args[i : i + 2]
    for name in args:
        result = "\n".join(trace(Path(name).read_text().splitlines())) + "\n"
        if out_dir is None:
            sys.stdout.write(result)
        else:
            (out_dir / (Path(name).stem + ".trace")).write_text(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
