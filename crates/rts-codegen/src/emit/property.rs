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
    let hit = builder.create_block();
    let found = builder.add_block_param(hit, UNPROVEN);
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
    builder.cached_get(narrowed, key, cache, (hit, &[]), (slow, &[]))?;

    builder.switch_to(hit);
    builder.jump(join, &[found])?;

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
    let stored = builder.create_block();
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
    builder.cached_set(narrowed, key, cache, value, (stored, &[]), (slow, &[]))?;

    // An assignment is an expression, and what it produces is the value that was
    // assigned — the same on both paths, which is why the join carries it rather
    // than each path answering separately.
    builder.switch_to(stored);
    builder.jump(join, &[value])?;

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
