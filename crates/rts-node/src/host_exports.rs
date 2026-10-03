//! What a host module's `module.exports` IS, declared in one place.
//!
//! # The question this answers, and why it had two answers
//!
//! Node has one rule: the ESM `default` of a CommonJS module is its
//! `module.exports`, whatever type that value has. Measured on Node 22.23.2 —
//! `events`, `assert`, `stream` and `module` export a FUNCTION, every other
//! `node:` module exports an object of properties.
//!
//! This crate had two answers instead. `require` read
//! `rts_core::entry::declare_module_common`'s value, while a default import read
//! the namespace object through `module_binding`'s host-module fallback. So
//! `require("events")` was the `EventEmitter` constructor and
//! `import EventEmitter from "events"` was the namespace — which is not a
//! constructor, and is what stopped `@whiskeysockets/baileys` building a socket.
//! The same split, in the other direction, made `require("ws")` an object while
//! `import WebSocket from "ws"` was the class.
//!
//! Two answers to one question is the defect, not either answer. [`declare`] is
//! the single statement: it writes `default` onto the namespace AND records the
//! CommonJS value, so the two views cannot come to disagree about one module.
//!
//! # Why `default` on the namespace rather than a case in the reader
//!
//! Because the reader already has the case it needs. `module_binding` reads the
//! `default` PROPERTY first and only falls back to the namespace on a miss, so a
//! module that states its `default` is answered by the ordinary path, and
//! `import fs from "node:fs"` — an object export, which declares nothing here —
//! keeps the fallback that makes it work. The alternative, teaching
//! `module_binding` to consult `common` before falling back, would have put a
//! second host-module special case into `rts-core`'s reader for a fact only this
//! crate knows; it is also the heavier change, in a file already far over its
//! ceiling.
//!
//! It is also what makes `import * as ns from "events"` answer
//! `ns.default === EventEmitter`, which Node does and which no fallback in a
//! named-import reader could ever reach.
//!
//! The property goes on the NAMESPACE and never on the exported value itself,
//! which is what keeps `require("events").default` undefined as Node has it: the
//! namespace and `module.exports` are two objects here, exactly as the ESM
//! namespace and `module.exports` are two objects there. A module whose
//! namespace IS its export — `assert`, built as a callable — therefore cannot
//! use this call without inventing a `.default` Node does not have.
//!
//! # What is still not Node, by name
//!
//! Three of the four function-exporting modules are NOT declared through this,
//! and each is a stated divergence rather than an oversight:
//!
//! - `stream`: Node's `module.exports` is the `Stream` class with `Readable`,
//!   `Writable`, `pipeline`, … hung off it as statics. This crate builds those
//!   as members of the namespace and not of the class, so declaring the class as
//!   the export would break `require("stream").Readable` — which works today.
//!   Mirroring the whole member list onto the class comes first, the way
//!   `events::namespace` already mirrors its six statics.
//! - `module`: the same shape, for `Module.createRequire`/`builtinModules`.
//! - `assert`: already answers the callable to both `require` and a default
//!   import, because its namespace IS the callable. Only `ns.default` is absent,
//!   and giving it one would also give `require("assert").default` one.

use rts_core::entry::{self, Context};

/// Declares one host module's `module.exports`.
///
/// `specifiers` must already be registered — every caller is either building the
/// namespace it passes or has just declared it — and all of them name the one
/// module, so the `node:` and bare spellings get the same value.
pub fn declare(context: &mut Context, specifiers: &[&str], namespace: u64, exports: u64) {
    entry::put_member(context, namespace, "default", exports);
    entry::declare_module_common(context, specifiers, exports);
}
