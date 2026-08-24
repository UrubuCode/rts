//! `node:stream/web` — the WHATWG streams, under the specifier Node gives them.
//!
//! # Why this is a re-export and not an implementation
//!
//! Because they already exist. `rts-std`'s `globals/streams/` builds
//! `ReadableStream`, `WritableStream`, `TransformStream` and the four codec
//! streams as GLOBALS, which is where the web platform puts them — and
//! `node:stream/web` is Node's second door to the same classes, not a second
//! set. Node's own documentation says so, and a program can check it:
//! `require('stream/web').ReadableStream === globalThis.ReadableStream`.
//!
//! So this reads them off the global object rather than building anything. Two
//! sets of classes would make that comparison false, and would make a stream
//! built through one door fail an `instanceof` at the other — the exact failure
//! `modules.rs` describes for two answers to one specifier.
//!
//! # What it costs to have been absent
//!
//! 18 files of Node's own suite died on `cannot find module "stream/web"`
//! (measured 2026-08-24) while the classes they wanted were already installed
//! and reachable by name. That is the shape of gap worth looking for: not a
//! missing implementation, a missing NAME over one that exists.
//!
//! # What is still absent, and why it is not here
//!
//! `stream/promises` and `stream/consumers` are not this. They are promise
//! shaped wrappers over `pipeline`/`finished` and over reading a stream to
//! completion, and neither of those exists yet in `super` — see its refusal
//! list. Registering an empty namespace under those names would be worse than
//! their absence: a program would import them and find nothing at the call.

use rts_core::entry::{self, Context};

/// The classes Node re-exports under `node:stream/web`.
///
/// The four codec streams are here beside the three core ones because Node puts
/// them there too — a program that reaches for `CompressionStream` through this
/// specifier is not asking for anything different from the global.
const CLASSES: &[&str] = &[
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "CompressionStream",
    "DecompressionStream",
    "TextEncoderStream",
    "TextDecoderStream",
];

/// Builds the namespace, from the globals already installed.
///
/// A class the globals do not carry is left OUT rather than written as
/// `undefined`: a namespace whose member exists and is undefined answers
/// `typeof ns.X === 'undefined'` exactly as an absent one does, and then fails
/// one line later at the `new` — where an absent member fails at the read, with
/// the name in the message.
pub(super) fn namespace(context: &mut Context) -> u64 {
    let namespace = entry::make_object(context);
    let global = entry::global_object(context);
    for name in CLASSES {
        let class = entry::get_member(context, global, name);
        if class == entry::undefined_in(context) {
            continue;
        }
        entry::put_member(context, namespace, name, class);
    }
    namespace
}
