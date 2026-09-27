//! The members, and the proof that admits each. See the module header.

use rts_cranelift::ir::{FuncBuilder, ValueId};
use rts_cranelift::repr::Repr;

use super::super::expr::{emit_expr, tagged};
use super::super::{Ctx, EmitResult, Scope};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind, Spreadable};

/// Which of the well-known names the whole program leaves as the language
/// defines them. Computed once, before anything is emitted — `primordial`'s
/// reason — and handed to the MIR stage unchanged.
#[derive(Clone, Copy, Default, Debug)]
pub struct Primordials {
    /// `Number` never leaves a member base and is never written.
    pub number: bool,
    /// `Array`, the same.
    pub array: bool,
    /// `Object`, the same.
    pub object: bool,
    /// The global `isNaN` is never written or shadowed at the top level.
    pub is_nan: bool,
    /// The global `isFinite`, the same.
    pub is_finite: bool,
}

/// How a call is answered.
pub(crate) enum Shape {
    /// `Number.isNaN` and its three siblings, over one proven double.
    IsNaN,
    /// See [`Shape::IsNaN`].
    IsFinite,
    /// See [`Shape::IsNaN`].
    IsInteger,
    /// See [`Shape::IsNaN`].
    IsSafeInteger,
    /// `Array.isArray(x)`: the direct entry, over anything.
    IsArray,
    /// `Object.is(a, b)`: `SameValue`, the direct entry, over anything.
    Is,
}

/// The shape a callee spelling has under `primordials`, or `None` for a call
/// this does not decide. The whole decision, shared with the MIR stage.
pub(crate) fn shape_of(
    primordials: Primordials,
    object: Option<&str>,
    member: &str,
    written: usize,
    shadowed: impl Fn(&str) -> bool,
) -> Option<Shape> {
    Some(match (object, member, written) {
        (Some("Number"), _, 1) if primordials.number && !shadowed("Number") => match member {
            "isNaN" => Shape::IsNaN,
            "isFinite" => Shape::IsFinite,
            "isInteger" => Shape::IsInteger,
            "isSafeInteger" => Shape::IsSafeInteger,
            _ => return None,
        },
        (Some("Array"), "isArray", 1) if primordials.array && !shadowed("Array") => Shape::IsArray,
        (Some("Object"), "is", 2) if primordials.object && !shadowed("Object") => Shape::Is,
        // The GLOBAL predicates convert first — `isNaN("abc")` is true — which a
        // proven double has already paid, so over one they are the same
        // instruction as `Number.isNaN`.
        (None, "isNaN", 1) if primordials.is_nan && !shadowed("isNaN") => Shape::IsNaN,
        (None, "isFinite", 1) if primordials.is_finite && !shadowed("isFinite") => Shape::IsFinite,
        _ => return None,
    })
}

/// `Number.isNaN(x)`, `Array.isArray(x)`, `Object.is(a, b)`, `isNaN(x)`,
/// `isFinite(x)` as instructions or a direct call, or `None` where the call
/// stays a call.
///
/// The three conditions are `emit/math`'s: the name proven the language's over
/// the whole program, unshadowed here, and — for the four predicates that are
/// instructions — every operand already a proven double. `Array.isArray` and
/// `Object.is` take any operand: the entry they reach is the member's own body
/// with the path removed. A refusal after the operands were emitted finishes
/// the call over them, for the reason `emit/math` gives: emitting them twice
/// evaluated them twice.
pub(in super::super) fn emit(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    let (object, member) = match &callee.kind {
        ExprKind::Member {
            object,
            property,
            optional: false,
        } => match &object.kind {
            ExprKind::Ident(name) => (Some(ctx.names.text(*name)), ctx.names.text(*property)),
            _ => return Ok(None),
        },
        ExprKind::Ident(name) => (None, ctx.names.text(*name)),
        _ => return Ok(None),
    };
    let shadowed = |spelled: &str| {
        ctx.names
            .find(spelled)
            .is_some_and(|name| scope.lookup(name).is_some())
    };
    let Some(shape) = shape_of(ctx.statics_primordial, object, member, arguments.len(), shadowed)
    else {
        return Ok(None);
    };
    let member_form = object.is_some();
    let mut values = Vec::with_capacity(2);
    for argument in arguments {
        let Spreadable::Single(argument) = argument else {
            return Ok(None);
        };
        values.push(emit_expr(builder, scope, ctx, argument)?);
    }
    let answered = match shape {
        Shape::IsArray => {
            let held = tagged(builder, values[0]);
            Some(super::super::expr::call(builder, ctx, RuntimeOp::ArrayIsArray, &[held])?[0])
        }
        Shape::Is => {
            let left = tagged(builder, values[0]);
            let right = tagged(builder, values[1]);
            Some(super::super::expr::call(builder, ctx, RuntimeOp::SameValue, &[left, right])?[0])
        }
        // `Number.isNaN(x)` over a value nothing proved is `!(x === x)` EXACTLY: NaN is the
        // one value strictly unequal to itself, and a non-number is never NaN. The
        // runtime's `===` is one crossing where the member was three, and it needs no
        // proof about the operand. The MEMBER only: the global `isNaN` converts first,
        // so `isNaN("abc")` is true, and over an unproven operand it stays a call.
        Shape::IsNaN if member_form && builder.repr_of(values[0]) != Repr::F64 => {
            let held = tagged(builder, values[0]);
            let same =
                super::super::expr::call(builder, ctx, RuntimeOp::StrictEquals, &[held, held])?[0];
            let no = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::Bool,
                bits: rts_cranelift::ir::ScalarBits(0),
            });
            let no = builder.use_const(no);
            Some(builder.compare(rts_cranelift::ir::CmpOp::Eq, same, no)?)
        }
        _ if builder.repr_of(values[0]) != Repr::F64 => None,
        Shape::IsNaN => Some(super::sequence::is_nan(builder, values[0])?),
        Shape::IsFinite => Some(super::sequence::is_finite(builder, values[0])?),
        Shape::IsInteger => Some(super::sequence::is_integer(builder, values[0])?),
        Shape::IsSafeInteger => Some(super::sequence::is_safe_integer(builder, values[0])?),
    };
    if let Some(answered) = answered {
        return Ok(Some(tagged(builder, answered)));
    }
    let (receiver, function) = super::super::call::callee_and_receiver(builder, scope, ctx, callee)?;
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
