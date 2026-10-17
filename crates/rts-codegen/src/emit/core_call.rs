//! A call of a name imported from `"rts:core"`, in the running emitter.
//!
//! The MIR stage's twin is `lower/intrinsic.rs::core_intrinsic`; both read
//! `runtime::core_names`, so the two cannot disagree about which names exist.
//! An `import` of `rts:core` emits nothing (`module.rs`), so a call that is not
//! handled by one of the two would reach no module at run time: that is why a
//! bound name used as anything but a call is refused (`binding::lexical_read`).

use rts_corelib::CorePrimitive;
use rts_cranelift::ir::{FuncBuilder, ValueId};
use rts_cranelift::repr::Repr;

use super::expr::{emit_expr, tagged};
use crate::runtime::RuntimeOp;
use crate::runtime::core_names::Bound;
use super::{Ctx, EmitResult, Scope};
use crate::syntax::{Expr, ExprKind, Spreadable};

pub(super) fn emit(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    let ExprKind::Ident(name) = &callee.kind else {
        return Ok(None);
    };
    // A same-module `@intrinsic` function: a real declaration (so the name IS in scope),
    // proved unshadowed by `register_intrinsics` (declared once, never written).
    let bound = if let Some(&primitive) = ctx.intrinsic_fns.get(name) {
        Bound::Core(primitive)
    } else {
        let Some(&bound) = ctx.core_imports.get(name) else {
            return Ok(None);
        };
        if scope.lookup(*name).is_some() {
            return Ok(None);
        }
        bound
    };
    let primitive = match bound {
        Bound::Core(primitive) => primitive,
        Bound::Sys(sys) => {
            if arguments.len() != sys.arity() {
                return Err(super::module::refuse("an rts:sys call with the wrong number of arguments"));
            }
            if let Some(text) = sys.constant() {
                let units: Vec<u16> = text.encode_utf16().collect();
                return Ok(Some(super::expr::string_literal_units(builder, ctx, &units)?));
            }
            let id = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::I64,
                bits: rts_cranelift::ir::ScalarBits(sys.selector() as u64),
            });
            let which = builder.use_const(id);
            if sys.is_text() {
                // Already a tagged string value: no boxing.
                return Ok(Some(super::expr::call(builder, ctx, RuntimeOp::SysText, &[which])?[0]));
            }
            let answered = super::expr::call(builder, ctx, RuntimeOp::SysMemory, &[which])?[0];
            return Ok(Some(tagged(builder, answered)));
        }
    };
    let arity = match primitive {
        CorePrimitive::I32Mul | CorePrimitive::Min | CorePrimitive::Max => 2,
        CorePrimitive::Clz32
        | CorePrimitive::Fround
        | CorePrimitive::Sqrt
        | CorePrimitive::Floor
        | CorePrimitive::Ceil
        | CorePrimitive::Trunc
        | CorePrimitive::Abs
        | CorePrimitive::Sign
        | CorePrimitive::Round => 1,
        // The library-call members: the same door `Math.<name>` takes (`math_direct`).
        other if other.math_direct_name().is_some_and(|name| crate::runtime::math_direct::binary_index(name).is_some()) => 2,
        other if other.math_direct_name().is_some() => 1,
        _ => return Err(super::module::refuse("an rts:core primitive with no lowering")),
    };
    if arguments.len() != arity {
        return Err(super::module::refuse("an rts:core call with the wrong number of arguments"));
    }
    let mut doubles = Vec::with_capacity(arity);
    for argument in arguments {
        let Spreadable::Single(argument) = argument else {
            return Err(super::module::refuse("a spread argument in an rts:core call"));
        };
        let value = emit_expr(builder, scope, ctx, argument)?;
        // ToNumber, then into the double domain: a number has two encodings.
        let number = super::unary::to_number(builder, ctx, value)?;
        doubles.push(as_double(builder, number)?);
    }
    let answered = match primitive {
        CorePrimitive::I32Mul => super::math::sequence::imul(builder, doubles[0], doubles[1])?,
        CorePrimitive::Clz32 => super::math::sequence::clz32(builder, doubles[0])?,
        CorePrimitive::Fround => builder.float_unary(rts_cranelift::ir::FloatOp::RoundToSingle, doubles[0])?,
        // The same instructions `emit/math/body.rs::shape_of` picks for `Math.<name>`.
        CorePrimitive::Sqrt => builder.float_unary(rts_cranelift::ir::FloatOp::Sqrt, doubles[0])?,
        CorePrimitive::Floor => builder.float_unary(rts_cranelift::ir::FloatOp::Floor, doubles[0])?,
        CorePrimitive::Ceil => builder.float_unary(rts_cranelift::ir::FloatOp::Ceil, doubles[0])?,
        CorePrimitive::Trunc => builder.float_unary(rts_cranelift::ir::FloatOp::Trunc, doubles[0])?,
        CorePrimitive::Abs => builder.float_unary(rts_cranelift::ir::FloatOp::Abs, doubles[0])?,
        CorePrimitive::Sign => super::math::sequence::sign(builder, doubles[0])?,
        CorePrimitive::Round => super::math::sequence::round(builder, doubles[0])?,
        CorePrimitive::Min => builder.arith(rts_cranelift::ir::NumOp::Min, doubles[0], doubles[1])?,
        CorePrimitive::Max => builder.arith(rts_cranelift::ir::NumOp::Max, doubles[0], doubles[1])?,
        other => {
            // `Math.f(…)` over doubles: `MathDirect1`/`2` with the member's number.
            let name = other.math_direct_name().expect("arity match above refused the rest");
            let (op, which, count) = match crate::runtime::math_direct::unary_index(name) {
                Some(which) => (RuntimeOp::MathDirect1, which, 1),
                None => (RuntimeOp::MathDirect2, crate::runtime::math_direct::binary_index(name).expect("a direct member"), 2),
            };
            let id = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::I64,
                bits: rts_cranelift::ir::ScalarBits(which as u64),
            });
            let which = builder.use_const(id);
            let mut args = vec![which];
            args.extend_from_slice(&doubles[..count]);
            super::expr::call(builder, ctx, op, &args)?[0]
        }
    };
    Ok(Some(tagged(builder, answered)))
}

/// A number the emitter holds either unboxed or tagged, as a double. A tagged
/// word that is neither a double nor an int32 traps, as `machine::unbox_number`
/// does: `ToNumber` already ran, so it cannot be anything else.
fn as_double(builder: &mut FuncBuilder, held: ValueId) -> EmitResult<ValueId> {
    match builder.repr_of(held) {
        Repr::F64 => return Ok(held),
        Repr::I32 => return Ok(builder.to_f64(held)?),
        _ => {}
    }
    let join = builder.create_block();
    let result = builder.add_block_param(join, Repr::F64);
    let double = builder.create_block();
    let as_dbl = builder.add_block_param(double, Repr::F64);
    let not_double = builder.create_block();
    builder.guard(held, Repr::F64, (double, &[]), (not_double, &[]))?;
    builder.switch_to(double);
    builder.jump(join, &[as_dbl])?;
    builder.switch_to(not_double);
    let integer = builder.create_block();
    let as_int = builder.add_block_param(integer, Repr::I32);
    let neither = builder.create_block();
    builder.guard(held, Repr::I32, (integer, &[]), (neither, &[]))?;
    builder.switch_to(integer);
    let widened = builder.to_f64(as_int)?;
    builder.jump(join, &[widened])?;
    builder.switch_to(neither);
    builder.trap(rts_cranelift::ir::TrapCode::Unreachable);
    builder.switch_to(join);
    Ok(result)
}
