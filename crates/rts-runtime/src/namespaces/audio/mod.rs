//! `audio` namespace — low-level audio device I/O via cpal.
//!
//! Raw primitive only ("virtual sound card"): open/close an output device,
//! push interleaved f32 samples into a ring buffer, drained on the real-time
//! audio thread. All high-level audio (oscillators, ADSR, mixer, effects,
//! format decoding) is TypeScript on top of this — the dev controls everything.
//!
//! Pull model: TS owns the loop (generate samples → check `available_frames`
//! → `write`). Rust never calls back into TS, so no GC/allocation ever runs on
//! the audio thread. See `docs/specs/audio.md`.
//!
//! ASIO (low-latency, Windows) is a separate opt-in namespace (`rts:asio_audio`,
//! feature `asio`) — not bundled here, to keep this path pure-Rust and
//! standalone-friendly.

pub mod abi;
pub mod ops;
pub mod state;
