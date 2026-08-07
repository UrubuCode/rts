//! An object literal, and the accessors a literal or a class body defines.
//!
//! # Why an accessor is defined and not written
//!
//! A getter stored under its key would be **returned** by a cached read rather
//! than called: compiled code emits `cached_get`, which loads the slot a layout
//! says the key is at. So the pair lives beside the cell in the runtime, out of
//! the layout entirely, and defining one is its own operation.
//!
//! That is the mirror of the decision an array's `length` needed. `length`
//! became a real property so the fast path and the runtime would agree; an
//! accessor must not be one, so that the fast path misses and the read reaches
//! the runtime at all.
//!
//! # Why this is not in `expr.rs`
//!
//! Because that file is at the thousand-line ceiling rule 8 sets, and because
//! the literal and the class body both define accessors — putting the shared
//! half here is what stops the two spellings from disagreeing about where the
//! pair is kept.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{call, emit_expr, gap, tagged};
use super::property::key_constant;
use super::{Ctx, EmitResult, Scope};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{Property, PropertyKey};

/// Emits one half of an accessor definition.
///
/// Shared by the object literal and the class body, which spell the same thing
/// two ways and must not disagree about it — the pair is kept beside the cell,
/// deliberately absent from the layout, and a second copy of that decision is
/// where one of them would write a property instead.
pub(super) fn define_accessor(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    object: ValueId,
    name: Name,
    function: ValueId,
    is_getter: bool,
) -> EmitResult<ValueId> {
    let key = key_constant(builder, ctx, name);
    let function = tagged(builder, function);
    let op = match is_getter {
        true => RuntimeOp::DefineGetter,
        false => RuntimeOp::DefineSetter,
    };
    Ok(call(builder, ctx, op, &[object, key, function])?[0])
}


/// Emits an object literal.
///
/// A fresh object, then one write per property, in source order. Not a shape
/// decided here and filled in: two objects built the same way reach the same
/// layout because they take the same transitions, and taking them is what the
/// writes do. Deciding a layout at the literal would be a second authority on
/// what an object's shape is, disagreeing with the runtime's the first time a
/// property was added after construction.
pub(super) fn emit_object(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    properties: &[Property],
) -> EmitResult<ValueId> {
    let object = call(builder, ctx, RuntimeOp::ObjectNew, &[])?[0];
    for property in properties {
        // A method is a function stored under a key, plus a **home object** —
        // which is what `super.x` inside it reads from. There is no `super`
        // yet, so the two are the same thing here; when there is, this becomes
        // the place that differs and the tree already records which was
        // written.
        let (key, value) = match property {
            Property::Value { key, value, .. } => {
                let value = emit_expr(builder, scope, ctx, value)?;
                (key, value)
            }
            Property::Method { key, function } => {
                let value = super::function::emit_closure(builder, scope, ctx, function)?;
                (key, value)
            }
            // An accessor is defined, not written: a getter stored under the
            // key would be returned by a cached read rather than called. The
            // computed spelling is refused because the definition takes the key
            // the compiler resolved, and a value would need a second entry
            // point that interns one.
            Property::Getter { key, function } | Property::Setter { key, function } => {
                let PropertyKey::Named(name) = key else {
                    return gap("a computed accessor name in an object literal");
                };
                let closure = super::function::emit_closure(builder, scope, ctx, function)?;
                let is_getter = matches!(property, Property::Getter { .. });
                define_accessor(builder, ctx, object, *name, closure, is_getter)?;
                continue;
            }
            // `{ ...source }` — the source's own enumerable properties, copied
            // in the order they are written. A getter on the source RUNS and
            // what lands is a plain data property, which is what the language
            // says a spread does and the difference from inheriting.
            Property::Spread(source) => {
                let source = emit_expr(builder, scope, ctx, source)?;
                let source = super::expr::as_value(builder, source);
                super::expr::call(builder, ctx, RuntimeOp::ObjectSpread, &[object, source])?;
                continue;
            }
            // `__proto__: v` in a literal SETS the prototype rather than adding
            // a property, and only in this spelling — the tree already made that
            // distinction, so nothing here has to re-decide it.
            Property::Prototype(value) => {
                let value = emit_expr(builder, scope, ctx, value)?;
                let value = super::expr::as_value(builder, value);
                super::expr::call(builder, ctx, RuntimeOp::SetPrototype, &[object, value])?;
                continue;
            }
        };
        let value = tagged(builder, value);

        match key {
            // The name is resolved while compiling, so the key crosses as the
            // number — which is the whole reason a written key and a computed
            // one are different operations rather than one taking a value.
            PropertyKey::Named(name) => {
                let key = key_constant(builder, ctx, *name);
                call(builder, ctx, RuntimeOp::SetProperty, &[object, key, value])?;
            }
            PropertyKey::Computed(expression) => {
                let key = emit_expr(builder, scope, ctx, expression)?;
                let key = tagged(builder, key);
                call(builder, ctx, RuntimeOp::SetIndexed, &[object, key, value])?;
            }
        }
    }
    Ok(object)
}
