//! The six members, and the proof that admits each. See the module header.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::super::expr::{emit_expr, name_constant, tagged};
use super::super::statics::Primordials;
use super::super::{Ctx, EmitResult, Scope};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind, Spreadable};

/// Which entry a member call reaches directly, by the member's spelling and how
/// many arguments were written, under `primordials` — or `None` for a call this
/// does not decide. Shared with the MIR stage so the two emitters admit the
/// same members.
/// The entry a member call compiles to, and whether its argument slots are
/// FIXED: a door with an `arity` takes that many values after the receiver,
/// `undefined` in the ones the program did not write, and then how many it did
/// — `f.call(t)` and `f.call(t, a, b, c)` are one entry. A door without one
/// takes exactly what was written.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Door {
    pub op: RuntimeOp,
    pub arity: Option<usize>,
}

/// The entry a NUMBER method compiles to when its receiver is a proven double:
/// the double itself, the one argument (`undefined` where none was written),
/// and no dispatch. Decided after the receiver is emitted, because it turns on
/// the receiver's representation; a receiver not proven takes the member.
pub(crate) fn number_door(primordials: Primordials, member: &str, written: usize) -> Option<RuntimeOp> {
    if !primordials.number {
        return None;
    }
    Some(match (member, written) {
        ("toString", 0 | 1) => RuntimeOp::NumberToStringDirect,
        ("toFixed", 1) => RuntimeOp::NumberToFixedDirect,
        _ => return None,
    })
}

pub(crate) fn shape_of(primordials: Primordials, member: &str, written: usize) -> Option<Door> {
    let exact = |op| Door { op, arity: None };
    Some(match (member, written) {
        ("get", 1) if primordials.map => exact(RuntimeOp::MapGetDirect),
        ("set", 2) if primordials.map => exact(RuntimeOp::MapSetDirect),
        // `has` is both classes' member, and the runtime's `Map.has` door
        // answers a set through its fallback exactly as its member would — but
        // only where BOTH classes are the language's, since a set's `has`
        // reached through the map door is the map's proof standing in for the
        // set's.
        ("has", 1) if primordials.map && primordials.set => exact(RuntimeOp::MapHasDirect),
        ("add", 1) if primordials.set => exact(RuntimeOp::SetAddDirect),
        ("push", 1) if primordials.array => exact(RuntimeOp::ArrayPushDirect),
        ("charCodeAt", 1) if primordials.string_base => exact(RuntimeOp::StringCharCodeAtDirect),
        // `f.call(thisArg, …)`: the receiver and up to three arguments, which is
        // what the convention carries without a vector. Wider calls stay calls.
        ("call", 1..=4) if primordials.function => Door {
            op: RuntimeOp::FunctionCallDirect,
            arity: Some(4),
        },
        ("apply", 2) if primordials.function => exact(RuntimeOp::FunctionApplyDirect),
        _ => return None,
    })
}

/// `recv.member(args)` as the direct entry, or `None` where the call stays one.
///
/// Decided before anything is emitted, from the spelling alone, so a refusal
/// leaves the caller to emit the call exactly as it would have. The receiver is
/// evaluated once, then the arguments, then the entry: the language reads the
/// member between the first two, which the module header says is observable
/// only where the read has an effect.
pub(in super::super) fn emit(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &callee.kind
    else {
        return Ok(None);
    };
    let member = ctx.names.text(*property);
    let Some(door) = shape_of(ctx.statics_primordial, member, arguments.len()) else {
        return typed(builder, scope, ctx, callee, object, *property, arguments);
    };
    let mut plain = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Spreadable::Single(argument) = argument else {
            return Ok(None);
        };
        plain.push(argument);
    }
    let receiver = emit_expr(builder, scope, ctx, object)?;
    let mut operands = vec![tagged(builder, receiver)];
    for argument in plain {
        let value = emit_expr(builder, scope, ctx, argument)?;
        operands.push(tagged(builder, value));
    }
    if let Some(arity) = door.arity {
        while operands.len() < 1 + arity {
            operands.push(super::super::expr::undefined(builder, ctx));
        }
        let written = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(arguments.len() as u64),
        });
        operands.push(builder.use_const(written));
    }
    let spelled = super::super::call::callee_spelling(ctx, callee);
    operands.push(name_constant(builder, spelled));
    Ok(Some(super::super::expr::call(builder, ctx, door.op, &operands)?[0]))
}

/// A number method over a receiver that turns out to be a proven double: the
/// direct entry. Over any other receiver the CALL the program wrote, with the
/// receiver already in hand — never evaluated twice.
fn typed(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    object: &Expr,
    property: crate::names::Name,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    let Some(door) = number_door(ctx.statics_primordial, ctx.names.text(property), arguments.len()) else {
        return Ok(None);
    };
    let mut plain = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Spreadable::Single(argument) = argument else {
            return Ok(None);
        };
        plain.push(argument);
    }
    let receiver = emit_expr(builder, scope, ctx, object)?;
    if builder.repr_of(receiver) == rts_cranelift::repr::Repr::F64 {
        let argument = match plain.first() {
            Some(argument) => {
                let value = emit_expr(builder, scope, ctx, argument)?;
                tagged(builder, value)
            }
            None => super::super::expr::undefined(builder, ctx),
        };
        return Ok(Some(super::super::expr::call(builder, ctx, door, &[receiver, argument])?[0]));
    }
    // Not proven: the member, read from the receiver already evaluated, then
    // the arguments, then the call -- the language's order.
    let receiver = tagged(builder, receiver);
    let function = super::super::property::emit_read(builder, ctx, receiver, property)?;
    let mut values = Vec::with_capacity(plain.len());
    for argument in plain {
        values.push(emit_expr(builder, scope, ctx, argument)?);
    }
    let name = super::super::call::callee_spelling(ctx, callee);
    Ok(Some(super::super::call::issue_as(
        builder,
        ctx,
        function,
        receiver,
        &values,
        name,
        RuntimeOp::Call,
    )?))
}
