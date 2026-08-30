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
/// O modo de quem escreve, como argumento da operação de escrita.
///
/// `1` para sloppy. Uma escrita que o objeto recusa — só-getter, congelada,
/// `writable: false` — é um `TypeError` em strict e um no-op silencioso em
/// sloppy, e só o sítio de onde foi escrita sabe qual. `ctx.sloppy` já é por
/// função: `emit::function` limpa-o num corpo com `"use strict"`, por isso a
/// pergunta é a certa exatamente onde é feita.
/// O modo ESTRITO, para as escritas que o EMISSOR faz em objetos que acabou de
/// criar: um campo de classe, um elemento de literal, `module.exports`, o
/// objeto de um literal. Nada os pode ter congelado entre a criação e a
/// escrita, por isso a recusa não é uma resposta possível — e pedir o modo do
/// programa aqui daria a um `"use strict"` distante o poder de mudar como um
/// literal se constrói.
pub(super) fn estrito(builder: &mut FuncBuilder, _ctx: &Ctx) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar { repr: Repr::I64, bits: ScalarBits(0) });
    builder.use_const(id)
}

pub(super) fn write_mode(builder: &mut FuncBuilder, ctx: &Ctx) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(u64::from(ctx.sloppy)),
    });
    builder.use_const(id)
}

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

/// Reads a property whose key the program computed.
///
/// The same three blocks [`emit_read`] uses, and the same slow path the site
/// had before this existed — so what changed is that a warm site stops making
/// the call, not what the call does.
///
/// # What was measured, and against what
///
/// `o[k]` with a string key cost **36 ns** against **3 ns** for `o.a`, on
/// `target/release/rts.exe`, 2026-08-23, over 3 M iterations. The whole of that
/// gap is this call: the computed path emitted `Call __rts_get_indexed` and no
/// cache at all, so the site could not remember anything and every read paid the
/// borrow, five receiver probes and a key resolution.
///
/// # Why the key is not resolved here
///
/// It could be, and a resolved key would be a smaller word to compare. It is not
/// because resolving is the expensive half: reaching the string's payload and
/// reading its memo is ~11 ns measured, so a hit that resolved first could never
/// beat that floor. The machine compares the operand's own bits instead, and
/// `rts_cranelift::ir::inst::Terminator::CachedGetKeyed` carries why that is
/// sound in the one direction that matters.
///
/// # What is left on the slow path, on purpose
///
/// Everything that is not "a named property of this receiver's own layout": an
/// array element, a typed array's byte, a string's character, an inherited
/// property, an accessor, a proxy. Each of those is answered correctly by
/// `GetIndexed` and none of them is something a layout can locate, so the
/// resolver refuses and the site takes this path forever — one load and one
/// predicted branch slower than before, which is the price of the fast case.
pub(super) fn emit_read_keyed(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    receiver: ValueId,
    key: ValueId,
) -> EmitResult<ValueId> {
    // A key the emitter has PROVEN to be a number never takes this path, and
    // the reason is a measurement rather than tidiness. `a[i]` is an array
    // element, an element is not a property of any layout, so the resolver
    // refuses every time — and the site then pays a load, a compare and a call
    // that cannot succeed before falling through to the read it was already
    // doing. Measured 2026-08-23, `keys[i & 3]`: **16.0 ns became 23.7** with
    // this arm missing, a 48% regression on the commonest computed read there
    // is.
    //
    // Asked before the widening, because widening is what destroys the proof:
    // once it is `Tagged` nothing here can tell a number from a name. Rule 5 of
    // this crate's README, in the direction it is usually read backwards — what
    // IS proven is what lets a site opt out.
    //
    // A number that is not proven still reaches the cache and still misses
    // forever. That case is stated in the commit rather than fixed here: fixing
    // it needs a site that can remember it was refused, which is a machine
    // capability that does not exist.
    if matches!(builder.repr_of(key), Repr::F64 | Repr::I32 | Repr::I64) {
        let receiver = tagged(builder, receiver);
        let key = tagged(builder, key);
        return Ok(call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0]);
    }

    let receiver = tagged(builder, receiver);
    // Generic, and the verifier refuses anything else: the site recognises the
    // next key by comparing raw bits, so a proven double and its tagged
    // spelling would be two keys where the program wrote one.
    let key = tagged(builder, key);

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
    builder.cached_get_keyed(narrowed, key, cache, (join, &[]), (slow, &[]))?;

    builder.switch_to(slow);
    let answered = call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
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
    let mode = write_mode(builder, ctx);
    let answered = call(
        builder,
        ctx,
        // O MODO de quem escreve, e não uma propriedade da operação: uma
        // escrita que o objeto recusa é um `TypeError` em strict e um no-op em
        // sloppy, e só aqui se sabe qual dos dois. `ctx.sloppy` já é por função
        // — `emit::function` limpa-o num corpo com `"use strict"` — por isso a
        // pergunta é a certa exatamente neste ponto.
        RuntimeOp::SetProperty,
        &[receiver, key_value, value, mode],
    )?[0];
    builder.jump(join, &[answered])?;

    builder.switch_to(join);
    Ok(result)
}
