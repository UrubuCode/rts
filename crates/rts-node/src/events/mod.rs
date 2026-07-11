//! `node:events` — the module specifier for the engine's `EventEmitter`.
//!
//! `EventEmitter` is a canonical engine global (the runtime layer's
//! `globals/events` Registry class — on/once/off/emit/listenerCount/eventNames/
//! removeAllListeners, with `AbiType::PolyValue` listener bridging). `node:events`
//! does not duplicate it (that would collide); it registers the `node:events`
//! module so `import { EventEmitter } from "node:events"` resolves to the same
//! class, matching Node (`require("node:events")` returns the EventEmitter class).
//!
//! The missing instance methods (addListener/removeListener aliases, prepend*,
//! listeners/rawListeners, get/setMaxListeners) are completed on the canonical
//! class in the runtime layer, not re-implemented here against a divergent
//! instance model.

use rts_engine::Engine;

/// Registers the `node:events` module specifier.
pub fn register(e: &mut Engine) {
    e.ns("node:events")
        .doc(
            "EventEmitter (node:events) — the engine's canonical EventEmitter \
             class. import { EventEmitter } from 'node:events'.",
        )
        .done();
}
