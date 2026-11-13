//! The Game Boy APU (the four channels, the frame sequencer and the mixer) behind a pin-shaped
//! boundary that any host drives (docs/contract.md):
//!
//! - the clock: `tick(t)` advances `t` T-cycles of the APU's 4,194,304 Hz clock (the Game Boy
//!   calls it every M-cycle; `t` is even), `run(t)` catches up any number of them in those
//!   steps;
//! - the frame sequencer: `frame_tick()` at each step (512 Hz) and `secondary_tick()` half a
//!   period later; the Game Boy takes both edges from its system counter's bit 12, a host without
//!   DIV clocks them itself. `frame_clock_high` is that clock's level, read when NR52 powers the
//!   APU on;
//! - registers by their Game Boy address: `read`/`write` of NR10-NR52 ($FF10-$FF26) and wave RAM
//!   ($FF30-$FF3F), the names below; a host maps its own addresses onto these;
//! - out: `digital()` (each channel's 0-15 DAC input), `dacs()`, `nr50()`/`nr51()` (panning and
//!   volume) for a host that mixes them itself, or `take_samples()` (this core's labelled-fit
//!   mix); `capture_state()` is an immutable snapshot.
//!
//! A `Model` row holds a revision's differences: DMG, CGB and AGB (the Game Boy Advance's four
//! Game Boy channels, in its own mode; docs/contract.md "The AGB row"). The behaviour follows Pan
//! Docs `Audio_details.md`, blargg `dmg_sound`/`cgb_sound` and SameSuite's APU ROMs (SameBoy's
//! model of the CGB); where a constant is fitted to a test it says so.
//!
//! A transcription of gb-apu-python (`src/gb_apu_python/__init__.py` at b8cab61): same names,
//! same order, same comments. Where Rust forced a difference it is said here:
//!
//! - Python's integers are unbounded; the channels' counters here are `i64`, which none of them
//!   approaches. Registers and wave RAM are bytes, as in Python's `bytearray`.
//! - The channels are three types, so the places where Python passes a channel object pass its
//!   index (0-3) and reach it through small accessors.
//! - `write` takes a `u8`: Python's `bytearray` raises on anything wider, so no host can pass
//!   more. Addresses below $FF10 or above $FF3F are outside the contract (Python indexes from the
//!   end of its array or raises); here they read $FF and writes are ignored.
//! - `read_masks()` is a method of the model's row rather than a stored reference.
//!
//! Save states (not in gb-apu-python, which has none): with the `serde` feature `APU` is
//! `Serialize`/`Deserialize`, every field included (a `Model` is saved by its name), so a host can
//! restore an APU exactly where it was. Without the feature the core imports nothing.

/// A hardware revision's APU: `cgb` selects the CGB-family rules (powering off clears the
/// length counters and keeps them read-only; wave RAM is reachable at any time while CH3 plays
/// and a trigger corrupts nothing; NRx2 writes while playing take the CGB "zombie" path; the CGB
/// sweep and noise counters; blargg cgb_sound 08-11, SameSuite).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    pub name: &'static str,
    pub cgb: bool,
    /// AGB: wave RAM is two banks of 32 digits held in a shift register (NR30 bit 6 picks the
    /// bank that plays, bit 5 chains both into 64 digits; the CPU reaches the bank that is not
    /// selected), and NR32 bit 7 forces the wave volume to 75 % (GBATEK "GBA Sound Channel 3")
    pub wave_banks: bool,
}

/// DMG-CPU (the revisions gameboy-python runs)
pub const DMG: Model = Model {
    name: "dmg",
    cgb: false,
    wave_banks: false,
};
/// CPU CGB E (gameboy-python's CGB)
pub const CGB: Model = Model {
    name: "cgb",
    cgb: true,
    wave_banks: false,
};
/// The Game Boy Advance's Game Boy channels, in GBA mode (gba-python): the CGB-family rules plus
/// the wave channel's two banks. The host clocks the frame sequencer from its own system clock.
pub const AGB: Model = Model {
    name: "agb",
    cgb: true,
    wave_banks: true,
};
/// The rows by name (Python's `MODELS` dict, in its order).
pub const MODELS: [(&str, Model); 3] = [("dmg", DMG), ("cgb", CGB), ("agb", AGB)];

/// `MODELS[name]`.
pub fn model(name: &str) -> Option<Model> {
    MODELS.iter().find(|(n, _)| *n == name).map(|&(_, m)| m)
}

// register addresses (the Game Boy's; a host maps its own onto them)
pub const NR10: u16 = 0xFF10;
pub const NR11: u16 = 0xFF11;
pub const NR12: u16 = 0xFF12;
pub const NR13: u16 = 0xFF13;
pub const NR14: u16 = 0xFF14;
pub const NR21: u16 = 0xFF16;
pub const NR22: u16 = 0xFF17;
pub const NR23: u16 = 0xFF18;
pub const NR24: u16 = 0xFF19;
pub const NR30: u16 = 0xFF1A;
pub const NR31: u16 = 0xFF1B;
pub const NR32: u16 = 0xFF1C;
pub const NR33: u16 = 0xFF1D;
pub const NR34: u16 = 0xFF1E;
pub const NR41: u16 = 0xFF20;
pub const NR42: u16 = 0xFF21;
pub const NR43: u16 = 0xFF22;
pub const NR44: u16 = 0xFF23;
pub const NR50: u16 = 0xFF24;
pub const NR51: u16 = 0xFF25;
pub const NR52: u16 = 0xFF26;
/// to $FF3F
pub const WAVE_RAM: u16 = 0xFF30;

/// bits that always read 1 per register $FF10-$FF26 (Pan Docs Audio_Registers.md, DMG)
#[rustfmt::skip]
pub const READ_MASKS: [u8; 0x17] = [
    0x80, 0x3F, 0x00, 0xFF, 0xBF,  // NR10-NR14
    0xFF, 0x3F, 0x00, 0xFF, 0xBF,  // $FF15 (unused), NR21-NR24
    0x7F, 0xFF, 0x9F, 0xFF, 0xBF,  // NR30-NR34
    0xFF, 0xFF, 0x00, 0x00, 0xBF,  // $FF1F (unused), NR41-NR44
    0x00, 0x00, 0x70,  // NR50, NR51, NR52
];

/// AGB: NR30's bank bits (6-5) and NR32's force bit (7) read back too
pub const READ_MASKS_AGB: [u8; 0x17] = {
    let mut masks = READ_MASKS;
    masks[0x0A] = 0x1F;
    masks[0x0B] = 0xFF;
    masks[0x0C] = 0x1F;
    masks
};
/// the wave channel's volume in quarters by NR32 bits 6-5; the AGB's force bit gives 3
pub const WAVE_QUARTERS: [i64; 4] = [0, 4, 2, 1];

/// NRx4 address -> channel index (the trigger bit is bit 7)
fn triggers(address: u16) -> usize {
    match address {
        0xFF14 => 0,
        0xFF19 => 1,
        0xFF1E => 2,
        _ => 3, // 0xFF23
    }
}

#[rustfmt::skip]
pub const DUTY: [[i64; 8]; 4] = [[0, 0, 0, 0, 0, 0, 0, 1], [1, 0, 0, 0, 0, 0, 0, 1], [1, 0, 0, 0, 0, 1, 1, 1],
        [0, 1, 1, 1, 1, 1, 1, 0]];
pub const NOISE_DIVISORS: [i64; 8] = [8, 16, 32, 48, 64, 80, 96, 112];
/// DMG wave RAM at power-on (Pan Docs; the board's skip-boot state)
pub const WAVE_POWER_ON: [u8; 16] = [
    0x84, 0x40, 0x43, 0xAA, 0x2D, 0x78, 0x92, 0x3C, 0x60, 0x59, 0x59, 0xB0, 0x34, 0xB8, 0x2E, 0xDA,
];

/// FITTED (blargg dmg_sound 09/12): T before the wave channel's first read
pub const WAVE_TRIGGER_DELAY: i64 = 6;
/// FITTED: the wave RAM is reachable for this many T after a channel read
pub const WAVE_WINDOW: i64 = 2;
/// FITTED (blargg dmg_sound 10): ages (T) at which a trigger corrupts
pub const CORRUPT_LO: i64 = 1;
pub const CORRUPT_HI: i64 = 3;
/// per T at 4194304 Hz (Pan Docs Audio_details.md, DMG)
pub const HPF_CHARGE: f64 = 0.999958;
pub const CLOCK_HZ: i64 = 4194304;

/// Volume envelope shared by the pulse and noise channels (NRx2): a 3-bit countdown that the
/// 64 Hz DIV-APU event decrements, a reload on the next (secondary) event after it reaches zero
/// that arms the clock, and the clock's tick on the event after that; the clock locks while the
/// volume cannot move. The model SameSuite's volume ROMs measure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Envelope {
    pub volume: i64,
    pub countdown: i64,
    pub clock: bool,
    pub should_lock: bool,
    pub locked: bool,
}

impl Envelope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trigger(&mut self, nrx2: u8) {
        let nrx2 = nrx2 as i64;
        self.locked = false;
        self.clock = false;
        self.countdown = nrx2 & 7;
        self.volume = nrx2 >> 4;
    }

    pub fn set_clock(&mut self, value: bool, up: bool, volume: i64) {
        if self.clock == value {
            return;
        }
        if value {
            self.clock = true;
            self.should_lock = (volume == 0xF && up) || (volume == 0 && !up);
        } else {
            self.clock = false;
            self.locked |= self.should_lock;
        }
    }

    /// The event that is the eighth since the last: the countdown runs unless a tick is
    /// armed.
    pub fn count(&mut self) {
        if !self.clock {
            self.countdown = (self.countdown - 1) & 7;
        }
    }

    /// The armed clock fires: the volume steps. Returns whether it did.
    pub fn tick(&mut self, nrx2: u8) -> bool {
        self.set_clock(false, false, 0);
        if self.locked || nrx2 & 7 == 0 {
            return false;
        }
        self.volume = (self.volume + (if nrx2 & 8 != 0 { 1 } else { -1 })) & 0xF;
        true
    }

    /// The event half a DIV-APU period after the tick: reload the countdown and arm the
    /// clock.
    pub fn secondary(&mut self, nrx2: u8) {
        if self.countdown == 0 {
            self.countdown = (nrx2 & 7) as i64;
            self.set_clock(self.countdown != 0, nrx2 & 8 != 0, self.volume);
        }
    }

    fn glitch(&mut self, value: i64, old: i64) {
        if self.clock {
            self.countdown = value & 7;
        }
        let mut should_tick = value & 7 != 0 && old & 7 == 0 && !self.locked;
        if value & 0xF == 8 && old & 0xF == 8 && !self.locked {
            should_tick = true;
        }
        if (value ^ old) & 8 != 0 {
            if value & 8 != 0 {
                if old & 7 == 0 && !self.locked {
                    self.volume ^= 0xF;
                } else {
                    self.volume = (0xE - self.volume) & 0xF;
                }
                should_tick = false;
            } else {
                self.volume = (0x10 - self.volume) & 0xF;
            }
        }
        if should_tick {
            self.volume = (self.volume + (if value & 8 != 0 { 1 } else { -1 })) & 0xF;
        } else if value & 7 == 0 && self.clock {
            self.set_clock(false, false, 0);
        }
    }

    /// NRx2 written while the channel plays (the "zombie mode" glitches).
    pub fn write(&mut self, old: u8, new: u8, cgb: bool) {
        let (old, new) = (old as i64, new as i64);
        if cgb {
            self.glitch(new, old);
        } else {
            self.glitch(0xFF, old);
            self.glitch(new, 0xFF);
        }
    }
}

/// A pulse channel clocked in 2 MHz ticks, the unit SameSuite's PCM12 reads resolve (SameBoy's
/// model of the CGB: a countdown per sample, a start-up `delay` and a suppressed first
/// sample).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pulse {
    pub has_sweep: bool,
    pub enabled: bool,
    pub length_enabled: bool,
    pub frequency: i64,
    pub env: Envelope,
    pub sweep_shadow: i64,
    pub sweep_timer: i64,
    pub sweep_enabled: bool,
    pub sweep_negated: bool,
    /// position in the duty pattern; only turning the APU off resets it
    pub index: i64,
    /// 2 MHz ticks to the next step
    pub countdown: i64,
    /// ticks of start-up left (the phantom step of CGB D/E)
    pub delay: i64,
    /// the first sample after a start is held
    pub suppressed: bool,
    pub just_reloaded: bool,
    pub did_tick: bool,
    /// the last digital value (what PCM12 shows while the channel is on)
    pub sample: i64,
    /// survives power-off on the DMG
    pub length: i64,
}

impl Pulse {
    pub fn new(sweep: bool) -> Self {
        let mut pulse = Pulse {
            has_sweep: sweep,
            enabled: false,
            length_enabled: false,
            frequency: 0,
            env: Envelope::new(),
            sweep_shadow: 0,
            sweep_timer: 0,
            sweep_enabled: false,
            sweep_negated: false,
            index: 0,
            countdown: 0,
            delay: 0,
            suppressed: false,
            just_reloaded: false,
            did_tick: false,
            sample: 0,
            length: 0,
        };
        pulse.reset();
        pulse.length = 0; // survives power-off on the DMG
        pulse
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length_enabled = false;
        self.frequency = 0;
        self.env = Envelope::new();
        self.sweep_shadow = 0;
        self.sweep_timer = 0;
        self.sweep_enabled = false;
        self.sweep_negated = false;
        self.index = 0; // position in the duty pattern; only turning the APU off resets it
        self.countdown = 0xFFFF; // 2 MHz ticks to the next step
        self.delay = 0; // ticks of start-up left (the phantom step of CGB D/E)
        self.suppressed = false; // the first sample after a start is held
        self.just_reloaded = false;
        self.did_tick = false;
        self.sample = 0; // the last digital value (what PCM12 shows while the channel is on)
    }

    pub fn output(&self) -> i64 {
        if self.enabled {
            self.sample
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Wave {
    pub enabled: bool,
    pub length_enabled: bool,
    pub frequency: i64,
    pub timer: i64,
    pub index: i64,
    pub buffer: i64,
    /// apu T of the last wave RAM read by the channel
    pub last_read: i64,
    pub length: i64,
}

impl Wave {
    pub fn new() -> Self {
        let mut wave = Wave {
            enabled: false,
            length_enabled: false,
            frequency: 0,
            timer: 0,
            index: 0,
            buffer: 0,
            last_read: 0,
            length: 0,
        };
        wave.reset();
        wave.length = 0;
        wave
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length_enabled = false;
        self.frequency = 0;
        self.timer = 0;
        self.index = 0;
        self.buffer = 0;
        self.last_read = -(1 << 30); // apu T of the last wave RAM read by the channel
    }
}

impl Default for Wave {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Noise {
    pub enabled: bool,
    pub length_enabled: bool,
    pub timer: i64,
    pub lfsr: i64,
    pub env: Envelope,
    /// CGB: the 14-bit counter whose bit NR43[7:4] clocks the LFSR, in 2 MHz ticks
    pub counter: i64,
    pub countdown: i64,
    /// 2 MHz ticks since power-on; its low bits set the start-up delay
    pub alignment: i64,
    pub counter_active: bool,
    /// the counter keeps running after the channel stops
    pub background_active: bool,
    pub started_with_dac_off: bool,
    pub did_step_counter: bool,
    pub reloaded: bool,
    pub length: i64,
}

impl Noise {
    pub fn new() -> Self {
        let mut noise = Noise {
            enabled: false,
            length_enabled: false,
            timer: 0,
            lfsr: 0,
            env: Envelope::new(),
            counter: 0,
            countdown: 0,
            alignment: 0,
            counter_active: false,
            background_active: false,
            started_with_dac_off: false,
            did_step_counter: false,
            reloaded: false,
            length: 0,
        };
        noise.reset();
        noise.length = 0;
        noise
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length_enabled = false;
        self.timer = 0;
        self.lfsr = 0;
        self.env = Envelope::new();
        // CGB: the 14-bit counter whose bit NR43[7:4] clocks the LFSR, in 2 MHz ticks
        self.counter = 0;
        self.countdown = 0;
        self.alignment = 0; // 2 MHz ticks since power-on; its low bits set the start-up delay
        self.counter_active = false;
        self.background_active = false; // the counter keeps running after the channel stops
        self.started_with_dac_off = false;
        self.did_step_counter = false;
        self.reloaded = false;
    }

    pub fn output(&self) -> i64 {
        if self.enabled && self.lfsr & 1 != 0 {
            self.env.volume
        } else {
            0
        }
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

/// An immutable snapshot, for comparison and reporting (to restore, serialise the `APU` itself:
/// the `serde` feature).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub struct APUState {
    pub model: &'static str,
    /// T-cycles since construction
    pub t: i64,
    pub power: bool,
    /// $FF10-$FF26 as written
    pub registers: [u8; 0x17],
    /// AGB: both banks (32 bytes) as the shift register holds them now
    pub wave_ram: Vec<u8>,
    /// frame sequencer steps counted (mod 256)
    pub divider: i64,
    pub enabled: [bool; 4],
    pub lengths: [i64; 4],
    /// CH1, CH2, CH4 envelopes
    pub volumes: [i64; 3],
    pub digital: [u8; 4],
}

#[allow(clippy::upper_case_acronyms)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct APU {
    pub model: Model,
    pub cgb: bool,
    pub wave_banks: bool,
    /// $FF10-$FF26 as written
    pub registers: [u8; 0x17],
    pub wave_ram: [u8; 16],
    /// AGB: the shift register, one digit per entry: bank 0 then bank 1, each in play order.
    /// Playing rotates the selected bank (both, in 64-digit mode), so digits move.
    #[cfg_attr(feature = "serde", serde(with = "bytes64"))]
    pub wave_digits: [u8; 64],
    pub power: bool,
    pub ch1: Pulse,
    pub ch2: Pulse,
    pub ch3: Wave,
    pub ch4: Noise,
    /// DIV-APU events counted (mod 256); its low bits pick what each one clocks
    pub divider: i64,
    /// 1: the first event after a power-on with the clock high is dropped, 2: and the next does
    /// not count
    pub skip_event: i64,
    // CGB sweep: a 3-bit period counter stepped by the DIV-APU events, and the calculation it
    // starts, which finishes a few 1 MHz ticks later (SameSuite channel_1_sweep*)
    pub sweep_countdown: i64,
    pub calc_countdown: i64,
    pub calc_reload: i64,
    pub calc_instant: bool,
    pub sweep_unshifted: bool,
    pub sweep_addend: i64,
    pub sweep_shadow: i64,
    pub sweep_completed: i64,
    pub restart_hold: i64,
    /// the 2 MHz tick's phase against the 1 MHz the pulse channels start on
    pub lf_div: i64,
    /// T since reset
    pub t: i64,
    /// an odd T run() has not passed on yet
    held_t: i64,
    /// set in `_write_nrx4` before a trigger reads it (Python sets the attribute there)
    old_frequency: i64,
    // output
    pub sample_rate: u32,
    pub capture: bool,
    pending: Vec<i16>,
    acc_l: f64,
    acc_r: f64,
    acc_t: i64,
    capacitor: [f64; 2],
    per_sample: f64,
    charge: f64,
    owed: f64,
    /// set by the host: the frame sequencer clock's level (DIV bit 12)
    pub frame_clock_high: bool,
}

/// `Model` in a save state: its name, looked up in `MODELS` on load.
#[cfg(feature = "serde")]
impl serde::Serialize for Model {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Model {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        model(&name).ok_or_else(|| serde::de::Error::custom(format!("unknown model {name}")))
    }
}

/// serde derives arrays up to 32 long; the AGB shift register is 64.
#[cfg(feature = "serde")]
mod bytes64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("wave_digits: 64 bytes"))
    }
}

/// C integer division by two (towards zero).
fn half(value: i64) -> i64 {
    if value >= 0 {
        value / 2
    } else {
        -((-value) / 2)
    }
}

impl APU {
    /// The bits of each register that read as 1 (the AGB's NR30/NR32 differ).
    fn read_masks(&self) -> &'static [u8; 0x17] {
        if self.wave_banks {
            &READ_MASKS_AGB
        } else {
            &READ_MASKS
        }
    }

    pub fn new(sample_rate: u32, model: Model) -> Self {
        APU {
            model,
            cgb: model.cgb,
            wave_banks: model.wave_banks,
            registers: [0; 0x17],
            wave_ram: WAVE_POWER_ON,
            wave_digits: [0; 64],
            power: false,
            ch1: Pulse::new(true),
            ch2: Pulse::new(false),
            ch3: Wave::new(),
            ch4: Noise::new(),
            divider: 0,
            skip_event: 0,
            sweep_countdown: 0,
            calc_countdown: 0,
            calc_reload: 0,
            calc_instant: false,
            sweep_unshifted: false,
            sweep_addend: 0,
            sweep_shadow: 0,
            sweep_completed: 0,
            restart_hold: 0,
            lf_div: 1,
            t: 0,
            held_t: 0,
            old_frequency: 0,
            sample_rate,
            capture: false,
            pending: Vec::new(),
            acc_l: 0.0,
            acc_r: 0.0,
            acc_t: 0,
            capacitor: [0.0, 0.0],
            per_sample: CLOCK_HZ as f64 / sample_rate as f64,
            charge: HPF_CHARGE.powf(CLOCK_HZ as f64 / sample_rate as f64),
            owed: 0.0,
            frame_clock_high: false,
        }
    }

    // ----- the channels by index (Python passes the channel objects) -------------------

    fn pulse_mut(&mut self, index: usize) -> &mut Pulse {
        if index == 0 {
            &mut self.ch1
        } else {
            &mut self.ch2
        }
    }

    fn enabled(&self, index: usize) -> bool {
        match index {
            0 => self.ch1.enabled,
            1 => self.ch2.enabled,
            2 => self.ch3.enabled,
            _ => self.ch4.enabled,
        }
    }

    fn set_enabled(&mut self, index: usize, value: bool) {
        match index {
            0 => self.ch1.enabled = value,
            1 => self.ch2.enabled = value,
            2 => self.ch3.enabled = value,
            _ => self.ch4.enabled = value,
        }
    }

    fn length_mut(&mut self, index: usize) -> &mut i64 {
        match index {
            0 => &mut self.ch1.length,
            1 => &mut self.ch2.length,
            2 => &mut self.ch3.length,
            _ => &mut self.ch4.length,
        }
    }

    fn length_enabled_mut(&mut self, index: usize) -> &mut bool {
        match index {
            0 => &mut self.ch1.length_enabled,
            1 => &mut self.ch2.length_enabled,
            2 => &mut self.ch3.length_enabled,
            _ => &mut self.ch4.length_enabled,
        }
    }

    /// The channels with an envelope (1, 2, 4) and their NRx2 index.
    fn env_mut(&mut self, index: usize) -> &mut Envelope {
        match index {
            0 => &mut self.ch1.env,
            1 => &mut self.ch2.env,
            _ => &mut self.ch4.env,
        }
    }

    // ----- state the rest of the board reads ----------------------------------------------

    pub fn channel_on(&self) -> [bool; 4] {
        [
            self.ch1.enabled,
            self.ch2.enabled,
            self.ch3.enabled,
            self.ch4.enabled,
        ]
    }

    /// A channel's DAC is on when NRx2's top five bits (NR30 bit 7 for CH3) are not zero.
    pub fn dac_on(&self, channel: usize) -> bool {
        if channel == 2 {
            return self.registers[0x0A] & 0x80 != 0;
        }
        self.registers[[0x02, 0x07, 0, 0x11][channel]] & 0xF8 != 0
    }

    // ----- registers ----------------------------------------------------------------------

    pub fn read(&self, address: u16) -> u8 {
        if (0xFF30..0xFF40).contains(&address) {
            return self.read_wave((address - 0xFF30) as usize);
        }
        if !(0xFF10..0xFF27).contains(&address) {
            return 0xFF; // $FF27-$FF2F; and outside the contract (see the module header)
        }
        if address == 0xFF26 {
            let status: u8 = self
                .channel_on()
                .iter()
                .enumerate()
                .map(|(i, &on)| if on { 1 << i } else { 0 })
                .sum();
            return 0x70 | (if self.power { 0x80 } else { 0 }) | status;
        }
        let index = (address - 0xFF10) as usize;
        self.registers[index] | self.read_masks()[index]
    }

    pub fn write(&mut self, address: u16, value: u8) {
        if (0xFF30..0xFF40).contains(&address) {
            self.write_wave((address - 0xFF30) as usize, value);
            return;
        }
        if !(0xFF10..0xFF27).contains(&address) {
            return;
        }
        if address == 0xFF26 {
            self.write_power(value & 0x80 != 0);
            return;
        }
        if !self.power {
            // DMG: the length counters stay writable (their duty bits do not)
            if !self.cgb && matches!(address, 0xFF11 | 0xFF16 | 0xFF1B | 0xFF20) {
                self.write_length(address, value);
            }
            return;
        }
        self.write_register(address, value);
    }

    fn write_power(&mut self, on: bool) {
        if self.power && !on {
            self.registers = [0; 0x17];
            let cgb = self.cgb;
            self.ch1.reset();
            self.ch2.reset();
            self.ch3.reset();
            self.ch4.reset();
            if cgb {
                for index in 0..4 {
                    *self.length_mut(index) = 0;
                }
            }
            self.divider = 0;
            self.skip_event = 0;
            self.sweep_countdown = 0;
            self.calc_countdown = 0;
            self.calc_reload = 0;
            self.calc_instant = false;
            self.sweep_unshifted = false;
            self.sweep_addend = 0;
            self.sweep_shadow = 0;
            self.sweep_completed = 0;
            self.restart_hold = 0;
        } else if on && !self.power {
            if self.frame_clock_high {
                // the sequencer clock is high already: a step is spent
                self.divider = 1;
                self.skip_event = 1;
            }
            self.lf_div = 1;
        }
        self.power = on;
    }

    fn write_length(&mut self, address: u16, value: u8) {
        let value = value as i64;
        match address {
            0xFF11 => self.ch1.length = 64 - (value & 0x3F),
            0xFF16 => self.ch2.length = 64 - (value & 0x3F),
            0xFF1B => self.ch3.length = 256 - value,
            _ => self.ch4.length = 64 - (value & 0x3F),
        }
    }

    fn write_register(&mut self, address: u16, value: u8) {
        let index = (address - 0xFF10) as usize;
        let old = self.registers[index];
        self.registers[index] = value;
        if address == 0xFF10 && self.cgb {
            self.write_nr10(old, value);
        } else if address == 0xFF10 {
            if self.ch1.sweep_negated && old & 8 != 0 && value & 8 == 0 {
                self.ch1.enabled = false; // sweep direction cleared after a negated calculation
            }
        } else if matches!(address, 0xFF11 | 0xFF16 | 0xFF1B | 0xFF20) {
            self.write_length(address, value);
        } else if matches!(address, 0xFF12 | 0xFF17 | 0xFF21) {
            let channel = match address {
                0xFF12 => 0,
                0xFF17 => 1,
                _ => 3,
            };
            if value & 0xF8 == 0 {
                if channel == 3 && self.cgb {
                    let ch4 = &mut self.ch4;
                    if ch4.enabled && self.registers[0x12] & 7 != 0 {
                        if ch4.countdown <= 2 {
                            ch4.counter = (ch4.counter + 1) & 0x3FFF;
                        }
                        ch4.background_active = false;
                    }
                    ch4.counter_active = false;
                }
                self.set_enabled(channel, false); // DAC off
            } else if self.enabled(channel) {
                let cgb = self.cgb;
                self.env_mut(channel).write(old, value, cgb);
                if channel != 3 {
                    self.refresh(channel);
                }
            }
        } else if address == 0xFF22 && self.cgb {
            self.write_nr43(old, value);
        } else if matches!(address, 0xFF13 | 0xFF18) {
            let channel = self.pulse_mut(if address == 0xFF13 { 0 } else { 1 });
            channel.frequency = (channel.frequency & 0x700) | value as i64;
            if channel.just_reloaded {
                channel.countdown = (channel.frequency ^ 0x7FF) * 2 + 1;
            }
        } else if address == 0xFF1D {
            self.ch3.frequency = (self.ch3.frequency & 0x700) | value as i64;
        } else if address == 0xFF1A {
            if value & 0x80 == 0 {
                self.ch3.enabled = false;
            }
        } else if matches!(address, 0xFF14 | 0xFF19 | 0xFF1E | 0xFF23) {
            self.write_nrx4(address, value, old);
        }
    }

    fn write_nrx4(&mut self, address: u16, value: u8, old: u8) {
        let index = triggers(address);
        self.old_frequency = match index {
            0 => self.ch1.frequency,
            1 => self.ch2.frequency,
            2 => self.ch3.frequency,
            _ => 0, // the noise channel has no frequency
        };
        if index < 2 {
            self.pulse_frequency_high(index, value, old);
        } else if address != 0xFF23 {
            self.ch3.frequency = (self.ch3.frequency & 0xFF) | (((value & 7) as i64) << 8);
        }
        let was_enabled = *self.length_enabled_mut(index);
        *self.length_enabled_mut(index) = value & 0x40 != 0;
        // extra length clock: length enabled in the half period the next step does not clock
        if value & 0x40 != 0
            && !was_enabled
            && self.divider & 1 != 0
            && *self.length_mut(index) != 0
        {
            *self.length_mut(index) -= 1;
            if *self.length_mut(index) == 0 && value & 0x80 == 0 {
                self.set_enabled(index, false);
            }
        }
        if value & 0x80 != 0 {
            self.trigger(index);
        }
    }

    fn trigger(&mut self, index: usize) {
        let dac = self.dac_on(index);
        if index == 2 {
            self.trigger_wave_corruption();
        }
        if *self.length_mut(index) == 0 {
            *self.length_mut(index) = if index == 2 { 256 } else { 64 };
            if *self.length_enabled_mut(index) && self.divider & 1 != 0 {
                *self.length_mut(index) -= 1;
            }
        }
        if index < 2 {
            let nrx2 = self.registers[if index == 0 { 0x02 } else { 0x07 }];
            self.pulse_mut(index).env.trigger(nrx2);
            let was_active = self.enabled(index);
            self.pulse_start(index, dac);
            if index == 0 {
                if self.cgb {
                    self.sweep_restart(was_active);
                } else {
                    self.trigger_sweep();
                }
            }
            return;
        }
        if index == 2 {
            let channel = &mut self.ch3;
            channel.enabled = dac;
            channel.timer = (2048 - channel.frequency) * 2 + WAVE_TRIGGER_DELAY;
            channel.index = 0;
        } else {
            let nr43 = self.registers[0x12];
            let nr42 = self.registers[0x11];
            self.ch4.env.trigger(nr42);
            if self.cgb {
                self.noise_start(); // reads whether the channel was running
                self.ch4.enabled = dac;
            } else {
                self.ch4.enabled = dac;
                self.ch4.lfsr = 0;
                self.ch4.timer = Self::noise_period(nr43);
            }
        }
    }

    // ----- the sweep on the CGB ------------------------------------------------------------

    /// A sweep calculation finishes: the overflow check uses the addend twice.
    fn sweep_done(&mut self) {
        let nr10 = self.registers[0];
        if self.restart_hold == 0 {
            self.sweep_shadow = self.ch1.frequency;
        }
        if nr10 & 8 != 0 {
            self.sweep_addend ^= 0x7FF;
        }
        if self.sweep_shadow + self.sweep_addend > 0x7FF && nr10 & 8 == 0 {
            self.ch1.enabled = false;
        }
        self.sweep_completed = self.sweep_addend;
    }

    fn sweep_calculation(&mut self) {
        let nr10 = self.registers[0] as i64;
        if nr10 & 0x70 != 0 && self.sweep_countdown == 7 {
            if nr10 & 7 != 0 {
                self.ch1.frequency =
                    (self.sweep_addend + self.sweep_shadow + (if nr10 & 8 != 0 { 1 } else { 0 }))
                        & 0x7FF;
            }
            if self.restart_hold == 0 {
                self.sweep_addend = self.ch1.frequency >> (nr10 & 7);
            }
            self.calc_countdown = nr10 & 7; // recalculation and the overflow check come later
            self.calc_reload = 1 + self.lf_div;
            self.sweep_unshifted = nr10 & 7 == 0;
            self.sweep_countdown = ((nr10 >> 4) & 7) ^ 7;
            if self.calc_countdown == 0 {
                self.calc_instant = true;
            }
        }
    }

    fn sweep_restart(&mut self, was_active: bool) {
        let nr10 = self.registers[0] as i64;
        self.calc_instant = false;
        self.sweep_shadow = 0;
        self.sweep_completed = 0;
        if nr10 & 7 != 0 {
            // the overflow check also happens on trigger
            self.calc_countdown = nr10 & 7;
            self.calc_reload = 2 + (if was_active { 0 } else { 1 });
            self.sweep_unshifted = false;
            self.sweep_addend = self.ch1.frequency >> (nr10 & 7);
        } else {
            self.sweep_addend = 0;
        }
        self.restart_hold = 4 - self.lf_div;
        self.sweep_countdown = ((nr10 >> 4) & 7) ^ 7;
    }

    fn write_nr10(&mut self, old: u8, value: u8) {
        let (old, value) = (old as i64, value as i64);
        if self.calc_countdown != 0 || self.calc_reload != 0 {
            if self.calc_reload == 2 {
                // the countdown just reloaded: reload it again
                self.calc_countdown = value & 7;
                if self.calc_countdown == 0 {
                    self.calc_reload = 0;
                }
            }
            if value & 7 != 0 && old & 7 == 0 && self.lf_div == 0 && self.calc_countdown > 1 {
                self.calc_countdown -= 1;
                if self.calc_countdown == 0 {
                    self.sweep_done();
                }
            }
        }
        if self.sweep_shadow + self.sweep_completed + (if old & 8 != 0 { 1 } else { 0 }) > 0x7FF
            && value & 8 == 0
        {
            self.ch1.enabled = false;
        }
        self.sweep_calculation();
    }

    fn run_sweep(&mut self, cycles: i64) {
        let mut ticks = cycles / 2;
        if cycles & 1 != 0 && self.lf_div == 0 {
            ticks += 1;
        }
        if self.calc_reload > ticks {
            self.calc_reload -= ticks;
            ticks = 0;
        } else {
            if self.calc_reload != 0 && self.calc_countdown == 0 && self.calc_instant {
                self.sweep_done();
            }
            self.calc_instant = false;
            ticks -= self.calc_reload;
            self.calc_reload = 0;
        }
        if self.calc_countdown != 0 && (self.registers[0] & 7 != 0 || self.sweep_unshifted) {
            if self.calc_countdown > ticks {
                self.calc_countdown -= ticks;
            } else {
                self.calc_countdown = 0;
                self.sweep_done();
            }
        }
        if self.restart_hold != 0 {
            self.restart_hold = if self.restart_hold <= cycles {
                0
            } else {
                self.restart_hold - cycles
            };
        }
    }

    // ----- the pulse channels in 2 MHz ticks ----------------------------------------------

    /// The digital value for the current duty step and volume (not while the first sample
    /// after a start is held).
    fn refresh(&mut self, index: usize) {
        let duty = (self.registers[if index == 0 { 0x01 } else { 0x06 }] >> 6) as usize;
        let channel = self.pulse_mut(index);
        if channel.suppressed {
            return;
        }
        channel.sample = DUTY[duty][channel.index as usize] * channel.env.volume;
    }

    /// NRx4's frequency bits and what a change of them does to a running channel.
    fn pulse_frequency_high(&mut self, index: usize, value: u8, old: u8) {
        let cgb = self.cgb;
        let channel = self.pulse_mut(index);
        if value & 0x80 == 0
            && channel.enabled
            && old & 7 == 7
            && value & 7 != 7
            && (cgb || channel.countdown & 1 != 0)
            && channel.did_tick
            && channel.countdown >> 1 == channel.frequency ^ 0x7FF
        {
            // the length just changed from >= $700 to < $700: undo the step it took too early
            channel.index = (channel.index - 1) & 7;
            channel.suppressed = false;
        }
        channel.frequency = (channel.frequency & 0xFF) | (((value & 7) as i64) << 8);
        if channel.just_reloaded {
            channel.countdown = (channel.frequency ^ 0x7FF) * 2 + 1;
        }
    }

    /// A trigger: the countdown and start-up delay depend on whether the channel was running
    /// and on the tick phase (`lf_div`).
    fn pulse_start(&mut self, index: usize, dac: bool) {
        let value = self.registers[if index == 0 { 0x04 } else { 0x09 }];
        let (cgb, lf_div, old_frequency) = (self.cgb, self.lf_div, self.old_frequency);
        let channel = self.pulse_mut(index);
        let was_active = channel.enabled;
        channel.did_tick = false;
        let mut unsuppress = false;
        if !was_active {
            if cgb && value & 4 == 0 && half(channel.countdown - channel.delay) & 0x400 == 0 {
                channel.index = (channel.index + 1) & 7; // CGB D/E: a phantom step
                unsuppress = true;
            }
            channel.delay = 6 - lf_div;
        } else {
            let mut extra = 0;
            if cgb {
                if !channel.just_reloaded
                    && value & 4 == 0
                    && half(channel.countdown - 1 - channel.delay) & 0x400 == 0
                {
                    channel.index = (channel.index + 1) & 7;
                    channel.suppressed = false;
                } else if channel.frequency == 0x7FF && old_frequency != 0x7FF && channel.suppressed
                {
                    extra = 2;
                }
            }
            channel.delay = 4 - lf_div + extra;
        }
        channel.countdown = ((channel.frequency ^ 0x7FF) * 2 + channel.delay) & 0xFFFF;
        if was_active {
            self.refresh(index); // the new volume takes effect at once, the new start does not
        }
        let channel = self.pulse_mut(index);
        if dac && !was_active {
            channel.enabled = true;
            channel.sample = 0;
            channel.suppressed = !unsuppress;
        } else if !dac {
            channel.enabled = false;
        }
    }

    fn run_pulse(&mut self, index: usize, cycles: i64) {
        if !self.pulse_mut(index).enabled {
            return;
        }
        let mut left = cycles;
        {
            let channel = self.pulse_mut(index);
            if channel.delay != 0 {
                channel.delay = if channel.delay < left {
                    0
                } else {
                    channel.delay - left
                };
            }
        }
        while left > self.pulse_mut(index).countdown {
            let channel = self.pulse_mut(index);
            left -= channel.countdown + 1;
            channel.countdown = (channel.frequency ^ 0x7FF) * 2 + 1;
            channel.index = (channel.index + 1) & 7;
            channel.suppressed = false;
            channel.did_tick = true;
            self.refresh(index);
        }
        let channel = self.pulse_mut(index);
        channel.just_reloaded = left == 0;
        if left != 0 {
            channel.countdown -= left;
        }
    }

    // ----- the noise channel in 2 MHz ticks (CGB) ------------------------------------------

    fn step_lfsr(&mut self) {
        let mask = if self.registers[0x12] & 8 != 0 {
            0x4040
        } else {
            0x4000
        };
        let ch4 = &mut self.ch4;
        let mut lfsr = ch4.lfsr;
        let bit = ((lfsr ^ (lfsr >> 1)) & 1) ^ 1; // XNOR of bits 0 and 1
        lfsr >>= 1;
        ch4.lfsr = if bit != 0 { lfsr | mask } else { lfsr & !mask };
    }

    /// A trigger: the counter's countdown to its next step depends on the divisor, on the
    /// 2 MHz tick's alignment against the start, and on whether the counter was running
    /// (SameSuite channel_4_align, _delay, _frequency_alignment measure these).
    fn noise_start(&mut self) {
        let nr43 = self.registers[0x12] as i64;
        let nr42 = self.registers[0x11];
        let ch4 = &mut self.ch4;
        ch4.counter_active = nr42 & 0xF8 != 0;
        let was_dac_off = ch4.started_with_dac_off;
        ch4.started_with_dac_off = !ch4.counter_active;
        let mut divisor = nr43 & 7;
        let was_background = ch4.background_active;
        ch4.background_active = true;
        let align = ch4.alignment & 3;
        let mut instant_step = false;
        let mut div_1_glitch = false;
        if divisor > 1 && ch4.countdown == 1 {
            ch4.counter = (ch4.counter + 1) & 0x3FFF;
        } else if ch4.countdown == 2 && align == 0 && ch4.enabled {
            if divisor == 0 {
                divisor = 8;
            } else if divisor == 1 {
                if !ch4.did_step_counter {
                    div_1_glitch = true;
                }
                let mask = 1 << (nr43 >> 4);
                let old_bit = ch4.counter & mask;
                ch4.counter = (ch4.counter + 1) & 0x3FFF;
                instant_step = ch4.counter & mask != 0 && old_bit == 0;
            }
        }
        let mut countdown = if divisor == 0 { 6 } else { divisor * 4 + 6 };
        if ch4.alignment & 1 != 0 {
            if divisor == 0 {
                countdown += if was_background { -1 } else { 1 };
            } else if ch4.alignment & 2 != 0 {
                countdown += if divisor == 1 && !ch4.enabled { 1 } else { -3 };
            } else {
                countdown -= 1;
                if divisor == 1 && ch4.enabled {
                    countdown -= 4;
                }
            }
        } else if divisor != 0 {
            if ch4.alignment & 2 != 0 {
                countdown -= 2;
            } else if divisor > 1 || (divisor == 1 && ch4.enabled && nr43 & 0xF0 == 0) {
                countdown -= 4;
            }
        }
        if divisor > 1 {
            if !ch4.counter_active && align == 0 {
                countdown += 4;
            }
        } else if was_background && !ch4.enabled && align == 0 {
            if divisor == 0 {
                if was_dac_off {
                    countdown += 28;
                }
            } else {
                countdown -= 4;
            }
        }
        if div_1_glitch {
            countdown -= 4;
        }
        ch4.countdown = countdown;
        ch4.lfsr = if divisor == 0 && ch4.enabled && align == 3 {
            0x55
        } else {
            0
        };
        ch4.did_step_counter = align == 2;
        if instant_step {
            self.step_lfsr();
        }
    }

    fn run_noise(&mut self, cycles: i64) {
        self.ch4.alignment += cycles;
        if !(self.ch4.counter_active || self.ch4.background_active) {
            return;
        }
        let nr43 = self.registers[0x12] as i64;
        let divisor = match (nr43 & 7) << 2 {
            0 => 2,
            d => d,
        };
        let mut left = cycles;
        if self.ch4.countdown == 0 {
            self.ch4.countdown = divisor;
        }
        while left >= self.ch4.countdown {
            let ch4 = &mut self.ch4;
            left -= ch4.countdown;
            ch4.countdown = divisor;
            let mask = 1 << (nr43 >> 4);
            let old_bit = ch4.counter & mask;
            ch4.counter = (ch4.counter + 1) & 0x3FFF;
            ch4.did_step_counter = true;
            if ch4.counter & mask != 0 && old_bit == 0 && ch4.enabled {
                self.step_lfsr();
            }
        }
        let ch4 = &mut self.ch4;
        if left != 0 {
            ch4.countdown -= left;
            ch4.reloaded = false;
        } else {
            ch4.reloaded = true;
        }
    }

    /// NR43 written: a changed clock-select bit glitches the LFSR (the plain, deterministic
    /// variants of what the CGB E does; the revision- and instance-specific ones are not here).
    fn write_nr43(&mut self, old: u8, new: u8) {
        let (old, new) = (old as i64, new as i64);
        let ch4 = &mut self.ch4;
        if ch4.reloaded {
            let divisor = match (new & 7) << 2 {
                0 => 2,
                d => d,
            };
            ch4.countdown = divisor
                + (if divisor == 2 {
                    0
                } else {
                    [2, 1, 0, 3][(ch4.alignment & 3) as usize]
                });
        }
        if old & 0xF0 == new & 0xF0 {
            return;
        }
        let counter = ch4.counter;
        let old_bit = (counter >> (old >> 4)) & 1;
        let glitch_bit = (counter >> (((old & 0x7F) | (new & 0x80)) >> 4)) & 1;
        let new_bit = (counter >> (new >> 4)) & 1;
        if old_bit == new_bit && new_bit != glitch_bit {
            if new_bit == 0 || new & 0x80 == 0 {
                self.step_lfsr();
            }
        } else if old_bit == 0 && new_bit != 0 {
            self.step_lfsr();
        }
    }

    fn noise_period(nr43: u8) -> i64 {
        let shift = (nr43 >> 4) as i64;
        if shift < 14 {
            NOISE_DIVISORS[(nr43 & 7) as usize] << shift
        } else {
            1 << 30
        }
    }

    // ----- the sweep ----------------------------------------------------------------------

    fn sweep_calc(&mut self) -> i64 {
        let nr10 = self.registers[0] as i64;
        let delta = self.ch1.sweep_shadow >> (nr10 & 7);
        if nr10 & 8 != 0 {
            self.ch1.sweep_negated = true;
            return self.ch1.sweep_shadow - delta;
        }
        self.ch1.sweep_shadow + delta
    }

    fn trigger_sweep(&mut self) {
        let nr10 = self.registers[0] as i64;
        let ch1 = &mut self.ch1;
        ch1.sweep_shadow = ch1.frequency;
        ch1.sweep_timer = match (nr10 >> 4) & 7 {
            0 => 8,
            p => p,
        };
        ch1.sweep_enabled = nr10 & 0x77 != 0;
        ch1.sweep_negated = false;
        if nr10 & 7 != 0 && self.sweep_calc() > 2047 {
            self.ch1.enabled = false;
        }
    }

    fn clock_sweep(&mut self) {
        self.ch1.sweep_timer -= 1;
        if self.ch1.sweep_timer > 0 {
            return;
        }
        let period = ((self.registers[0] >> 4) & 7) as i64;
        self.ch1.sweep_timer = if period != 0 { period } else { 8 };
        if !(self.ch1.sweep_enabled && period != 0) {
            return;
        }
        let new = self.sweep_calc();
        if new > 2047 {
            self.ch1.enabled = false;
        } else if self.registers[0] & 7 != 0 {
            self.ch1.sweep_shadow = new;
            self.ch1.frequency = new;
            self.registers[0x03] = (new & 0xFF) as u8;
            self.registers[0x04] = (self.registers[0x04] & !7) | (new >> 8) as u8;
            if self.sweep_calc() > 2047 {
                self.ch1.enabled = false;
            }
        }
    }

    // ----- wave RAM -----------------------------------------------------------------------

    fn wave_reachable(&self) -> bool {
        self.t - self.ch3.last_read < WAVE_WINDOW
    }

    /// AGB: where the bank the CPU reaches starts in the shift register: the one NR30 bit 6
    /// does not select, playing or not (GBATEK; with the APU off NR30 is 0, so bank 1).
    fn other_bank(&self) -> usize {
        if self.registers[0x0A] & 0x40 != 0 {
            0
        } else {
            32
        }
    }

    fn read_wave(&self, offset: usize) -> u8 {
        if self.wave_banks {
            let at = self.other_bank() + 2 * offset;
            return (self.wave_digits[at] << 4) | self.wave_digits[at + 1];
        }
        let ch3 = &self.ch3;
        if !(ch3.enabled && self.power) {
            return self.wave_ram[offset];
        }
        if self.cgb || self.wave_reachable() {
            return self.wave_ram[(ch3.index >> 1) as usize];
        }
        0xFF
    }

    fn write_wave(&mut self, offset: usize, value: u8) {
        if self.wave_banks {
            let at = self.other_bank() + 2 * offset;
            self.wave_digits[at] = value >> 4;
            self.wave_digits[at + 1] = value & 0xF;
            return;
        }
        if !(self.ch3.enabled && self.power) {
            self.wave_ram[offset] = value;
        } else if self.cgb || self.wave_reachable() {
            self.wave_ram[(self.ch3.index >> 1) as usize] = value;
        }
    }

    /// DMG: triggering CH3 while it reads wave RAM rewrites the first bytes (Pan Docs).
    fn trigger_wave_corruption(&mut self) {
        let ch3 = &self.ch3;
        let age = self.t - ch3.last_read;
        if self.cgb || !(ch3.enabled && (CORRUPT_LO..CORRUPT_HI).contains(&age)) {
            return;
        }
        let position = (((ch3.index + 1) & 31) >> 1) as usize;
        if position < 4 {
            self.wave_ram[0] = self.wave_ram[position];
        } else {
            let start = position & !3;
            self.wave_ram.copy_within(start..start + 4, 0);
        }
    }

    // ----- time ---------------------------------------------------------------------------

    /// A falling edge of DIV's bit 12 (DIV-APU): the next frame sequencer step.
    pub fn frame_tick(&mut self) {
        if !self.power {
            return;
        }
        if self.skip_event == 1 {
            self.skip_event = 2;
            return;
        }
        if self.skip_event == 2 {
            self.skip_event = 0;
        } else {
            self.divider = (self.divider + 1) & 0xFF;
        }
        let divider = self.divider;
        for (channel, nrx2) in Self::ENVELOPES {
            let value = self.registers[nrx2];
            let env = self.env_mut(channel);
            if divider & 7 == 7 {
                env.count();
            }
            let ticked = env.clock && env.tick(value);
            if ticked && self.enabled(channel) && channel != 3 {
                self.refresh(channel);
            }
        }
        if divider & 1 != 0 {
            self.clock_lengths();
        }
        if divider & 3 == 3 {
            if self.cgb {
                self.sweep_countdown = (self.sweep_countdown + 1) & 7;
                self.sweep_calculation();
            } else {
                self.clock_sweep();
            }
        }
    }

    /// The channels with an envelope and their NRx2 register (Python's `_envelopes()`).
    const ENVELOPES: [(usize, usize); 3] = [(0, 0x02), (1, 0x07), (3, 0x11)];

    /// A rising edge of DIV's bit 12: half a period after the falling one.
    pub fn secondary_tick(&mut self) {
        if !self.power {
            return;
        }
        for (channel, nrx2) in Self::ENVELOPES {
            if self.enabled(channel) {
                let value = self.registers[nrx2];
                self.env_mut(channel).secondary(value);
            }
        }
    }

    fn clock_lengths(&mut self) {
        for channel in 0..4 {
            if *self.length_enabled_mut(channel) && *self.length_mut(channel) != 0 {
                *self.length_mut(channel) -= 1;
                if *self.length_mut(channel) == 0 {
                    self.set_enabled(channel, false);
                }
            }
        }
    }

    /// Catch up `t` T-cycles in the Game Boy's steps: 4 at a time (one M-cycle), the rest
    /// last. Sequencer edges and register accesses go between calls. The channels count 2 MHz
    /// ticks, two T each: an odd T is held and joins the next call, so a host whose clock is
    /// not the Game Boy's loses no time.
    pub fn run(&mut self, t: u64) {
        let mut t = t as i64 + self.held_t;
        self.held_t = t & 1;
        t -= self.held_t;
        while t >= 4 {
            self.tick(4);
            t -= 4;
        }
        if t != 0 {
            self.tick(t as u32);
        }
    }

    /// Advance `t` T-cycles (the Game Boy passes 4, one M-cycle; 2 in CGB double speed).
    pub fn tick(&mut self, t: u32) {
        let t = t as i64;
        let base = self.t;
        self.t = base + t;
        if self.power {
            let cycles = t >> 1;
            self.lf_div ^= cycles & 1;
            if self.cgb {
                self.run_sweep(cycles);
            }
            self.run_pulse(0, cycles);
            self.run_pulse(1, cycles);
            self.tick_wave(base, t);
            if self.cgb {
                self.run_noise(cycles);
            } else {
                self.tick_noise(t);
            }
        }
        if self.capture {
            self.sample(t);
        }
    }

    fn tick_wave(&mut self, base: i64, t: i64) {
        if !self.ch3.enabled {
            return;
        }
        self.ch3.timer -= t;
        if self.wave_banks {
            let period = (2048 - self.ch3.frequency) * 2;
            while self.ch3.timer <= 0 {
                self.ch3.timer += period;
                self.shift_wave();
            }
            return;
        }
        let ch3 = &mut self.ch3;
        while ch3.timer <= 0 {
            let at = base + t + ch3.timer; // when inside this tick the period ran out
            ch3.timer += (2048 - ch3.frequency) * 2;
            ch3.index = (ch3.index + 1) & 31;
            let byte = self.wave_ram[(ch3.index >> 1) as usize] as i64;
            ch3.buffer = if ch3.index & 1 == 0 {
                byte >> 4
            } else {
                byte & 0xF
            };
            ch3.last_read = at;
        }
    }

    /// AGB: the shift register moves one digit and the digit that leaves its end is played
    /// (and re-enters at the other end). One bank is a ring of 32; in 64-digit mode both are
    /// one ring that starts with the selected bank (GBATEK).
    fn shift_wave(&mut self) {
        let digits = &mut self.wave_digits;
        let nr30 = self.registers[0x0A];
        let start = if nr30 & 0x40 != 0 { 32 } else { 0 };
        let out = digits[start];
        if nr30 & 0x20 != 0 {
            let other = start ^ 32;
            digits.copy_within(start + 1..start + 32, start);
            digits[start + 31] = digits[other];
            digits.copy_within(other + 1..other + 32, other);
            digits[other + 31] = out;
        } else {
            digits.copy_within(start + 1..start + 32, start);
            digits[start + 31] = out;
        }
        self.ch3.buffer = out as i64;
    }

    fn tick_noise(&mut self, t: i64) {
        if !self.ch4.enabled {
            return;
        }
        let nr43 = self.registers[0x12];
        let ch4 = &mut self.ch4;
        ch4.timer -= t;
        while ch4.timer <= 0 {
            ch4.timer += Self::noise_period(nr43);
            let mut lfsr = ch4.lfsr;
            let bit = ((lfsr ^ (lfsr >> 1)) & 1) ^ 1; // XNOR of bits 0 and 1
            lfsr = (lfsr >> 1) | (bit << 14);
            if nr43 & 8 != 0 {
                lfsr = (lfsr & !0x40) | (bit << 6);
            }
            ch4.lfsr = lfsr;
        }
    }

    // ----- output -------------------------------------------------------------------------

    pub fn wave_output(&self) -> i64 {
        if !self.ch3.enabled {
            return 0;
        }
        (self.ch3.buffer * self.wave_quarters()) >> 2
    }

    /// The wave channel's volume in quarters (NR32): 0, 4 (100 %), 2 or 1; on the AGB bit 7
    /// forces 3 (75 %).
    pub fn wave_quarters(&self) -> i64 {
        let nr32 = self.registers[0x0C];
        if self.wave_banks && nr32 & 0x80 != 0 {
            return 3;
        }
        WAVE_QUARTERS[((nr32 >> 5) & 3) as usize]
    }

    /// The wave channel's current digit (0-15) before its volume; 0 while it is off. With
    /// `wave_quarters()`, what a host needs to mix the AGB's 75 % exactly.
    pub fn wave_digit(&self) -> i64 {
        if self.ch3.enabled {
            self.ch3.buffer
        } else {
            0
        }
    }

    /// The envelope volumes (0-15) of channels 1, 2 and 4: a host that mixes around zero (the
    /// AGB's digital mixer) needs them beside `digital()`.
    pub fn volumes(&self) -> [i64; 3] {
        [
            self.ch1.env.volume,
            self.ch2.env.volume,
            self.ch4.env.volume,
        ]
    }

    /// Whether any channel routed to an output (NR51) is producing a non-zero digital value
    /// through an enabled DAC: Gambatte's "outaudio" criterion (no DC, no filter).
    pub fn audible(&self) -> bool {
        let nr51 = self.registers[0x15];
        self.power
            && self
                .digital()
                .iter()
                .enumerate()
                .any(|(i, &value)| value != 0 && self.dac_on(i) && nr51 & (0x11 << i) != 0)
    }

    /// The four channels' digital outputs (0-15), the DACs' inputs.
    pub fn digital(&self) -> [u8; 4] {
        [
            self.ch1.output() as u8,
            self.ch2.output() as u8,
            self.wave_output() as u8,
            self.ch4.output() as u8,
        ]
    }

    /// Which channels' DACs are on (an off DAC outputs nothing at all).
    pub fn dacs(&self) -> [bool; 4] {
        [
            self.dac_on(0),
            self.dac_on(1),
            self.dac_on(2),
            self.dac_on(3),
        ]
    }

    /// Master volume (bits 6-4 left, 2-0 right) and the VIN bits, as written.
    pub fn nr50(&self) -> u8 {
        self.registers[0x14]
    }

    /// Panning: bit 4 + n sends channel n + 1 left, bit n sends it right.
    pub fn nr51(&self) -> u8 {
        self.registers[0x15]
    }

    pub fn capture_state(&self) -> APUState {
        APUState {
            model: self.model.name,
            t: self.t,
            power: self.power,
            registers: self.registers,
            wave_ram: self.wave_bytes(),
            divider: self.divider,
            enabled: [
                self.ch1.enabled,
                self.ch2.enabled,
                self.ch3.enabled,
                self.ch4.enabled,
            ],
            lengths: [
                self.ch1.length,
                self.ch2.length,
                self.ch3.length,
                self.ch4.length,
            ],
            volumes: [
                self.ch1.env.volume,
                self.ch2.env.volume,
                self.ch4.env.volume,
            ],
            digital: self.digital(),
        }
    }

    fn wave_bytes(&self) -> Vec<u8> {
        if !self.wave_banks {
            return self.wave_ram.to_vec();
        }
        let digits = &self.wave_digits;
        (0..64)
            .step_by(2)
            .map(|i| (digits[i] << 4) | digits[i + 1])
            .collect()
    }

    /// Analogue left and right, -1..1 before the volume: DAC maps digital 0 to +1 and 15 to
    /// -1, an off DAC to 0; NR51 pans; NR50 scales by (volume + 1) / 8. LABELLED FIT: the host
    /// output is this sum divided by 4, high-passed; the real DMG is analogue.
    fn mix(&self) -> (f64, f64) {
        let outputs = self.digital();
        let (nr50, nr51) = (self.registers[0x14], self.registers[0x15]);
        let mut left = 0.0;
        let mut right = 0.0;
        for (i, &digital) in outputs.iter().enumerate() {
            if !self.dac_on(i) {
                continue;
            }
            let analog = (7.5 - digital as f64) / 7.5;
            if nr51 & (0x10 << i) != 0 {
                left += analog;
            }
            if nr51 & (1 << i) != 0 {
                right += analog;
            }
        }
        (
            left * ((((nr50 >> 4) & 7) + 1) as f64) / 8.0 / 4.0,
            right * (((nr50 & 7) + 1) as f64) / 8.0 / 4.0,
        )
    }

    fn sample(&mut self, t: i64) {
        let (left, right) = self.mix();
        self.acc_l += left * t as f64;
        self.acc_r += right * t as f64;
        self.acc_t += t;
        self.owed += t as f64;
        while self.owed >= self.per_sample {
            self.owed -= self.per_sample;
            let span = if self.acc_t != 0 { self.acc_t } else { 1 } as f64;
            let any_dac = (0..4).any(|i| self.dac_on(i)) && self.power;
            for (k, value) in [self.acc_l / span, self.acc_r / span]
                .into_iter()
                .enumerate()
            {
                let filtered = if any_dac {
                    let filtered = value - self.capacitor[k];
                    self.capacitor[k] = value - filtered * self.charge;
                    filtered
                } else {
                    0.0
                };
                // Python's int() truncates towards zero, as `as` does
                self.pending
                    .push(((filtered * 32767.0) as i64).clamp(-32768, 32767) as i16);
            }
            self.acc_l = 0.0;
            self.acc_r = 0.0;
            self.acc_t = 0;
        }
    }

    /// Interleaved signed 16-bit stereo since the last call (capture must be on).
    pub fn take_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.pending)
    }

    // ----- boot hand-off ------------------------------------------------------------------

    /// PCM12/PCM34 ($FF76/$FF77, CGB): the channels' current 4-bit digital outputs, channel
    /// 1 or 3 in the low nibble and 2 or 4 in the high one (Pan Docs Audio_Registers.md).
    pub fn pcm(&self, pair: usize) -> u8 {
        let outputs = if self.power {
            self.digital()
        } else {
            [0, 0, 0, 0]
        };
        outputs[2 * pair] | (outputs[2 * pair + 1] << 4)
    }

    /// The state the DMG boot ROM leaves: powered, the registers it writes, channel 1 on.
    pub fn skip_boot(&mut self) {
        self.power = true;
        self.divider = 0;
        #[rustfmt::skip]
        let writes: [(usize, u8); 20] = [
            (0x10, 0x80), (0x11, 0xBF), (0x12, 0xF3), (0x13, 0xFF), (0x14, 0xBF),
            (0x16, 0x3F), (0x17, 0x00), (0x18, 0xFF), (0x19, 0xBF),
            (0x1A, 0x7F), (0x1B, 0xFF), (0x1C, 0x9F), (0x1D, 0xFF), (0x1E, 0xBF),
            (0x20, 0xFF), (0x21, 0x00), (0x22, 0x00), (0x23, 0xBF),
            (0x24, 0x77), (0x25, 0xF3),
        ];
        for (address, value) in writes {
            self.registers[address - 0x10] = value;
        }
        self.ch1.enabled = true; // NR52 reads $F1 at $0100
        self.ch1.frequency = 0x7FF;
        self.ch1.countdown = 1;
        self.ch1.env.volume = 0;
        self.ch1.length = 64;
    }
}

impl Default for APU {
    /// `APU()`: 44,100 Hz, the DMG row.
    fn default() -> Self {
        APU::new(44100, DMG)
    }
}
