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
use rts_cranelift::tags;

use super::{Ctx, EmitError, EmitResult, Scope, UNPROVEN};
use crate::runtime::RuntimeOp;
use crate::syntax::{AssignTarget, Expr, ExprKind, Literal};
use crate::syntax::{AssignOp, BinaryOp};
use crate::values::Singleton;

/// Materializes `undefined`.
///
/// Used by more than one caller — a function falling off its end, a `return`
/// with no operand, a `var` with no initialiser — which is why it is a function
/// rather than three copies of the same encoding.
pub fn undefined(builder: &mut FuncBuilder, ctx: &mut Ctx) -> ValueId {
    singleton(builder, ctx, Singleton::Undefined)
}

/// Materializes one of the language's singletons.
fn singleton(builder: &mut FuncBuilder, ctx: &mut Ctx, which: Singleton) -> ValueId {
    // The machine numbers singletons and this crate says what they mean, so the
    // id comes from the model rather than from a constant written here. A
    // literal `1` at this line would be the same bug the registry exists to
    // prevent.
    let id = ctx.model.singleton(which);
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
    ctx: &mut Ctx,
    expr: &Expr,
) -> EmitResult<ValueId> {
    match &expr.kind {
        ExprKind::Literal(literal) => emit_literal(builder, ctx, literal),

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
            let a = emit_expr(builder, scope, ctx, left)?;
            let b = emit_expr(builder, scope, ctx, right)?;
            emit_binary(builder, ctx, *op, a, b)
        }

        ExprKind::Sequence { operands } => {
            // Evaluate each, yield the last. The earlier values are not unused
            // by accident — the operator exists for their side effects.
            let mut last = None;
            for operand in operands {
                last = Some(emit_expr(builder, scope, ctx, operand)?);
            }
            last.ok_or(EmitError::Unsupported {
                construct: "an empty comma expression",
            })
        }

        ExprKind::Assign { target, value, op } => emit_assign(builder, scope, ctx, target, value, *op),

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

/// Calls a runtime operation.
///
/// Declaring on demand rather than up front: what a program does not do should
/// not appear in what it links, and a compilation that never concatenates
/// should carry no relocation to the string path.
fn call(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: RuntimeOp,
    args: &[ValueId],
) -> EmitResult<Vec<ValueId>> {
    let callee = ctx.calls.declare(ctx.funcs, op);
    Ok(builder.call(ctx.funcs, callee, args)?)
}

/// Turns a JavaScript value into the proven boolean a branch requires.
///
/// # Why this cannot be an instruction
///
/// Seven values are falsy and six of them a comparison settles. The seventh is
/// the empty string, and finding out whether a string is empty reads its length
/// from the heap — so truthiness is a call, and the machine's `branch` accepts
/// nothing but `Repr::Bool`.
///
/// That is the whole reason control flow could not be emitted before calls, and
/// it was found by reading the builder rather than by assuming: `branch`
/// answers `WrongDomain` for a tagged condition.
pub fn emit_condition(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    condition: &Expr,
) -> EmitResult<ValueId> {
    let value = emit_expr(builder, scope, ctx, condition)?;
    Ok(call(builder, ctx, RuntimeOp::ToBoolean, &[value])?[0])
}

/// Emits a literal.
fn emit_literal(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
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
        Literal::Singleton(which) => Ok(singleton(builder, ctx, *which)),
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
    ctx: &mut Ctx,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
) -> EmitResult<ValueId> {
    let runtime = match op {
        BinaryOp::Add => RuntimeOp::Add,

        // `-`, `*`, `/` and the four relational operators are refused rather
        // than emitted, and that is a REGRESSION from what E1 accepted. Stated
        // rather than quiet, because the rule is that a regression is allowed
        // and never silent.
        //
        // E1 emitted them as `Inst::Generic`, which reads as working and is not:
        // the machine refuses to lower a generic operation at all —
        //
        //     Inst::Generic(..) => Err(NotYetLowered { needs: Capability::Calls })
        //
        // — because which symbol a generic subtraction dials is a fact about
        // JavaScript, and the machine declines to know it. So what E1 produced
        // for these could pass the verifier and could never become machine
        // code, which no test caught because the tests stopped at the verifier.
        //
        // They come back when the runtime defines them. Refusing until then is
        // what keeps `runtime/` and `rts-core-rwk` from stating different sets —
        // the exact drift the audit named, since nothing links the two yet.
        BinaryOp::Sub => return gap("`-`"),
        BinaryOp::Mul => return gap("`*`"),
        BinaryOp::Div => return gap("`/`"),
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
            return gap("a relational operator");
        }

        // `===` is not `CmpOp::Eq` even though the spelling matches. Two
        // strings are `===` when their *text* is, which reads the heap, so it
        // is a call. `!==` is its negation and needs one more instruction than
        // exists here — negating a proven boolean is arithmetic, and this
        // module has no unary path yet.
        // The runtime answers `Repr::Bool` — it PROVED one, which is what
        // lets a branch consume it without a guard. But `a === b` written in an
        // expression is a JavaScript value, and the widening is what turns the
        // proof back into one.
        //
        // Found by running a program rather than by reading: `return 1 === 1`
        // returned the machine's raw 1 where the signature declared a tagged
        // value, so the caller read tag 0 — an inline integer — instead of a
        // boolean.
        BinaryOp::StrictEqual => {
            let proven = call(builder, ctx, RuntimeOp::StrictEquals, &[a, b])?[0];
            return Ok(builder.widen(proven));
        }
        BinaryOp::StrictNotEqual => return gap("`!==`"),
        BinaryOp::LooseEqual | BinaryOp::LooseNotEqual => return gap("`==` or `!=`"),
        BinaryOp::Rem => return gap("`%`"),
        BinaryOp::Exponent => return gap("`**`"),
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => return gap("a bitwise operator"),
        BinaryOp::Shl | BinaryOp::Shr | BinaryOp::UShr => return gap("a shift"),
        BinaryOp::In => return gap("`in`"),
        BinaryOp::InstanceOf => return gap("`instanceof`"),
    };
    Ok(call(builder, ctx, runtime, &[a, b])?[0])
}

/// Emits an assignment.
fn emit_assign(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
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
        AssignOp::Plain => emit_expr(builder, scope, ctx, value)?,
        AssignOp::Compound(binary) => {
            // `a += b` reads `a` once. The tree carries the operator rather
            // than a rewritten `a = a + b` precisely so that stays true, and
            // reading the binding here rather than re-emitting the target is
            // what honours it.
            let current = match scope.lookup(*name) {
                Some(super::scope::Binding::Value(current)) => current,
                None => return Err(EmitError::UnboundName(*name)),
            };
            let operand = emit_expr(builder, scope, ctx, value)?;
            emit_binary(builder, ctx, binary, current, operand)?
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
