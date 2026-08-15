//! `TransformStream` and `TransformStreamDefaultController`, and the pair
//! builder the four native transforms are made of.
//!
//! # Why [`pair`] is here rather than in each class
//!
//! `CompressionStream`, `DecompressionStream`, `TextEncoderStream` and
//! `TextDecoderStream` are all defined by their own standards as "a
//! `TransformStream` whose transform is …". The only thing that varies between
//! them and a program's own `new TransformStream({ transform })` is where the
//! transform comes from — a native of this crate instead of a JS function — so
//! what they share is exactly this file's `readable` + `writable` + sink
//! wiring, written once. What they do NOT share is the transform controller: a
//! native sink enqueues into its readable directly, so it needs no controller
//! object at all.

use rts_core::entry::{self, Context, Provided};

use super::{field, hook, threw};

/// The readable half, recorded on the sink and on the controller.
pub(super) const READABLE: &str = "__readable";
/// The `transformer` a program handed the constructor.
const TRANSFORMER: &str = "__transformer";
/// The `TransformStreamDefaultController` a transformer's hooks are called with.
const CONTROLLER: &str = "__ctl";

const STREAM_METHODS: &[(&str, Provided)] = &[];

const CONTROLLER_METHODS: &[(&str, Provided)] = &[
    ("enqueue", controller_enqueue),
    ("terminate", controller_terminate),
    ("error", controller_error),
];

/// The `TransformStream` constructor.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_of(context, "TransformStream", prototype, construct)
}

fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TransformStream", STREAM_METHODS)
}

fn controller_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TransformStreamDefaultController", CONTROLLER_METHODS)
}

/// A readable/writable pair on `instance`, with a sink whose `write` and
/// `close` are the two natives given.
///
/// Answers the sink, so the caller can stamp whatever state its own transform
/// needs on it — a codec's table key, a `TextDecoder` instance. The sink is
/// what `this` is inside both natives, which is why that is the place for it.
pub(super) fn pair(context: &mut Context, instance: u64, write: Provided, close: Provided) -> u64 {
    let readable = super::readable::make(context);
    let sink = entry::make_object(context);
    entry::put_member(context, sink, READABLE, readable);
    let held = entry::make_callable(context, write);
    entry::put_member(context, sink, "write", held);
    let held = entry::make_callable(context, close);
    entry::put_member(context, sink, "close", held);
    let writable = super::writable::make(context, sink);
    entry::put_member(context, instance, "readable", readable);
    entry::put_member(context, instance, "writable", writable);
    sink
}

// --------------------------------------------------------------- the natives

/// `new TransformStream(transformer?, writableStrategy?, readableStrategy?)`.
extern "C" fn construct(_e: u64, this: u64, transformer: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (instance, controller) = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        let sink = pair(context, instance, transform_write, transform_close);
        entry::put_member(context, sink, TRANSFORMER, transformer);
        let readable = entry::get_member(context, sink, READABLE);
        let held = controller_prototype(context);
        let controller = entry::make_instance(context, held);
        entry::put_member(context, controller, READABLE, readable);
        entry::put_member(context, sink, CONTROLLER, controller);
        (instance, controller)
    });
    // OUTSIDE the borrow: `start` is user code — see `readable::construct`.
    if let Some(start) = hook(transformer, "start") {
        let absent = entry::undefined_value();
        entry::call(start, transformer, controller, absent, absent, absent);
        if threw() {
            return absent;
        }
    }
    instance
}

/// The writable half's sink `write`: one chunk through the transformer.
///
/// `this` is the sink. A transformer with no `transform` is the identity one
/// the specification defines, which is why the absent case enqueues rather than
/// dropping the chunk.
extern "C" fn transform_write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let transformer = field(this, TRANSFORMER);
    let Some(transform) = hook(transformer, "transform") else {
        super::readable::enqueue(field(this, READABLE), chunk);
        return absent;
    };
    let controller = field(this, CONTROLLER);
    let answer = entry::call(transform, transformer, chunk, controller, absent, absent);
    if threw() {
        return absent;
    }
    answer
}

/// The sink `close`: `flush`, then the readable half ends.
///
/// A `flush` that answers a promise is not awaited before the close — the
/// readable ends in this call. Named in the folder's module doc rather than
/// worked around, because awaiting it needs a continuation this file has no way
/// to express without a `.then` on user code.
extern "C" fn transform_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let transformer = field(this, TRANSFORMER);
    if let Some(flush) = hook(transformer, "flush") {
        let controller = field(this, CONTROLLER);
        entry::call(flush, transformer, controller, absent, absent, absent);
        if threw() {
            return absent;
        }
    }
    super::readable::close(field(this, READABLE));
    absent
}

extern "C" fn controller_enqueue(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::enqueue(field(this, READABLE), chunk);
    entry::undefined_value()
}

extern "C" fn controller_terminate(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::close(field(this, READABLE));
    entry::undefined_value()
}

extern "C" fn controller_error(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::fail(field(this, READABLE), reason);
    entry::undefined_value()
}
