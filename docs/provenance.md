# Provenance

Audited 2026-11-13 (provenance audit, wave 1).

**A transcription of gb-apu-python b8cab61; the verdicts are in gb-apu-python's
`docs/provenance.md`.** In short: the DMG side is from Pan Docs (CC0) and blargg's verdicts;
the CGB-family timing (envelope and NRx2 glitches, pulse channels in 2 MHz ticks, CGB sweep,
CGB noise and the NR43 glitch) is translated from SameBoy's `Core/apu.c` (Expat), whose notice
is now kept at the top of `src/lib.rs` (added 2026-11-13); two items there are **unclear**
and carry over here unchanged: `WAVE_POWER_ON` (16 bytes whose cited source, Pan Docs, does not
hold them) and the AGB row (written from GBATEK while mGBA's MPL-2.0 and NanoBoyAdvance's
GPL-3.0 sources were open). If gb-apu-python rewrites either, this crate follows.

**Additions of its own:** none from an outside source. The Rust-only parts (the comments on
where Rust forced a difference, the optional serde save-state feature, `examples/`, the
lockstep and seeded-fault scripts, the rung scripts) are own work. No constant, table or
comment exists here that is not in the Python apart from Rust plumbing (checked by comparing
every numeric literal in `src/lib.rs` against the Python module). `tests/streams/` are
gb-apu-python's board streams (traffic recordings, no ROM bytes). History: 8 commits, nothing
removed from elsewhere.

Owed: SPDX headers (family-wide pass).

## The AGB row rewritten clean-room (2026-11-25, branch `cleanroom-agb-wave`)

gb-apu-python rewrote the **unclear** AGB row from GBATEK alone (its `docs/provenance.md`,
"Clean-room rewrite of the AGB row"; new module `src/gb_apu_python/agb.py`). This crate follows,
as promised above: `src/agb.rs` is a transcription of `agb.py`, written from the Python module
alone. The old Rust AGB code (`other_bank`, `shift_wave`, the AGB branches of `read_wave`,
`write_wave`, `tick_wave`, `wave_quarters` and `wave_bytes`, the 64-entry `wave_digits` field and
its `bytes64` serde helper, `READ_MASKS_AGB`'s table) was removed by line number with a script
checked against the visible lines around each block, so its text was not read; a `grep -n` for
the AGB names did print five of its body lines (the 64-entry indexing in `read_wave` and
`write_wave`, the first line of `shift_wave`) before the rewrite here began, after the Python
design was settled. The nine AGB seeded faults, whose texts named the old code, are replaced by
gb-apu-python's ten new ones, broken the same way in `src/agb.rs`. The AGB row's verdict is now
**clean**, as in gb-apu-python. `WAVE_POWER_ON` stays **unclear**.

Save states: `APU` now serialises `agb_wave` (`None` on the DMG and CGB rows; on the AGB two
32-digit banks, a count and a flag) instead of the 64-byte `wave_digits`, so states saved before
this change do not load on any row (status.md, Provisional 2026-11-13: a host versions its own
states).
