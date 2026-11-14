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
