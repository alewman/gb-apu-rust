//! Run lockstep scripts (scripts/lockstep_gen.py) on this core and print the trace that
//! scripts/lockstep_ref.py prints for gb-apu-python (rung 2).
//!
//!     gb-apu-trace SCRIPT [SCRIPT ...] [--out-dir DIR]
//!
//! With one script and no --out-dir the trace goes to stdout; otherwise each script's trace is
//! written to DIR/<name>.trace.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use gb_apu::{model, APU};

fn b(values: &[bool]) -> String {
    values.iter().map(|&v| if v { '1' } else { '0' }).collect()
}

fn join<T: ToString>(values: &[T], sep: &str) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

fn trace(text: &str) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = text.lines().collect();
    let name = lines[0].split_whitespace().nth(1).unwrap();
    let rate: u32 = lines[1].split_whitespace().nth(1).unwrap().parse().unwrap();
    let mut apu = APU::new(rate, model(name).unwrap());
    for line in &lines[2..] {
        let mut parts = line.split_whitespace();
        let op = parts.next().unwrap();
        let args: Vec<&str> = parts.collect();
        let dec = |i: usize| -> u64 { args[i].parse().unwrap() };
        let hx = |i: usize| -> u16 { u16::from_str_radix(args[i], 16).unwrap() };
        match op {
            "t" => {
                let (n, k) = (dec(0) as u32, dec(1));
                for _ in 0..k {
                    apu.tick(n);
                }
            }
            "w" => apu.write(hx(0), hx(1) as u8),
            "r" => {
                let address = hx(0);
                writeln!(out, "r {address:04X} {:02X}", apu.read(address)).unwrap();
            }
            "d" => {
                writeln!(
                    out,
                    "d {} | {} | {} | {} {} | {} | {} | {:02X} {:02X}",
                    join(&apu.digital(), " "),
                    b(&apu.dacs()),
                    join(&apu.volumes(), " "),
                    apu.wave_digit(),
                    apu.wave_quarters(),
                    apu.audible() as u8,
                    b(&apu.channel_on()),
                    apu.nr50(),
                    apu.nr51()
                )
                .unwrap();
            }
            "f" => apu.frame_tick(),
            "s" => apu.secondary_tick(),
            "run" => apu.run(dec(0)),
            "h" => apu.frame_clock_high = args[0] == "1",
            "p" => {
                let pair = dec(0) as usize;
                writeln!(out, "p {pair} {:02X}", apu.pcm(pair)).unwrap();
            }
            "state" => {
                let s = apu.capture_state();
                writeln!(
                    out,
                    "state {} {} {} {} {} {} {} {} {} {}",
                    s.model,
                    s.t,
                    s.power as u8,
                    hex(&s.registers),
                    hex(&s.wave_ram),
                    s.divider,
                    b(&s.enabled),
                    join(&s.lengths, ","),
                    join(&s.volumes, ","),
                    join(&s.digital, ",")
                )
                .unwrap();
            }
            "take" => {
                let samples = apu.take_samples();
                write!(out, "take {} ", samples.len()).unwrap();
                out.push_str(&join(&samples, " "));
                out.push('\n');
            }
            "boot" => apu.skip_boot(),
            "capture" => apu.capture = args[0] == "1",
            _ => panic!("unknown op {op:?}"),
        }
    }
    out
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut out_dir: Option<PathBuf> = None;
    if let Some(i) = args.iter().position(|a| a == "--out-dir") {
        out_dir = Some(PathBuf::from(&args[i + 1]));
        args.drain(i..i + 2);
    }
    for name in &args {
        let result = trace(&fs::read_to_string(name).unwrap());
        match &out_dir {
            None => print!("{result}"),
            Some(dir) => {
                let stem = Path::new(name).file_stem().unwrap().to_string_lossy();
                fs::write(dir.join(format!("{stem}.trace")), result).unwrap();
            }
        }
    }
}
