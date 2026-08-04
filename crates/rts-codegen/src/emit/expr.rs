//! Expressions, in the machine's representation.
//!
//! # Why every operator here is generic
//!
//! `a + b` in JavaScript is not addition. It is: evaluate both, convert both to
//! primitives, and then *either* concatenate strings *or* add numbers depending
//! on what came back — a decision made at run time from values, not at compile
//! time from syntax. `a < b` is worse: it compares as text when both sides are
//! strings and numerically otherwise, and it evaluates its operands
//! left-to-right while converting them right-to-left for one of the four
//! spellings.
//!
//! So the machine's `GenericOp` is not a fallback taken because the type pass is
//! missing. It is the *correct* emission for an operator whose meaning depends
//! on values, and it would still be correct with a type pass that failed to
//! prove anything about this particular site. What the type pass adds is the
//! ability to emit `arith` instead when it can defend the claim.
//!
//! # What is refused, and why refusing beats approximating
//!
//! A string literal is a heap value: two occurrences of `"a"` in a program are
//! the same string, which is interning, which is a runtime entry point. Emitting
//! one as an immediate would produce a value that is not a string and compares
//! wrongly with everything. So it is named as a gap.
//!
//! The same reasoning covers calls, objects, member access and closures. Each is
//! a mechanism this module does not have yet rather than a shortcut it declined
//! to take.

use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::ir::inst::{CmpOp, GenericOp};
use rts_cranelift::tags;

use super::{EmitError, EmitResult, Scope, UNPROVEN};
use crate::syntax::{AssignTarget, Expr, ExprKind, Literal};
use crate::syntax::{AssignOp, BinaryOp};
use crate::values::{Singleton, ValueModel};

/// Materializes `undefined`.
///
/// Used by more than one caller — a function falling off its end, a `return`
/// with no operand, a `var` with no initialiser — which is why it is a function
/// rather than three copies of the same encoding.
pub fn undefined(builder: &mut FuncBuilder, model: &ValueModel) -> ValueId {
    singleton(builder, model, Singleton::Undefined)
}

/// Materializes one of the language's singletons.
fn singleton(builder: &mut FuncBuilder, model: &ValueModel, which: Singleton) -> ValueId {
    // The machine numbers singletons and this crate says what they mean, so the
    // id comes from the model rather than from a constant written here. A
    // literal `1` at this line would be the same bug the registry exists to
    // prevent.
    let id = model.singleton(which);
    constant(builder, id.word())
}

/// Materializes an already-encoded word.
fn constant(builder: &mut FuncBuilder, bits: u64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: UNPROVEN,
        bits: ScalarBits(bits),
    });
    builder.use_const(id)
}

/// Emits an expression, yielding the value it produces.
pub fn emit_expr(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    model: &ValueModel,
    expr: &Expr,
) -> EmitResult<ValueId> {
    match &expr.kind {
        ExprKind::Literal(literal) => emit_literal(builder, model, literal),

        ExprKind::Ident(name) => match scope.lookup(*name) {
            Some(super::scope::Binding::Value(value)) => Ok(value),
            // Not a gap. The construct is emitted; the program is wrong, or the
            // name is a global — and globals are a mechanism this module does
            // not have, which is a different sentence from "identifiers are not
            // supported".
            None => Err(EmitError::UnboundName(*name)),
        },

        ExprKind::Binary { op, left, right } => {
            // Left before right, unconditionally. Every JavaScript binary
            // operator evaluates its operands in source order even where it
            // then converts them in the other order, and emitting in the wrong
            // order changes which side effect happens first.
            let a = emit_expr(builder, scope, model, left)?;
            let b = emit_expr(builder, scope, model, right)?;
            emit_binary(builder, *op, a, b)
        }

        ExprKind::Sequence { operands } => {
            // Evaluate each, yield the last. The earlier values are not unused
            // by accident — the operator exists for their side effects.
            let mut last = None;
            for operand in operands {
                last = Some(emit_expr(builder, scope, model, operand)?);
            }
            last.ok_or(EmitError::Unsupported {
                construct: "an empty comma expression",
            })
        }

        ExprKind::Assign { target, value, op } => emit_assign(builder, scope, model, target, value, *op),

        // Every remaining form, named. The list is the deliverable: it is the
        // work queue for the phases after this one, and a reader can check it
        // against `PLAN.md` §E without running anything.
        ExprKind::Call { .. } => gap("a call"),
        ExprKind::New { .. } => gap("`new`"),
        ExprKind::Member { .. } => gap("member access"),
        ExprKind::Index { .. } => gap("indexing"),
        ExprKind::Object { .. } => gap("an object literal"),
        ExprKind::Array { .. } => gap("an array literal"),
        ExprKind::Function(_) => gap("a function expression"),
        ExprKind::Class(_) => gap("a class expression"),
        ExprKind::Unary { .. } => gap("a unary operator"),
        ExprKind::Update { .. } => gap("`++` or `--`"),
        ExprKind::Logical { .. } => gap("`&&`, `||` or `??`"),
        ExprKind::Conditional { .. } => gap("`?:`"),
        ExprKind::This => gap("`this`"),
        ExprKind::Await(_) => gap("`await`"),
        ExprKind::Yield { .. } => gap("`yield`"),
        ExprKind::Template { .. } => gap("a template literal"),
        ExprKind::TaggedTemplate { .. } => gap("a tagged template"),
        ExprKind::Chain(_) => gap("optional chaining"),
        ExprKind::SuperMember { .. } => gap("`super.x`"),
        ExprKind::SuperCall { .. } => gap("`super()`"),
        ExprKind::PrivateName(_) => gap("a private name"),
        ExprKind::NewTarget => gap("`new.target`"),
        ExprKind::ImportMeta => gap("`import.meta`"),
        ExprKind::ImportCall { .. } => gap("`import()`"),
        ExprKind::Asserted { .. } => gap("a type assertion"),
    }
}

/// A named gap.
fn gap<T>(construct: &'static str) -> EmitResult<T> {
    Err(EmitError::Unsupported { construct })
}

/// Emits a literal.
fn emit_literal(
    builder: &mut FuncBuilder,
    model: &ValueModel,
    literal: &Literal,
) -> EmitResult<ValueId> {
    match literal {
        // Every JavaScript number is a double, including the ones that look
        // like integers. Emitting `1` as a tagged int32 would be a narrowing
        // this module has not proved is safe — `1` and `1.0` are the same
        // value and the encoding must not decide otherwise here.
        Literal::Number(value) => Ok(constant(builder, tags::encode_double(*value))),
        Literal::Boolean(value) => {
            let payload = if *value { tags::BOOL_TRUE } else { tags::BOOL_FALSE };
            Ok(constant(builder, tags::encode(tags::TAG_BOOL, payload)))
        }
        Literal::Singleton(which) => Ok(singleton(builder, model, *which)),
        // A string literal is a heap value that two occurrences share, which is
        // interning, which is a runtime entry point. An immediate here would
        // produce something that is not a string.
        Literal::String(_) => gap("a string literal"),
    }
}

/// Emits a binary operator.
///
/// # Why the comparisons are not simply `CmpOp`
///
/// They are `GenericOp::Compare(CmpOp)`, which is a different instruction from
/// `compare`. The machine's `compare` is a proven comparison over operands of a
/// known representation; `<` in JavaScript compares text when both sides are
/// strings. Reaching for `compare` because the spelling matches is exactly the
/// mistake rule 2 names.
fn emit_binary(
    builder: &mut FuncBuilder,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
) -> EmitResult<ValueId> {
    let generic = match op {
        BinaryOp::Add => GenericOp::Add,
        BinaryOp::Sub => GenericOp::Sub,
        BinaryOp::Mul => GenericOp::Mul,
        BinaryOp::Div => GenericOp::Div,
        BinaryOp::Less => GenericOp::Compare(CmpOp::Lt),
        BinaryOp::LessEqual => GenericOp::Compare(CmpOp::Le),
        BinaryOp::Greater => GenericOp::Compare(CmpOp::Gt),
        BinaryOp::GreaterEqual => GenericOp::Compare(CmpOp::Ge),

        // `===` is deliberately absent from this list even though `CmpOp::Eq`
        // exists. Strict equality is one of three equalities that differ in
        // exactly two cells, and two strings are `===` when their *text* is —
        // which needs the heap. It is a runtime entry point (`CoreEntry::
        // StrictEquals`), not a comparison instruction, and calls are the next
        // phase.
        BinaryOp::StrictEqual | BinaryOp::StrictNotEqual => return gap("`===` or `!==`"),
        BinaryOp::LooseEqual | BinaryOp::LooseNotEqual => return gap("`==` or `!=`"),
        BinaryOp::Rem => return gap("`%`"),
        BinaryOp::Exponent => return gap("`**`"),
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => return gap("a bitwise operator"),
        BinaryOp::Shl | BinaryOp::Shr | BinaryOp::UShr => return gap("a shift"),
        BinaryOp::In => return gap("`in`"),
        BinaryOp::InstanceOf => return gap("`instanceof`"),
    };
    Ok(builder.generic(generic, a, b))
}

/// Emits an assignment.
fn emit_assign(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    model: &ValueModel,
    target: &AssignTarget,
    value: &Expr,
    op: AssignOp,
) -> EmitResult<ValueId> {
    let AssignTarget::Place(place) = target else {
        return gap("destructuring assignment");
    };
    let ExprKind::Ident(name) = &place.kind else {
        return gap("assigning to anything but a local");
    };

    let result = match op {
        AssignOp::Plain => emit_expr(builder, scope, model, value)?,
        AssignOp::Compound(binary) => {
            // `a += b` reads `a` once. The tree carries the operator rather
            // than a rewritten `a = a + b` precisely so that stays true, and
            // reading the binding here rather than re-emitting the target is
            // what honours it.
            let current = match scope.lookup(*name) {
                Some(super::scope::Binding::Value(current)) => current,
                None => return Err(EmitError::UnboundName(*name)),
            };
            let operand = emit_expr(builder, scope, model, value)?;
            emit_binary(builder, binary, current, operand)?
        }
        // `&&=` does not evaluate its right side when the target already
        // decided, which needs a branch.
        AssignOp::Logical(_) => return gap("`&&=`, `||=` or `??=`"),
    };

    if !scope.assign(*name, result) {
        return Err(EmitError::UnboundName(*name));
    }
    // An assignment is an expression: `x = (y = 1)` needs the inner one's
    // value, and it is the assigned value rather than the binding.
    Ok(result)
}
