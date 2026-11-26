// SPDX-License-Identifier: MIT
//! The AGB row's wave channel: the Game Boy Advance's two wave RAM banks.
//!
//! A transcription of gb-apu-python's `src/gb_apu_python/agb.py` (branch `cleanroom-agb-wave`),
//! function for function, with its comments. That module is a clean-room rewrite (2026-11-25,
//! gb-apu-python docs/provenance.md "Clean-room rewrite of the AGB row"): written from GBATEK
//! alone (Martin Korth, extracted from no$gba 3.06; sha256 of gbatek.txt
//! ba503864b2201973db4fbeb4d98b4394b19417820a6e6b9f6014fa73ce52845c), with no other emulator's
//! source and without the code it replaces. This transcription was likewise written from the
//! Python module alone, without reading the code it replaces here. Each rule names its GBATEK
//! section; where GBATEK is silent the rule says so and is marked HYPOTHESIS.
//!
//! The sections cited:
//!
//! - [CH3] "GBA Sound Channel 3 - Wave Output": 4000070h SOUND3CNT_L (NR30), 4000072h
//!   SOUND3CNT_H (NR31, NR32) and 4000090h..400009Eh WAVE_RAM.
//! - [CMP] "GBA Comparison of CGB and GBA Sound", "Sound Channel 3 Changes".
//! - [CTL] "GBA Sound Control Registers", 4000084h SOUNDCNT_X (NR52).
//!
//! The Game Boy register addresses are this core's (a host maps the GBA's onto them): NR30 is
//! $FF1A, NR32 is $FF1C, wave RAM is $FF30-$FF3F.
//!
//! Where Rust forced a difference: Python's bank is a `bytearray` it pops from the front of and
//! appends to; here it is a fixed array turned with `rotate_left(1)`, the same ring.

/// NR30 bit 5: "Wave RAM Dimension (0=One bank/32 digits, 1=Two banks/64 digits)" (DOC [CH3])
pub const NR30_DIMENSION: u8 = 0x20;
/// NR30 bit 6: "Wave RAM Bank Number (0-1)" (DOC [CH3])
pub const NR30_BANK: u8 = 0x40;
/// NR32 bit 7 (SOUND3CNT_H bit 15): "Force Volume (0=Use above, 1=Force 75% regardless of
/// above)" (DOC [CH3]); 75 % is three quarters, the unit WAVE_QUARTERS counts in
pub const NR32_FORCE: u8 = 0x80;
pub const FORCED_QUARTERS: i64 = 3;

/// digits in a bank: "two Wave Patterns exists (each 32 x 4bits)" (DOC [CH3], WAVE_RAM)
pub const BANK_DIGITS: usize = 32;

/// The bits of $FF10-$FF26 that read as 1 on the AGB: the CGB's, except where GBATEK marks a
/// bit R/W that the CGB does not.
///
/// - NR30: bits 0-4 "Not used", bits 5-7 R/W (DOC [CH3], SOUND3CNT_L): mask $1F.
/// - NR31: bits 0-7 "W" (DOC [CH3], SOUND3CNT_H): GBATEK does not say what a write-only bit
///   reads as, so the CGB's $FF is kept (HYPOTHESIS: unchanged from the CGB row).
/// - NR32: bits 8-12 of SOUND3CNT_H "Not used", 13-14 and 15 R/W (DOC [CH3]): in the NR32 byte,
///   bits 0-4 unused and 5-7 readable: mask $1F.
///
/// Everything else as the CGB (DOC [CMP]: "mostly the same than that of older monochrome gameboy
/// and CGB"; the changes it lists for channel 3 are NR30 bits 5-6 and NR32 bit 7).
pub const fn agb_read_masks(cgb_masks: [u8; 0x17]) -> [u8; 0x17] {
    let mut masks = cgb_masks;
    masks[0x0A] = 0x1F; // NR30
    masks[0x0B] = 0xFF; // NR31 (HYPOTHESIS: as the CGB)
    masks[0x0C] = 0x1F; // NR32
    masks
}

/// Two banks of 32 four-bit digits, each a shift register.
///
/// DOC [CH3], WAVE_RAM: "Internally, Wave RAM is a giant shift-register, there is no pointer
/// which is addressing the currently played digit. Instead, the entire 128 bits are shifted,
/// and the 4 least significant bits are output." 128 bits is one bank (32 x 4 bits), so each
/// bank is its own register here, and a bank shifts only while it is the one played
/// (HYPOTHESIS: GBATEK does not say whether the bank not played also moves; it says only the
/// played one is output).
///
/// Each bank is held as its digits in play order: entry 0 is the digit that leaves next. The CPU
/// sees byte `k` of a bank as digits `2k` (high nibble) and `2k + 1` (low nibble): DOC [CH3],
/// WAVE_RAM, "Data is played back ordered as follows: MSBs of 1st byte, followed by LSBs of 1st
/// byte, followed by MSBs of 2nd byte, and so on". After a shift the same byte address holds
/// digits one place further on: "when reading from Wave RAM, data might have changed its
/// position" (DOC [CH3]).
///
/// The digit that leaves is fed back in at the far end, so a bank is a ring: DOC [CH3], "After
/// all 32 samples have been played, output of the same bank ... will be automatically
/// restarted"; after 32 shifts the bank is as it was.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WaveBanks {
    // HYPOTHESIS: GBATEK does not give the power-on contents; zero, the simplest (the DMG's
    // WAVE_POWER_ON is a DMG unit's and is not carried over)
    pub banks: [[u8; BANK_DIGITS]; 2],
    /// digits played from the bank now playing (0-31): GBATEK says the switch to the other bank
    /// comes "after all 32 samples have been played" (DOC [CH3]); the count is how this core
    /// knows when that is (HYPOTHESIS as to mechanism: GBATEK names no counter)
    pub count: i64,
    /// 64-digit mode: the bank playing is the one after the selected one
    pub second: bool,
}

impl Default for WaveBanks {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveBanks {
    pub fn new() -> Self {
        WaveBanks {
            banks: [[0; BANK_DIGITS]; 2],
            count: 0,
            second: false,
        }
    }

    /// The bank the CPU reads and writes: the one NR30 bit 6 does not select. DOC [CH3],
    /// SOUND3CNT_L: "The currently selected Bank Number (Bit 6) will be played back, while
    /// reading/writing to/from wave RAM will address the other (not selected) bank." GBATEK
    /// names no exception while the channel plays, or in 64-digit mode, so there is none
    /// (HYPOTHESIS: in 64-digit mode the CPU can be reaching the bank that is playing).
    pub fn cpu_bank(nr30: u8) -> usize {
        (((nr30 & NR30_BANK) >> 6) ^ 1) as usize
    }

    /// The bank played: the selected one (DOC [CH3], SOUND3CNT_L); with two banks, the other one
    /// for the second 32 digits (DOC [CMP]: "either one or both banks may be replayed (one after
    /// each other)"). The selection is read at each digit (HYPOTHESIS: GBATEK does not say when a
    /// change of bit 6 or bit 5 during playback takes effect).
    pub fn playing_bank(&self, nr30: u8) -> usize {
        let selected = ((nr30 & NR30_BANK) >> 6) as usize;
        if nr30 & NR30_DIMENSION != 0 && self.second {
            return selected ^ 1;
        }
        selected
    }

    /// CPU read of wave RAM byte `offset` (0-15) in the bank it reaches (DOC [CH3]).
    pub fn read(&self, nr30: u8, offset: usize) -> u8 {
        let digits = &self.banks[Self::cpu_bank(nr30)];
        (digits[2 * offset] << 4) | digits[2 * offset + 1]
    }

    /// CPU write of wave RAM byte `offset` (0-15) in the bank it reaches (DOC [CH3]). Wave RAM
    /// sits at 4000090h, outside the 4000060h..4000081h that NR52 clears and locks (DOC [CTL]),
    /// so this works with the APU powered off too.
    pub fn write(&mut self, nr30: u8, offset: usize, value: u8) {
        let digits = &mut self.banks[Self::cpu_bank(nr30)];
        digits[2 * offset] = (value >> 4) & 0xF;
        digits[2 * offset + 1] = value & 0xF;
    }

    /// NR34 bit 7 restarts the channel. DOC [CH3], SOUND3CNT_L: "When dimension is set to two
    /// banks, output will start by replaying the currently selected bank." There is no pointer
    /// to reset (DOC [CH3]), so the digits stay where they are and the next one out is whatever
    /// sits at the bank's end (HYPOTHESIS: GBATEK does not say a restart moves the register).
    pub fn trigger(&mut self) {
        self.count = 0;
        self.second = false;
    }

    /// One period of the channel's timer: the playing bank shifts one digit and the digit that
    /// leaves is the output (DOC [CH3]: "the 4 least significant bits are output"); it re-enters
    /// at the far end (the ring above). After 32 digits, 64-digit mode moves on to the other
    /// bank (DOC [CH3], [CMP]).
    pub fn shift(&mut self, nr30: u8) -> i64 {
        let digits = &mut self.banks[self.playing_bank(nr30)];
        let digit = digits[0];
        digits.rotate_left(1);
        self.count += 1;
        if self.count == BANK_DIGITS as i64 {
            self.count = 0;
            self.second = if nr30 & NR30_DIMENSION != 0 {
                !self.second
            } else {
                false
            };
        }
        digit as i64
    }

    /// Both banks as bytes, bank 0 then bank 1, each as the CPU would read it now (32 bytes).
    pub fn snapshot(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BANK_DIGITS);
        for digits in &self.banks {
            out.extend(
                (0..BANK_DIGITS)
                    .step_by(2)
                    .map(|i| (digits[i] << 4) | digits[i + 1]),
            );
        }
        out
    }
}

/// The wave channel's volume in quarters on the AGB: NR32 bit 7 forces 75 % "regardless of
/// above" (DOC [CH3], SOUND3CNT_H), so also over the mute setting; otherwise the CGB's
/// `quarters`. GBATEK does not say how 75 % of a digit rounds; this core's wave output is
/// `digit * quarters >> 2` for every row (HYPOTHESIS: the same truncation).
pub fn agb_wave_quarters(nr32: u8, quarters: i64) -> i64 {
    if nr32 & NR32_FORCE != 0 {
        FORCED_QUARTERS
    } else {
        quarters
    }
}
