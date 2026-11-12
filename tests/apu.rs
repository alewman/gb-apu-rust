//! The APU on its own: the host seams (clock, sequencer, registers, outputs, state) and the model
//! rows. gb-apu-python's `tests/test_apu.py` at b8cab61, test for test. The behaviour against
//! hardware is judged through the board's streams (board_streams.rs).

use gb_apu::*;

fn square(apu: &mut APU) {
    apu.write(NR52, 0x80); // power
    apu.write(NR50, 0x77);
    apu.write(NR51, 0xFF);
    apu.write(NR11, 0x80); // duty 50%
    apu.write(NR12, 0xF0); // volume 15, no envelope
    apu.write(NR13, 0x00);
    apu.write(NR14, 0x87); // trigger, frequency 0x700: about 2 kHz
}

#[test]
fn a_pulse_channel_makes_a_square_wave() {
    let mut apu = APU::new(44100, DMG);
    apu.capture = true;
    square(&mut apu);
    for _ in 0..100_000 {
        // 0.095 s of the 4 MHz clock
        apu.tick(4);
    }
    let samples = apu.take_samples();
    assert!(samples.len() > 2 * 4000); // about 4,200 stereo frames
    assert!(*samples.iter().max().unwrap() > 2000 && *samples.iter().min().unwrap() < -2000);
}

#[test]
fn power_off_clears_registers_but_not_length_counters() {
    let mut apu = APU::default();
    apu.write(NR52, 0x80);
    apu.write(NR12, 0xF0);
    apu.write(NR11, 0x3F);
    apu.write(NR52, 0x00);
    assert_eq!(apu.read(NR12), 0x00);
    assert_eq!(apu.ch1.length, 1); // DMG: the length counter survives
    apu.write(NR12, 0xF0); // ignored while off
    assert_eq!(apu.read(NR12), 0x00);
}

#[test]
fn cgb_power_off_clears_the_length_counters() {
    let mut apu = APU::new(44100, CGB);
    apu.write(NR52, 0x80);
    apu.write(NR11, 0x3F);
    apu.write(NR52, 0x00);
    assert_eq!(apu.ch1.length, 0);
}

#[test]
fn the_model_rows() {
    assert_eq!(MODELS, [("dmg", DMG), ("cgb", CGB), ("agb", AGB)]);
    assert!(!DMG.cgb && CGB.cgb);
    assert_eq!(APU::default().model, DMG);
}

/// run(t) catches up in the Game Boy's 4-T steps, so a host with another master clock gets the
/// same result as the Game Boy's per-M-cycle calls.
#[test]
fn run_is_the_board_steps() {
    for model in [DMG, CGB] {
        let mut a = APU::new(44100, model);
        let mut b = APU::new(44100, model);
        for apu in [&mut a, &mut b] {
            apu.capture = true;
            square(apu);
        }
        for _ in 0..1000 {
            a.tick(4);
        }
        a.tick(2);
        b.run(4002);
        assert_eq!(a.capture_state(), b.capture_state());
        assert_eq!(a.take_samples(), b.take_samples());
    }
}

/// The frame sequencer only moves when the host says so: no clock of its own.
#[test]
fn the_sequencer_is_the_hosts() {
    let mut apu = APU::default();
    apu.write(NR52, 0x80);
    apu.write(NR11, 0x3E); // length 2
    apu.write(NR12, 0xF0);
    apu.write(NR14, 0xC0); // trigger with the length enabled
    apu.run(1 << 20);
    assert!(apu.digital()[0] != 0 || apu.channel_on()[0]); // no sequencer step yet: still playing
    for _ in 0..4 {
        apu.frame_tick();
    }
    assert!(!apu.channel_on()[0]); // two length clocks (every other step) ran it out
}

/// Powering on while the sequencer clock is high spends its next step (SameSuite
/// div_write_trigger): the host tells the APU the level before the NR52 write.
#[test]
fn the_clock_level_at_power_on_spends_the_first_step() {
    let mut low = APU::default();
    let mut high = APU::default();
    high.frame_clock_high = true;
    for apu in [&mut low, &mut high] {
        apu.write(NR52, 0x80);
        apu.frame_tick();
    }
    assert_eq!(low.capture_state().divider, 1);
    assert_eq!(high.capture_state().divider, 1); // the spent step set it, the real one did not count
    for apu in [&mut low, &mut high] {
        apu.frame_tick();
    }
    assert_eq!(
        (low.capture_state().divider, high.capture_state().divider),
        (2, 1)
    );
}

#[test]
fn outputs_for_a_host_that_mixes() {
    let mut apu = APU::default();
    square(&mut apu);
    apu.run(4096);
    assert_eq!(apu.dacs(), [true, false, false, false]);
    assert_eq!((apu.nr50(), apu.nr51()), (0x77, 0xFF));
    assert!(apu.digital().iter().all(|&v| v <= 15));
}

#[test]
fn wave_ram_by_address() {
    let mut apu = APU::new(44100, CGB);
    apu.write(NR52, 0x80);
    for i in 0..16u16 {
        apu.write(WAVE_RAM + i, (i * 0x11) as u8);
    }
    let got: Vec<u8> = (0..16).map(|i| apu.read(WAVE_RAM + i)).collect();
    assert_eq!(got, (0..16u8).map(|i| i * 0x11).collect::<Vec<_>>());
}

#[test]
fn state_is_a_snapshot() {
    let mut apu = APU::default();
    square(&mut apu);
    let state = apu.capture_state();
    assert!(state.model == "dmg" && state.power && state.enabled[0]);
    assert_eq!(state.registers[(NR12 - 0xFF10) as usize], 0xF0);
    apu.write(NR52, 0x00);
    assert!(state.power); // unchanged by later writes
    assert_eq!(state.wave_ram.len(), 16);
    assert_eq!(state.digital, [0, 0, 0, 0]); // the first sample after a start is held
}

/// NR43 bit 3 narrows the LFSR to 7 bits: the sequence repeats every 127 steps, not 32,767
/// (Pan Docs Audio_details.md). No judge ROM listens to the narrow mode, so this rule is tested
/// here.
#[test]
fn noise_lfsr_periods() {
    for model in [DMG, CGB] {
        for (nr43, period) in [(0x00u8, 32767usize), (0x08, 127)] {
            let mut apu = APU::new(44100, model);
            apu.write(NR52, 0x80);
            apu.write(NR42, 0xF0);
            apu.write(NR43, nr43); // the fastest clock
            apu.write(NR44, 0x80);
            let mut states = Vec::new();
            let mut last = apu.ch4.lfsr;
            for _ in 0..64 * (2 * period + 200) {
                // a stuck LFSR fails below instead of hanging
                apu.tick(2);
                if apu.ch4.lfsr != last {
                    last = apu.ch4.lfsr;
                    states.push(last & 0x7F);
                    if states.len() == 2 * period + 200 {
                        break;
                    }
                }
            }
            assert_eq!(states.len(), 2 * period + 200, "the LFSR stopped stepping");
            let tail = &states[200..];
            assert!((0..period).all(|i| tail[i] == tail[i + period]));
            if period == 127 {
                assert!((0..period).any(|i| tail[i] != tail[i + 63]));
            }
        }
    }
}

// ----- the AGB row: wave RAM as two banks in a shift register ---------------------------------

/// digits 0,1,0,2,...: every byte differs
const BANK_A: [u8; 16] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
];
const BANK_B: [u8; 16] = [
    0xF0, 0xEF, 0xEE, 0xED, 0xEC, 0xEB, 0xEA, 0xE9, 0xE8, 0xE7, 0xE6, 0xE5, 0xE4, 0xE3, 0xE2, 0xE1,
];

/// An AGB APU with BANK_A in bank 0 and BANK_B in bank 1, then `nr30` and a trigger at the
/// fastest rate (one digit per 2 T).
fn agb_wave(nr30: u8, nr32: u8) -> APU {
    let mut apu = APU::new(44100, AGB);
    apu.write(NR52, 0x80);
    apu.write(NR30, 0x40); // bank 1 selected: the CPU reaches bank 0
    for (i, &value) in BANK_A.iter().enumerate() {
        apu.write(WAVE_RAM + i as u16, value);
    }
    apu.write(NR30, 0x00);
    for (i, &value) in BANK_B.iter().enumerate() {
        apu.write(WAVE_RAM + i as u16, value);
    }
    apu.write(NR30, nr30);
    apu.write(NR32, nr32);
    apu.write(NR33, 0xFF);
    apu.write(NR34, 0x87);
    apu
}

/// The next `count` digits the wave channel plays.
fn played(apu: &mut APU, count: usize) -> Vec<i64> {
    let mut digits = Vec::new();
    apu.tick(8); // the start-up delay and one period: the first digit
    for _ in 0..count {
        digits.push(apu.wave_digit());
        apu.tick(2);
    }
    digits
}

fn digits_of(data: &[u8]) -> Vec<i64> {
    data.iter()
        .flat_map(|&b| [(b >> 4) as i64, (b & 0xF) as i64])
        .collect()
}

fn cat(a: &[u8], b: &[u8]) -> Vec<u8> {
    [a, b].concat()
}

fn bank(apu: &APU) -> Vec<u8> {
    (0..16).map(|i| apu.read(WAVE_RAM + i)).collect()
}

#[test]
fn agb_is_a_cgb_family_row_with_wave_banks() {
    assert_eq!(model("agb"), Some(AGB));
    assert!(AGB.cgb && AGB.wave_banks && !CGB.wave_banks && !DMG.wave_banks);
}

#[test]
fn agb_cpu_reaches_the_bank_that_is_not_selected() {
    let mut apu = agb_wave(0x00, 0x20); // bank 0 selected
    apu.write(NR30, 0x40);
    assert_eq!(bank(&apu), BANK_A);
    let mut apu = agb_wave(0x40, 0x20);
    apu.write(NR30, 0x00);
    assert_eq!(bank(&apu), BANK_B);
}

#[test]
fn agb_plays_the_selected_bank_from_its_first_digit() {
    assert_eq!(played(&mut agb_wave(0x80, 0x20), 32), digits_of(&BANK_A));
    assert_eq!(played(&mut agb_wave(0xC0, 0x20), 32), digits_of(&BANK_B));
    // one bank repeats
    assert_eq!(
        played(&mut agb_wave(0x80, 0x20), 64),
        digits_of(&cat(&BANK_A, &BANK_A))
    );
}

#[test]
fn agb_64_digit_mode_starts_with_the_selected_bank_then_the_other() {
    assert_eq!(
        played(&mut agb_wave(0xA0, 0x20), 64),
        digits_of(&cat(&BANK_A, &BANK_B))
    );
    let ba = cat(&BANK_B, &BANK_A);
    assert_eq!(
        played(&mut agb_wave(0xE0, 0x20), 128),
        digits_of(&cat(&ba, &ba))
    );
}

#[test]
fn agb_wave_ram_is_a_shift_register_the_cpu_sees_move() {
    let mut apu = agb_wave(0xA0, 0x20); // 64 digits from bank 0: the CPU reaches bank 1
    apu.tick(8 + 2); // two digits played: one byte has left the front and gone to the back
    assert_eq!(apu.read(WAVE_RAM), BANK_B[1]);
    assert_eq!(apu.read(WAVE_RAM + 15), BANK_A[0]);
    let want = [&BANK_A[1..], &BANK_B[..], &BANK_A[..1]].concat();
    assert_eq!(apu.capture_state().wave_ram, want);
    let mut apu = agb_wave(0x80, 0x20); // one bank: stopping part way leaves it turned
    apu.tick(8 + 2 * 3); // four digits
    apu.write(NR30, 0x40);
    assert_eq!(bank(&apu), cat(&BANK_A[2..], &BANK_A[..2]));
}

#[test]
fn agb_force_bit_is_75_percent_and_reads_back() {
    let mut apu = agb_wave(0x80, 0x80);
    assert_eq!(apu.wave_quarters(), 3);
    assert_eq!(apu.read(NR32), 0x9F);
    apu.tick(8 + 2 * 29); // the 30th digit: the 15 of byte 0Fh
    assert_eq!(apu.wave_digit(), 15);
    assert_eq!(apu.digital()[2], 11); // 15 x 3/4, rounded down
    let quarters: Vec<i64> = (0..4)
        .map(|n| agb_wave(0x80, n << 5).wave_quarters())
        .collect();
    assert_eq!(quarters, [0, 4, 2, 1]);
    let mut cgb = APU::new(44100, CGB);
    cgb.write(NR52, 0x80);
    cgb.write(NR32, 0x80);
    assert!(cgb.wave_quarters() == 0 && cgb.read(NR32) == 0x9F | 0x80);
}

#[test]
fn agb_nr30_reads_its_bank_bits() {
    let mut apu = APU::new(44100, AGB);
    apu.write(NR52, 0x80);
    apu.write(NR30, 0xE0);
    assert_eq!(apu.read(NR30), 0xFF);
    apu.write(NR30, 0x00);
    assert_eq!(apu.read(NR30), 0x1F);
}

#[test]
fn volumes_are_the_envelopes() {
    let mut apu = APU::new(44100, AGB);
    apu.write(NR52, 0x80);
    apu.write(NR12, 0xA0);
    apu.write(NR14, 0x80);
    assert_eq!(apu.volumes(), [10, 0, 0]);
}

/// A host on another clock catches up by odd amounts: the same T must give the same wave.
#[test]
fn run_loses_no_time_whatever_the_step() {
    let mut whole = APU::new(44100, AGB);
    let mut pieces = APU::new(44100, AGB);
    for apu in [&mut whole, &mut pieces] {
        square(apu);
    }
    whole.run(3000);
    for _ in 0..120 {
        for step in [1, 3, 5, 7, 9] {
            // 3,000 T in odd steps
            pieces.run(step);
        }
    }
    assert_eq!(pieces.ch1.index, whole.ch1.index);
    assert_eq!(pieces.ch1.countdown, whole.ch1.countdown);
}
