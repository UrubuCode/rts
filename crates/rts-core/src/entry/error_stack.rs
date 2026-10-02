//! `.stack`: the accessor every error inherits, and V8's `captureStackTrace`.
//!
//! # Why this is a module of its own
//!
//! Because `error.rs` was 620 lines against this crate's 500-line ceiling, and
//! the cohesive half to take out is the one that is about a TRACE rather than
//! about the `Error` family: the deferred accessor, its setter, and the static
//! that writes the same property onto something that is not an error at all.
//! `error.rs` keeps the eight declarations and the message/cause protocol.
//!
//! # What already answered this, and what is called rather than rewritten
//!
//! Everything but the entry itself. `throw::stack_text_of` renders a captured
//! `Vec<u64>` of callees into the `at …` block, `error_describe::joined` builds the
//! `Name: message` header from properties alone, and `Context::callees` IS the
//! call stack — `functions::invoke` pushes onto it so a bound function can know
//! its binding. Keeping a second record of the frames is the one thing this
//! module must not do: two records of one fact disagree the first time either
//! forgets to pop.
//!
//! # `Error.captureStackTrace`, and the honest size of what it does
//!
//! It is not ECMAScript. It is V8's, Node exposes it, and half the ecosystem
//! calls it — any library that builds its own errors calls it to erase its own
//! frame from the trace. `node_modules/ws` calls
//! `Error.captureStackTrace(err, abortHandshake)` and died on `undefined is not
//! a function` here, which ends the process at the call where Node would have
//! carried on.
//!
//! What it puts in `.stack` is the same trace `new Error(…).stack` carries here:
//! a header line and one `at <name>` per named frame, **with no file and no line
//! number**. Node's carries `at inner (file.js:7:40)`. That shortfall is not
//! this module's to close — the machine records a source position per
//! instruction and nothing maps an address back to one at run time
//! (`rts_cranelift::observe`'s question, issue #2862) — and the trace is still
//! the difference between "something threw" and "this path threw".
//!
//! # Why the string is rendered HERE and not deferred
//!
//! `new Error(…)` defers: it stores the frames and renders them only if
//! something reads `.stack`, because almost nothing does and rendering was
//! measured at 320 ns of a 790 ns constructor. The deferral table cannot be used
//! here, for a reason that has nothing to do with cost: `has_pending_stack` is
//! what `Object.prototype.toString` asks to decide that a cell is an Error, so
//! deferring on a plain object would make `Object.prototype.toString.call({})`
//! answer `[object Error]` after a `captureStackTrace` on it. Rendering now also
//! needs no justification on cost grounds: a program that calls this is a
//! program that wants a trace.
//!
//! The visible difference from Node is the property's SHAPE — Node installs an
//! accessor pair and recomputes the header on the first read, so assigning
//! `obj.name` after the capture changes the header there and not here. Both
//! answer a non-enumerable `.stack` string that a program can overwrite.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// `Error.captureStackTrace(target, hideFrom)`.
///
/// # What the second argument does
///
/// It names a function whose frame, and every frame INSIDE it, is dropped — the
/// point of the call, since a library's own helper is what the caller does not
/// want to read about. Measured in Node 22: a function that is not on the stack
/// at all drops every frame, which is what V8's "skip until found" loop does
/// when it never finds it, so that is what this does rather than quietly
/// ignoring it.
///
/// A second argument that is not callable is ignored, which is also V8:
/// `captureStackTrace({}, 5)` returns without complaint.
///
/// # Why a non-object first argument throws
///
/// Because Node does, with that exact text, and the alternative is the failure
/// mode this whole surface exists to remove: a call that answers `undefined` and
/// lets the program carry on without the property it asked for.
pub(in crate::entry) fn capture(target: u64, hide_from: u64) -> u64 {
    // Outside the borrow, before anything is built: `type_error` builds the
    // program's own `TypeError`, which allocates.
    if !with_current(|context| super::objects::is_object(context, target)) {
        super::throw::type_error("invalid_argument");
        return with_current(|context| undefined_of(context));
    }
    let limit = with_current(limit_of);
    with_current(|context| {
        let Some(cell) = Value(target).as_slot() else {
            return undefined_of(context);
        };
        // Whatever the constructor deferred is now stale: this call decides what
        // `.stack` says, and leaving the frames behind would let the inherited
        // accessor overwrite the answer the first time something read it. The
        // setter drops them for the same reason.
        context.take_stack(cell);
        let frames = kept(context, hide_from, limit);
        // The header off the target's own properties, which is what V8 reads:
        // `captureStackTrace({ name: "Foo", message: "bar" })` opens with
        // `Foo: bar`, and an object carrying neither opens with `Error`.
        // `joined` runs no user code, so it is safe inside this borrow.
        let header = super::error_describe::joined(context, cell).unwrap_or_else(|| "Error".to_owned());
        let text = format!("{header}{}", super::throw::stack_text_of(context, &frames));
        let value = context.intern_value(Str::from_str(&text)).bits();
        let key = context.well_known("stack");
        super::objects::put(context, cell, key, value);
        // Non-enumerable, as the accessor Node installs is: `Object.keys(obj)`
        // answers `[]` after a capture, and an enumerable one would put the
        // whole trace into `JSON.stringify(obj)`.
        super::native::hidden(context, cell, key);
        undefined_of(context)
    })
}

/// The frames the trace keeps: innermost last, as [`super::throw::stack_text_of`]
/// expects them.
///
/// Two cuts, in this order. `hide_from` cuts from the inside, because what it
/// names is a frame the caller knows about; the limit cuts from the OUTSIDE,
/// keeping the innermost `limit` frames, which is what V8 keeps and what makes
/// `Error.stackTraceLimit = 2` answer the two frames nearest the capture.
fn kept(context: &Context, hide_from: u64, limit: usize) -> Vec<u64> {
    let mut frames = context.callees.clone();
    // This native's OWN frame is on the list — `functions::invoke` pushes every
    // callee, a native included — and Node's trace opens at the function that
    // called `captureStackTrace`. Dropping it here rather than teaching
    // `stack_text_of` about natives: the renderer is shared with `new Error`,
    // whose capture happens one frame deeper for the same reason and already
    // comes out right.
    frames.pop();
    let callable = Value(hide_from)
        .as_slot()
        .is_some_and(|cell| context.callable_at(cell).is_some());
    if callable {
        // By IDENTITY, not by code address: two closures over one body are two
        // functions to a program, and the caller passed the one it has.
        frames.truncate(frames.iter().rposition(|callee| *callee == hide_from).unwrap_or(0));
    }
    let over = frames.len().saturating_sub(limit);
    frames.drain(..over);
    frames
}

/// `Error.stackTraceLimit`, as a count of frames.
///
/// Read as a PROPERTY of the constructor on every capture, never cached: the
/// name exists so a program can write it, and a cached copy would be a second
/// record of the one number the program thinks it just changed.
///
/// A value that is not a number is zero here, where V8 omits the `stack`
/// property altogether. The divergence is deliberate: a `.stack` that is
/// `undefined` is the failure this module was written to remove, and nothing
/// sets the limit to a string on purpose.
fn limit_of(context: &mut Context) -> usize {
    let Some(constructor) = super::class_support::made(context, "Error") else {
        return 0;
    };
    let Some(cell) = Value(constructor).as_slot() else {
        return 0;
    };
    let key = context.well_known("stackTraceLimit");
    let Some(found) = super::objects::read_property(context, cell, key) else {
        return 0;
    };
    found.as_f64().filter(|limit| *limit > 0.0).unwrap_or(0.0) as usize
}

/// Puts the `stack` accessor on `Error.prototype`, once per context.
///
/// ON THE PROTOTYPE, not on each instance. Per instance was refused by reading
/// `integrity::retype`, which `define_accessor_and_invalidate` calls: it
/// declares a FRESH TYPE for the cell. Doing that per construction would mint a
/// type per Error and invalidate every inline cache that has ever seen one —
/// more expensive than the thing it replaces, and paid by unrelated code. The
/// six subclasses inherit it, because their prototypes chain to this one.
///
/// AT CONSTRUCTION, not at registration, and that is not tidiness.
/// `register_type_error` and its five siblings reach `register_error` directly
/// through the macro's `extends`, so an internal `TypeError` — one this engine
/// throws itself — builds `Error.prototype` without passing through
/// `error::provided`. Installing there worked under `rts run` and left the
/// 332 tests that share one process reading `undefined` from every `.stack`.
pub(in crate::entry) fn install_stack_accessor(context: &mut Context) {
    if context.stack_accessor {
        return;
    }
    if let Some(prototype) = super::class_support::prototype(context, "Error") {
        context.stack_accessor = true;
        super::accessor::define_accessor_in(context, prototype, "stack", stack_get, Some(stack_set));
    }
}

/// `err.stack` — rendered here, on the first read, and never again.
///
/// The first read installs an OWN data property and drops the captured frames,
/// so a second read is an ordinary cached property read rather than a second
/// call through here. That also means a program that reads `.stack` twice pays
/// what it used to pay once, and one that never reads it pays nothing.
extern "C" fn stack_get(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        // Already rendered, or written by the setter: the own property answers
        // and this accessor is only reached because the own one is absent.
        let Some((class, frames)) = context.take_stack(cell) else {
            return undefined_of(context);
        };
        let described = super::error_describe::joined(context, cell).unwrap_or_else(|| class.to_owned());
        let stack = format!("{described}{}", super::throw::stack_text_of(context, &frames));
        let value = context.intern_value(Str::from_str(&stack)).bits();
        let key = context.well_known("stack");
        super::objects::put(context, cell, key, value);
        super::native::hidden(context, cell, key);
        value
    })
}

/// `err.stack = v` — an own data property, which is what a write to it makes in
/// every engine, and what drops the captured frames.
extern "C" fn stack_set(_e: u64, this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        context.take_stack(cell);
        let key = context.well_known("stack");
        super::objects::put(context, cell, key, value);
        super::native::hidden(context, cell, key);
        undefined_of(context)
    })
}
