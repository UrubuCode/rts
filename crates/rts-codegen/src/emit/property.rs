//! Reading and writing a property through the runtime, with the site's cache.
//!
//! # Why this is not in `expr.rs`
//!
//! Because eight modules reach for it and none of them is emitting an
//! expression when they do. `binding.rs` reads a captured variable, `class.rs`
//! reads `prototype`, `call.rs` reads a method, `unary.rs` writes back an
//! increment — a property access is the mechanism *underneath* several language
//! constructs rather than one of them.
//!
//! It is also the thing `escape.rs` exists to avoid. A read here is a guard, a
//! cache and a possible call across four blocks; the whole point of proving an
//! object cannot escape is to emit none of it. Keeping the cost in one file is
//! what makes that trade legible.

use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::{RefKind, Repr};

use super::expr::{call, tagged};
use super::{Ctx, EmitResult, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
/// The number a property name has, as a machine constant.
///
/// An integer rather than a tagged value: it is not a JavaScript value at all,
/// it is which name the compiler resolved. Emitting it tagged would be claiming
/// the program could compute it, which is exactly what a computed property does
/// and what this path is not.
pub(super) fn key_constant(builder: &mut FuncBuilder, ctx: &mut Ctx, name: Name) -> ValueId {
    let key = ctx.key_of(name);
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(u64::from(key)),
    });
    builder.use_const(id)
}
/// Reads a property, through the site's memory of what it last saw.
///
/// # The shape of what this emits
///
/// ```text
///   guard the receiver is a reference ─── not one ──┐
///            │                                      │
///   cached_get ─── the layout changed ──────────────┤
///            │                                      │
///          hit(value)                             slow: call the runtime
///            │                                      │
///            └───────────────► join(value) ◄────────┘
/// ```
///
/// Four blocks for one property read, and every one of them is required by
/// something the machine refuses to assume.
///
/// # Why the guard is there at all
///
/// `cached_get` takes a **proven reference** and a JavaScript value is generic:
/// `o` might be a number. The machine will not narrow without a guard, because
/// narrowing can fail — and `1..x` failing quietly is a load from an integer.
///
/// # Why the slow path is a call and not a repeat of the fast one
///
/// The site missed, which means either the object has a different layout or the
/// property is not somewhere a load reaches. Both are the runtime's to answer,
/// and answering them here would be a second implementation of what
/// `get_property` already is.
///
/// # What the cache is not
///
/// Ours to fill. The machine writes it from what `rts_cache_resolve` returns:
/// *"there is no cell to initialize, no miss handler to write, and no way to
/// forget to update it"*. This declares one and names it; that is all.
pub(super) fn emit_read(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    receiver: ValueId,
    property: Name,
) -> EmitResult<ValueId> {
    let receiver = tagged(builder, receiver);
    let key = ctx.shape_key(property);

    let as_reference = builder.create_block();
    let narrowed = builder.add_block_param(as_reference, Repr::Ref(RefKind::Opaque));
    let slow = builder.create_block();
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    builder.guard(
        receiver,
        Repr::Ref(RefKind::Opaque),
        (as_reference, &[]),
        (slow, &[]),
    )?;

    builder.switch_to(as_reference);
    let cache = builder.declare_cache();
    // Straight to the join, with no `hit` block in between.
    //
    // There was one: a block whose only parameter was the value the cache
    // found and whose only instruction was a jump handing that same value to
    // `join`. It could go because the machine already does the handing —
    // `cached_get` prepends the found value to whatever the hit `BlockCall`
    // carries, which is why the hit target must have `Repr::Tagged` first and
    // why the args here are empty. `join`'s one parameter IS that shape, so the
    // forwarding block was passing a value to itself.
    //
    // Three sites in this file did it and they were 403 of the 1 072 blocks in
    // `bench/analytic.ts` that held nothing but a `Jump`.
    builder.cached_get(narrowed, key, cache, (join, &[]), (slow, &[]))?;

    builder.switch_to(slow);
    let key_value = key_constant(builder, ctx, property);
    let answered = call(builder, ctx, RuntimeOp::GetProperty, &[receiver, key_value])?[0];
    builder.jump(join, &[answered])?;

    builder.switch_to(join);
    Ok(result)
}

/// Reads a property the site may last have found on something the receiver
/// inherits from.
///
/// # Why this is a second function and not a parameter
///
/// Because it costs more, and the extra is paid on the recognised path. A site
/// that reads a field of an object it holds must not pay for the possibility of
/// a method it never calls, and the only place that difference is known is
/// where the read is emitted — [`emit_read`] is what nine callers want and this
/// is what two do.
///
/// # Which two, and why the position is the right signal
///
/// The callee of a call. `o.m()` reads `m` and immediately calls it, and a
/// method is written on a class body, which puts it on the prototype rather
/// than on the instance — so the cheap form's cache can never arm and the site
/// re-resolves on every pass, forever. Everything else keeps the cheap form.
///
/// The signal is syntactic rather than proved, and that is deliberate: nothing
/// in this crate knows what an expression's type is (there is no type pass), and
/// a guess that is wrong costs a load and a predicted branch rather than a wrong
/// answer. A callee that turns out to be an own property — `handlers.a()`,
/// `Math.abs()`, `this.cb()` — resolves on the first pass and hits every time
/// after, exactly as it does today, one load slower.
pub(super) fn emit_read_indirect(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    receiver: ValueId,
    property: Name,
) -> EmitResult<ValueId> {
    let receiver = tagged(builder, receiver);
    let key = ctx.shape_key(property);

    let as_reference = builder.create_block();
    let narrowed = builder.add_block_param(as_reference, Repr::Ref(RefKind::Opaque));
    let slow = builder.create_block();
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    builder.guard(
        receiver,
        Repr::Ref(RefKind::Opaque),
        (as_reference, &[]),
        (slow, &[]),
    )?;

    builder.switch_to(as_reference);
    let cache = builder.declare_cache();
    // No `hit` block, for the reason [`emit_read`] states: the machine prepends
    // the found value itself, so a block that received it and passed it on was
    // handing a value to itself.
    builder.cached_get_indirect(narrowed, key, cache, (join, &[]), (slow, &[]))?;

    builder.switch_to(slow);
    let key_value = key_constant(builder, ctx, property);
    let answered = call(builder, ctx, RuntimeOp::GetProperty, &[receiver, key_value])?[0];
    builder.jump(join, &[answered])?;

    builder.switch_to(join);
    Ok(result)
}

/// Writes a property, through the site's memory of what it last saw.
///
/// The mirror of [`emit_read`] with one difference that is not symmetry: the
/// slow path is not a slower store. A key the object does not have changes what
/// the object IS, which is a shape transition — and a transition is not
/// something a site can remember, because the next object through it may be at
/// a different layout entirely. `rts_cache_resolve` answers that it cannot be
/// reached this way, and `set_property` is what takes the transition.
///
/// So the fast path is exactly the case a store repeats: a property the object
/// already has.
pub(super) fn emit_write(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    receiver: ValueId,
    property: Name,
    value: ValueId,
) -> EmitResult<ValueId> {
    let receiver = tagged(builder, receiver);
    let value = tagged(builder, value);
    let key = ctx.shape_key(property);

    let as_reference = builder.create_block();
    let narrowed = builder.add_block_param(as_reference, Repr::Ref(RefKind::Opaque));
    let slow = builder.create_block();
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    builder.guard(
        receiver,
        Repr::Ref(RefKind::Opaque),
        (as_reference, &[]),
        (slow, &[]),
    )?;

    builder.switch_to(as_reference);
    let cache = builder.declare_cache();
    // An assignment is an expression, and what it produces is the value that was
    // assigned — the same on both paths, which is why the join carries it rather
    // than each path answering separately.
    //
    // It is carried straight from here. There was a `stored` block whose only
    // instruction was `jump(join, &[value])`, and unlike the two reads above
    // this one needs the argument written out: `cached_set` has no found value
    // to prepend, so its hit `BlockCall` carries what the caller puts in it.
    builder.cached_set(narrowed, key, cache, value, (join, &[value]), (slow, &[]))?;

    builder.switch_to(slow);
    let key_value = key_constant(builder, ctx, property);
    let answered = call(
        builder,
        ctx,
        RuntimeOp::SetProperty,
        &[receiver, key_value, value],
    )?[0];
    builder.jump(join, &[answered])?;

    builder.switch_to(join);
    Ok(result)
}
