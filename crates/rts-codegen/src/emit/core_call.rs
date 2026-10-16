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
    let Some(&bound) = ctx.core_imports.get(name) else {
        return Ok(None);
    };
    if scope.lookup(*name).is_some() {
        return Ok(None);
    }
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
            let answered = super::expr::call(builder, ctx, RuntimeOp::SysMemory, &[which])?[0];
            return Ok(Some(tagged(builder, answered)));
        }
    };
    let arity = match primitive {
        CorePrimitive::I32Mul => 2,
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
        _ => unreachable!("arity match above refused the rest"),
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
