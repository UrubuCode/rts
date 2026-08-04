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

use rts_cranelift::ir::inst::{CmpOp, NumOp};
use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::tags;

use super::{Ctx, EmitError, EmitResult, Scope, UNPROVEN};
use crate::runtime::RuntimeOp;
use crate::syntax::{AssignTarget, Expr, ExprKind, Literal, Property, PropertyKey};
use crate::syntax::{AssignOp, BinaryOp};
use crate::names::Name;
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
        ExprKind::Member {
            object, property, ..
        } => {
            // `property` is a name, not a key: `o[e]` is `Index`, a different
            // node. So there is no computed case to refuse here.
            let receiver = emit_expr(builder, scope, ctx, object)?;
            emit_read(builder, ctx, receiver, *property)
        }
        ExprKind::Index { .. } => gap("indexing"),
        ExprKind::Object { properties } => emit_object(builder, scope, ctx, properties),
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

/// Widens a value for a position that takes a JavaScript value.
pub fn as_value(builder: &mut FuncBuilder, value: ValueId) -> ValueId {
    tagged(builder, value)
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
    // Widened to match what the operation DECLARED, parameter by parameter —
    // not to tagged unconditionally.
    //
    // The first version widened everything, on the reasoning that a runtime
    // operation takes JavaScript values. That is true of every parameter except
    // the ones that are not values at all: a property key is a number the
    // compiler resolved, declared `I64`, and widening it produced a call the
    // machine refused — correctly, and with the position named.
    let expected = op.signature().params;
    let args: Vec<_> = args
        .iter()
        .zip(expected)
        .map(|(value, want)| {
            if want == UNPROVEN {
                tagged(builder, *value)
            } else {
                *value
            }
        })
        .collect();
    Ok(builder.call(ctx.funcs, callee, &args)?)
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
    // A proven boolean IS the answer: `ToBoolean` of a boolean is itself, so
    // asking the runtime buys nothing at all. It is also the common case —
    // `while (i < n)` and `if (a === b)` are how conditions get written.
    if builder.repr_of(value) == Repr::Bool {
        return Ok(value);
    }
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
        // Emitted as a PROVEN double, not as a tagged word. The bits are the
        // same — a NaN-boxed double IS the double — but the representation is
        // what an operator reads to decide between an instruction and a call,
        // so tagging at the literal throws away every proof that could have
        // started there.
        Literal::Number(value) => Ok(number_constant(builder, *value)),
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
    // The whole point of the pass. Two proven doubles turn every one of these
    // into a machine instruction, because the decision the runtime call exists
    // to make has exactly one answer once the operands are known.
    //
    // `+` included, and it is the one worth being explicit about: it is a call
    // in general BECAUSE it might concatenate, and proving both sides numeric
    // is precisely the evidence that it cannot.
    if builder.repr_of(a) == Repr::F64 && builder.repr_of(b) == Repr::F64 {
        match proven_binary(op) {
            Some(Proven::Arith(num)) => return Ok(builder.arith(num, a, b)?),
            // `===` on two numbers is IEEE equality, and that is not an
            // approximation of it: NaN !== NaN and +0 === -0 are what the
            // hardware comparison already answers.
            Some(Proven::Compare(cmp)) => return Ok(builder.compare(cmp, a, b)?),
            None => {}
        }
    }

    // Neither operand was proved, and the operator is one a pair of doubles
    // settles. So ASK: guard each side, and take the instruction when both
    // answer. The slow path is the call that would have been made anyway.
    //
    // This is what the type pass cannot reach. It proves things about locals,
    // and `o.n` is not one — nothing knows what an object holds. A guard needs
    // no such knowledge: it tests the value it actually got.
    if let Some(instruction) = proven_binary(op) {
        return emit_guarded(builder, ctx, op, instruction, a, b);
    }

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
        BinaryOp::Sub => RuntimeOp::Subtract,
        BinaryOp::Mul => RuntimeOp::Multiply,
        BinaryOp::Div => RuntimeOp::Divide,
        BinaryOp::Rem => RuntimeOp::Remainder,

        // The four relational operators answer a PROVEN boolean, for the same
        // reason `===` does, and so need the same widening back into a
        // JavaScript value. `a < b` in expression position is a value; the
        // proof is what a branch would want, and a branch gets it from
        // `to_boolean` instead.
        BinaryOp::Less => return Ok(compared(builder, ctx, RuntimeOp::Less, a, b)?),
        BinaryOp::LessEqual => return Ok(compared(builder, ctx, RuntimeOp::LessEqual, a, b)?),
        BinaryOp::Greater => return Ok(compared(builder, ctx, RuntimeOp::Greater, a, b)?),
        BinaryOp::GreaterEqual => {
            return Ok(compared(builder, ctx, RuntimeOp::GreaterEqual, a, b)?);
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
        BinaryOp::StrictEqual => return Ok(compared(builder, ctx, RuntimeOp::StrictEquals, a, b)?),
        BinaryOp::StrictNotEqual => return gap("`!==`"),
        BinaryOp::LooseEqual | BinaryOp::LooseNotEqual => return gap("`==` or `!=`"),
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
    // A property write is the other assignment target that exists today, and
    // it is handled before the local case because it does not go through the
    // scope at all: the binding is on the heap.
    if let ExprKind::Member {
        object, property, ..
    } = &place.kind
    {
        let AssignOp::Plain = op else {
            return gap("a compound assignment to a property");
        };
        // Receiver first, then the value: `a().x = b()` runs `a` before `b`.
        let receiver = emit_expr(builder, scope, ctx, object)?;
        let assigned = emit_expr(builder, scope, ctx, value)?;
        return emit_write(builder, ctx, receiver, *property, assigned);
    }

    let ExprKind::Ident(name) = &place.kind else {
        return gap("assigning to anything but a local or a property");
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

    let result = stored(builder, ctx, *name, result);
    if !scope.assign(*name, result) {
        return Err(EmitError::UnboundName(*name));
    }
    // An assignment is an expression: `x = (y = 1)` needs the inner one's
    // value, and it is the assigned value rather than the binding.
    Ok(result)
}

/// A comparison, as a JavaScript value.
///
/// The runtime answers `Repr::Bool` — it **proved** one, which is what lets a
/// branch consume it without a guard. But a comparison written in expression
/// position is a value, so the proof is widened back into one.
///
/// Found by running a program rather than by reading: `return 1 === 1` handed
/// back the machine's raw `1` where the signature declared a tagged value, and
/// the caller read tag 0 — an inline integer — as the answer. Shared by all five
/// comparisons so the next one cannot be added without it.
fn compared(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: RuntimeOp,
    a: ValueId,
    b: ValueId,
) -> EmitResult<ValueId> {
    let proven = call(builder, ctx, op, &[a, b])?[0];
    Ok(builder.widen(proven))
}

/// A number, as a value the machine knows is a number.
fn number_constant(builder: &mut FuncBuilder, value: f64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(value.to_bits()),
    });
    builder.use_const(id)
}

/// What a binding holds, given what was proved about its name.
///
/// A proved local keeps its machine representation, so the next operator that
/// reads it can be an instruction. Anything else is widened at the store, which
/// is what makes the representation of a binding a property of the NAME rather
/// than of whichever value reached it last — and that is what a merge needs,
/// since two paths writing different representations into one name is a program
/// the machine refuses to build.
pub fn stored(builder: &mut FuncBuilder, ctx: &Ctx, name: Name, value: ValueId) -> ValueId {
    if ctx.holds_number(name) {
        value
    } else {
        tagged(builder, value)
    }
}

/// A value as JavaScript sees it.
///
/// Widening where a proof stops being useful: a `return`, a runtime call's
/// argument, a binding nothing proved. The machine inserts nothing when the
/// value is already generic, so this costs a comparison while compiling and
/// nothing at run time for values that were never proved.
fn tagged(builder: &mut FuncBuilder, value: ValueId) -> ValueId {
    builder.widen(value)
}

/// What a proven pair of doubles turns an operator into.
enum Proven {
    /// A machine arithmetic instruction.
    Arith(NumOp),
    /// A machine comparison, which yields a proven boolean.
    Compare(CmpOp),
}

/// The instruction an operator becomes when both operands are proven doubles.
///
/// `None` for the ones that stay calls whatever is known: `==` has its own
/// conversion table, `in` and `instanceof` ask the heap, and the bitwise
/// operators are not emitted at all yet.
fn proven_binary(op: BinaryOp) -> Option<Proven> {
    Some(match op {
        BinaryOp::Add => Proven::Arith(NumOp::Add),
        BinaryOp::Sub => Proven::Arith(NumOp::Sub),
        BinaryOp::Mul => Proven::Arith(NumOp::Mul),
        BinaryOp::Div => Proven::Arith(NumOp::Div),
        // `%` has no machine instruction here: the code generator's numeric
        // set is add, subtract, multiply and divide, and a remainder on
        // doubles is a library call on most targets anyway. It stays a runtime
        // call, which is correct rather than a gap.
        BinaryOp::Less => Proven::Compare(CmpOp::Lt),
        BinaryOp::LessEqual => Proven::Compare(CmpOp::Le),
        BinaryOp::Greater => Proven::Compare(CmpOp::Gt),
        BinaryOp::GreaterEqual => Proven::Compare(CmpOp::Ge),
        BinaryOp::StrictEqual => Proven::Compare(CmpOp::Eq),
        BinaryOp::StrictNotEqual => Proven::Compare(CmpOp::Ne),
        _ => return None,
    })
}

/// The number a property name has, as a machine constant.
///
/// An integer rather than a tagged value: it is not a JavaScript value at all,
/// it is which name the compiler resolved. Emitting it tagged would be claiming
/// the program could compute it, which is exactly what a computed property does
/// and what this path is not.
fn key_constant(builder: &mut FuncBuilder, ctx: &mut Ctx, name: Name) -> ValueId {
    let key = ctx.key_of(name);
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(u64::from(key)),
    });
    builder.use_const(id)
}

/// Emits an object literal.
///
/// A fresh object, then one write per property, in source order. Not a shape
/// decided here and filled in: two objects built the same way reach the same
/// layout because they take the same transitions, and taking them is what the
/// writes do. Deciding a layout at the literal would be a second authority on
/// what an object's shape is, disagreeing with the runtime's the first time a
/// property was added after construction.
fn emit_object(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    properties: &[Property],
) -> EmitResult<ValueId> {
    let object = call(builder, ctx, RuntimeOp::ObjectNew, &[])?[0];
    for property in properties {
        let Property::Value { key, value, .. } = property else {
            return gap("a method, getter, setter, spread or `__proto__` in an object literal");
        };
        let PropertyKey::Named(name) = key else {
            return gap("a computed key in an object literal");
        };
        let key = key_constant(builder, ctx, *name);
        let value = emit_expr(builder, scope, ctx, value)?;
        let value = tagged(builder, value);
        call(builder, ctx, RuntimeOp::SetProperty, &[object, key, value])?;
    }
    Ok(object)
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
fn emit_read(
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

/// An operator over operands nothing proved, taking the instruction when they
/// turn out to be numbers.
///
/// ```text
///   guard a is a double ── not one ──┐
///          │                         │
///   guard b is a double ── not one ──┤
///          │                         │
///     instruction                  slow: the call
///          │                         │
///          └──────► join(value) ◄────┘
/// ```
///
/// # Why two guards and not one test of both
///
/// Because a guard narrows, and narrowing is what makes the instruction legal.
/// A test that answered "both are doubles" without producing the two narrowed
/// values would leave the operands generic, and `arith` refuses those — which is
/// the refusal that makes this layer worth having.
///
/// # What it costs when the guess is wrong
///
/// Two compares and a branch, then the call that would have happened anyway.
/// A program whose operands are never numbers pays that and nothing else; the
/// guard cannot make the slow path slower than it was.
fn emit_guarded(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: BinaryOp,
    instruction: Proven,
    a: ValueId,
    b: ValueId,
) -> EmitResult<ValueId> {
    let a = tagged(builder, a);
    let b = tagged(builder, b);

    let left_is_number = builder.create_block();
    let left = builder.add_block_param(left_is_number, Repr::F64);
    let both = builder.create_block();
    let right = builder.add_block_param(both, Repr::F64);
    let slow = builder.create_block();
    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);

    builder.guard(a, Repr::F64, (left_is_number, &[]), (slow, &[]))?;

    builder.switch_to(left_is_number);
    builder.guard(b, Repr::F64, (both, &[]), (slow, &[]))?;

    builder.switch_to(both);
    let fast = match instruction {
        Proven::Arith(num) => builder.arith(num, left, right)?,
        Proven::Compare(cmp) => builder.compare(cmp, left, right)?,
    };
    let fast = tagged(builder, fast);
    builder.jump(join, &[fast])?;

    builder.switch_to(slow);
    let runtime = runtime_binary(op).expect("every instruction has a runtime operation");
    let answered = call(builder, ctx, runtime, &[a, b])?[0];
    let answered = tagged(builder, answered);
    builder.jump(join, &[answered])?;

    builder.switch_to(join);
    Ok(result)
}

/// The runtime operation an operator falls back to.
///
/// Every operator `proven_binary` names has one, which is why this cannot fail
/// for a caller that asked that question first — and why the two lists are
/// beside each other rather than in different files.
fn runtime_binary(op: BinaryOp) -> Option<RuntimeOp> {
    Some(match op {
        BinaryOp::Add => RuntimeOp::Add,
        BinaryOp::Sub => RuntimeOp::Subtract,
        BinaryOp::Mul => RuntimeOp::Multiply,
        BinaryOp::Div => RuntimeOp::Divide,
        BinaryOp::Rem => RuntimeOp::Remainder,
        BinaryOp::Less => RuntimeOp::Less,
        BinaryOp::LessEqual => RuntimeOp::LessEqual,
        BinaryOp::Greater => RuntimeOp::Greater,
        BinaryOp::GreaterEqual => RuntimeOp::GreaterEqual,
        BinaryOp::StrictEqual => RuntimeOp::StrictEquals,
        _ => return None,
    })
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
fn emit_write(
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
