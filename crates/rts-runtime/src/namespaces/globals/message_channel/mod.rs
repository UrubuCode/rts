//! MessageChannel / MessagePort (web messaging) — minimal synchronous model.
//!
//! Cross-runtime fixture `76_message_channel` uses MessageChannel as an
//! ambient global. RTS models it "outside the box": both ports are plain
//! `Entry::Map` objects, so `port.onmessage = cb` is a generic property store
//! and `port.postMessage(x)` / `port.close()` are reified bound fn handles
//! kept in the port map. `postMessage` delivers to the PEER port's `onmessage`
//! **synchronously** (the spec is async/next-tick, but for handlers installed
//! before the post — the common case, and what the fixture does — the observed
//! output is identical). A future upgrade can queue + drain at the event-loop
//! exit for full async fidelity.

pub mod abi;
pub mod instance;
