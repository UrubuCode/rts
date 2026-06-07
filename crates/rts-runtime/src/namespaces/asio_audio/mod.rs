//! `asio_audio` namespace — low-latency audio output via ASIO (Windows).
//!
//! Identical API to `audio` (pull model, ring buffer, f32 samples via buffer),
//! but routed through the ASIO host instead of the OS default. Gives exclusive,
//! low-latency access to the hardware — relevant for live monitoring / virtual
//! instruments, not for plain playback (where WASAPI `audio` is already fine).
//!
//! Compiled ONLY with the `asio` feature (requires the Steinberg ASIO SDK +
//! LLVM/libclang at build time) and needs an installed ASIO driver (ASIO4ALL or
//! the sound card's own). See `docs/specs/audio.md`.

pub mod abi;
pub mod ops;
pub mod state;
