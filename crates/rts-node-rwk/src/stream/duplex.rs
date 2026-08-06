//! `stream.Duplex`, `stream.Transform`, `stream.PassThrough`.
//!
//! # How `Duplex` gets both APIs with one prototype chain
//!
//! Node makes `Duplex.prototype = Object.create(Readable.prototype)` and then
//! copies `Writable.prototype`'s own methods onto it — a mixin, not a second
//! `extends`. This module does the same: [`duplex_prototype`] chains onto
//! `"Readable"` (so `instanceof Readable` holds, matching Node) and adds
//! every [`super::writable::METHODS`] entry as an OWN member of the `Duplex`
//! prototype, `destroy` excepted (it needs its own version that closes both
//! halves — see [`duplex_destroy`]).
//!
//! # How `Transform` turns a write into a push, with no closures
//!
//! `Transform` sets `_write` to [`transform_write_hook`] — a fixed native
//! function, installed as an instance OWN property exactly the way an
//! options-supplied `_write` would be (see `writable.rs`'s module doc for
//! why that lookup is uniform). From `writable::dispatch_one`'s point of
//! view this **is** an ordinary `_write`: it is handed a chunk and a
//! callback and must call the callback once. What it does with them is call
//! the instance's `_transform` (own property from options, or this module's
//! own identity default), and its own callback — [`transform_done`] —
//! relays `_transform`'s `(error, data)` into a `push()` plus the original
//! completion. [`TRANSFORM_PENDING`] is the same "thread-local stands in for
//! a bound callback" trick `writable.rs::PENDING` uses, because
//! [`transform_done`] has the same no-closure problem `write_done` does.

use rts_core_rwk::entry::{self, Provided};
use std::cell::RefCell;

use super::common::*;
use super::{readable, writable};

// ------------------------------------------------------------------ Duplex --

fn duplex_methods() -> Vec<(&'static str, Provided)> {
    writable::METHODS.iter().copied().filter(|(name, _)| *name != "destroy").chain([("destroy", duplex_destroy as Provided)]).collect()
}

pub(super) fn duplex_prototype(context: &mut entry::Context) -> u64 {
    let parent = readable::prototype(context);
    let prototype = entry::make_prototype(context, "Duplex", &duplex_methods());
    entry::set_prototype_in(context, prototype, parent);
    prototype
}

pub(super) extern "C" fn duplex_construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = duplex_prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        readable::init(context, instance, options);
        writable::init(context, instance, options);
        let half_open = super::option_flag_default(options, "allowHalfOpen").unwrap_or(true);
        set_bool(context, instance, "allowHalfOpen", half_open);
        instance
    })
}

/// `duplex.destroy(error?)` — one call, both halves: real Node's `Duplex`
/// has a single `destroyed` flag shared by both APIs, which is what makes
/// running `readable::destroy` then `writable::destroy` (each idempotent,
/// each guarded by its own flag) safe here rather than double-firing
/// `'close'`. Only the readable side's `_destroy` hook is called — the two
/// modules would otherwise both look it up and both call it once each.
extern "C" fn duplex_destroy(e: u64, this: u64, error: u64, b: u64, c: u64, d: u64) -> u64 {
    readable::destroy(e, this, error, b, c, d);
    // `writable::destroy` would emit `'close'` a second time; its own
    // bookkeeping (writable/destroyed/errored) still needs setting by hand.
    entry::with_runtime(|context| {
        set_bool(context, this, "writable", false);
    });
    this
}

// --------------------------------------------------------------- Transform --

thread_local! {
    // The `Transform` instance and the ORIGINAL `_write` callback a
    // dispatched `_transform` call must relay into — see the module doc.
    static TRANSFORM_PENDING: RefCell<Vec<(u64, u64)>> = const { RefCell::new(Vec::new()) };
}

const TRANSFORM_METHODS: &[(&str, Provided)] = &[("_transform", default_transform)];

pub(super) fn transform_prototype(context: &mut entry::Context) -> u64 {
    let parent = duplex_prototype(context);
    let prototype = entry::make_prototype(context, "Transform", TRANSFORM_METHODS);
    entry::set_prototype_in(context, prototype, parent);
    prototype
}

pub(super) extern "C" fn transform_construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    build_transform(this, options, transform_prototype)
}

fn build_transform(this: u64, options: u64, prototype_fn: fn(&mut entry::Context) -> u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype_fn(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        readable::init(context, instance, options);
        writable::init(context, instance, options);
        let half_open = super::option_flag_default(options, "allowHalfOpen").unwrap_or(true);
        set_bool(context, instance, "allowHalfOpen", half_open);
        // Overrides whatever `writable::init` read from `options.write` —
        // `Transform`'s write path is never the caller's to set directly.
        let write_hook = entry::make_callable(context, transform_write_hook);
        set_value(context, instance, "_write", write_hook);
        let final_hook = entry::make_callable(context, transform_final);
        set_value(context, instance, "_final", final_hook);
        let function = super::option_member(options, "transform");
        let absent = entry::undefined_value();
        if function != absent {
            set_value(context, instance, "_transform", function);
        }
        let flush = super::option_member(options, "flush");
        if flush != absent {
            set_value(context, instance, "_flush", flush);
        }
        instance
    })
}

/// The default `_transform` every `Transform`/`PassThrough` falls back to
/// when no subclass or options function overrides it: pass the chunk
/// through unchanged. This is what makes a bare `new Transform()` behave
/// like `PassThrough` rather than throwing — real Node's base `Transform`
/// throws `ERR_METHOD_NOT_IMPLEMENTED`; this crate's no-throw convention
/// (`fs/mod.rs`'s stated rule) turns that into an identity pass-through
/// instead, a named simplification rather than a silent one.
extern "C" fn default_transform(_e: u64, _this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::call(callback, absent, absent, chunk, absent, absent);
    absent
}

/// Installed as `this._write`: routes every write through `_transform`
/// instead of buffering it — see the module doc.
extern "C" fn transform_write_hook(_e: u64, this: u64, chunk: u64, encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let transform_hook = entry::with_runtime(|context| entry::get_member(context, this, "_transform"));
    if transform_hook == absent {
        entry::call(callback, absent, absent, absent, absent, absent);
        return absent;
    }
    TRANSFORM_PENDING.with(|stack| stack.borrow_mut().push((this, callback)));
    let relay = entry::with_runtime(|context| entry::make_callable(context, transform_done));
    let encoding_key = if encoding == absent { key("buffer") } else { encoding };
    entry::call(transform_hook, this, chunk, encoding_key, relay, absent);
    absent
}

/// `_transform`'s own callback: `(error, data?)`. Pushes `data` (if any)
/// onto the readable side, then relays completion to the ORIGINAL `_write`
/// callback `transform_write_hook` was itself handed.
extern "C" fn transform_done(_e: u64, _this: u64, error: u64, data: u64, _c: u64, _d: u64) -> u64 {
    let Some((instance, original_callback)) = TRANSFORM_PENDING.with(|stack| stack.borrow_mut().pop()) else {
        return entry::undefined_value();
    };
    let absent = entry::undefined_value();
    if data != absent {
        let push_fn = entry::with_runtime(|context| entry::get_member(context, instance, "push"));
        entry::call(push_fn, instance, data, absent, absent, absent);
    }
    entry::call(original_callback, absent, error, absent, absent, absent);
    absent
}

/// The default `_final`: runs `_flush` (own property or absent), pushes any
/// trailing data it hands back, then completes — this is what
/// `writable::maybe_finish` calls before `'finish'`, found through the same
/// own-property-first lookup every hook in this crate uses.
extern "C" fn transform_final(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let flush_hook = entry::with_runtime(|context| entry::get_member(context, this, "_flush"));
    if flush_hook == absent {
        entry::call(callback, absent, absent, absent, absent, absent);
        return absent;
    }
    TRANSFORM_PENDING.with(|stack| stack.borrow_mut().push((this, callback)));
    let relay = entry::with_runtime(|context| entry::make_callable(context, transform_done));
    entry::call(flush_hook, this, relay, absent, absent, absent);
    absent
}

// ------------------------------------------------------------- PassThrough --

pub(super) fn pass_through_prototype(context: &mut entry::Context) -> u64 {
    let parent = transform_prototype(context);
    let prototype = entry::make_prototype(context, "PassThrough", &[]);
    entry::set_prototype_in(context, prototype, parent);
    prototype
}

pub(super) extern "C" fn pass_through_construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    build_transform(this, options, pass_through_prototype)
}
