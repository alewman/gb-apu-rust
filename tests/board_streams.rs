//! Rung 3: replay the calls gameboy-python's board made into its APU while it passed (or failed)
//! the APU's test ROMs (gb-apu-python's `scripts/export_board_streams.py`; the 94 streams are
//! copied from gb-apu-python's `tests/streams/` at b8cab61). Every read, every PCM read and the
//! digital outputs after every frame sequencer step must come out as the board saw them, and the
//! captured 44.1 kHz samples must hash the same: gb-apu-python's `tests/test_board_streams.py`.
//!
//! `GB_APU_STREAMS=N` replays only the first N streams (sorted by name), as in Python.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use gb_apu::{model, APU};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn streams() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/streams");
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("tests/streams")
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".json.gz"))
        .collect();
    paths.sort();
    if let Ok(n) = std::env::var("GB_APU_STREAMS") {
        paths.truncate(n.parse().unwrap());
    }
    paths
}

fn load(path: &Path) -> Value {
    let mut text = String::new();
    GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Run a stream; the first mismatch, or None.
fn replay(stream: &Value) -> Option<String> {
    let header = &stream["header"];
    let mut apu = APU::new(44100, model(header["model"].as_str().unwrap()).unwrap());
    apu.skip_boot();
    apu.capture = true;
    let n = |v: &Value| v.as_u64().unwrap();
    for (i, event) in stream["events"].as_array().unwrap().iter().enumerate() {
        let event = event.as_array().unwrap();
        match event[0].as_str().unwrap() {
            "t" => {
                let t = n(&event[1]) as u32;
                for _ in 0..n(&event[2]) {
                    apu.tick(t);
                }
            }
            "w" => {
                apu.frame_clock_high = n(&event[3]) != 0;
                apu.write(n(&event[1]) as u16, n(&event[2]) as u8);
            }
            "r" => {
                let (address, want) = (n(&event[1]) as u16, n(&event[2]));
                let got = apu.read(address);
                if got as u64 != want {
                    return Some(format!(
                        "event {i}: read ${address:04X} gave ${got:02X}, want ${want:02X}"
                    ));
                }
            }
            "p" => {
                let (pair, want) = (n(&event[1]) as usize, n(&event[2]));
                let got = apu.pcm(pair);
                if got as u64 != want {
                    return Some(format!(
                        "event {i}: PCM{pair} gave ${got:02X}, want ${want:02X}"
                    ));
                }
            }
            "f" => apu.frame_tick(),
            "s" => apu.secondary_tick(),
            "c" => {
                let want: Vec<u64> = event[1..].iter().map(n).collect();
                let got: Vec<u64> = apu.digital().iter().map(|&d| d as u64).collect();
                if got != want {
                    return Some(format!("event {i}: digital {got:?}, want {want:?}"));
                }
            }
            other => return Some(format!("event {i}: unknown kind {other}")),
        }
    }
    let bytes: Vec<u8> = apu
        .take_samples()
        .iter()
        .flat_map(|s| s.to_le_bytes())
        .collect();
    let samples = format!("{:x}", Sha256::digest(bytes));
    if samples != header["samples_sha256"].as_str().unwrap() {
        return Some("the samples differ".into());
    }
    None
}

#[test]
fn every_board_stream() {
    let paths = streams();
    assert!(!paths.is_empty(), "no streams");
    let mut failed = Vec::new();
    for path in &paths {
        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".json.gz", "");
        match replay(&load(path)) {
            None => println!("ok   {name}"),
            Some(why) => {
                println!("FAIL {name}: {why}");
                failed.push(name);
            }
        }
    }
    println!(
        "{} of {} board streams identical",
        paths.len() - failed.len(),
        paths.len()
    );
    assert!(
        failed.is_empty(),
        "{} streams differ: {failed:?}",
        failed.len()
    );
}
