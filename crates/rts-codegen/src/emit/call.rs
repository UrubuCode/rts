//! Calling something.
//!
//! # What a call site decides, and what it must not
//!
//! It decides two things the callee cannot: what `this` is, and how many
//! arguments were written. Everything else — whether the callee is code at all,
//! which code, and what happens when it is not — belongs to the runtime, and
//! this module deliberately does not look.
//!
//! # `this` comes from the *spelling*, not from the value
//!
//! `f(a)` and `o.f(a)` pass the same arguments to the same function and differ
//! only in the receiver, and which one was written is a syntactic fact this is
//! the last place that still knows. `o.f()` is the only form that has a
//! receiver; everything else passes `undefined`.
//!
//! That is why a member call is not "emit the callee, then call it": the
//! receiver has to be kept, and it has to be evaluated **once**. `f().g()` calls
//! `f` a single time, and an implementation that emitted the object expression
//! for the read and again for the receiver would call it twice — invisibly, for
//! every receiver without a side effect.
//!
//! # Why the arity is padded here
//!
//! Because the callee has a fixed shape and the call does not know which
//! function it will reach. A call passing three arguments to a function that
//! declared one is ordinary JavaScript, so the missing slots are filled with
//! `undefined` at the site rather than being the callee's problem — the callee
//! cannot have a problem, since its parameters exist whether or not anything
//! was passed.

use rts_cranelift::ir::FloatOp;
use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

use super::expr::{self, emit_expr};
use super::{Ctx, EmitError, EmitResult, Scope};
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{Expr, ExprKind, Spreadable};

/// Emits a call.
pub fn emit_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    // A DIRECT `eval`, which is a syntactic form rather than a value and so has
    // to be recognised here, before the callee becomes an ordinary expression.
    if let Some(value) = direct_eval(builder, scope, ctx, callee, arguments)? {
        return Ok(value);
    }

    // One machine instruction, when the whole program proves the name still
    // means what it means and the argument is already a proven double.
    if let Some(value) = machine_operation(builder, scope, ctx, callee, arguments)? {
        return Ok(value);
    }

    // No call at all, when the whole program proves which function this is and
    // that function is one expression. Asked before the callee is emitted:
    // reading the name would be the one piece of the call this removes.
    if let Some(value) = super::inline::emit_substituted(builder, scope, ctx, callee, arguments)? {
        return Ok(value);
    }

    // The receiver and the callee, in the order the language evaluates them:
    // the member expression first, then the arguments.
    let (receiver, function) = callee_and_receiver(builder, scope, ctx, callee)?;
    let name = callee_spelling(ctx, callee);
    emit_call_with_name(builder, scope, ctx, function, receiver, arguments, name)
}

/// `eval(source)` written as exactly that, and nothing else.
///
/// # Why this is decided here and cannot be decided anywhere else
///
/// A direct `eval` sees the bindings of the frame it was written in; an indirect
/// one — `(0, eval)(s)`, `const e = eval; e(s)`, `globalThis.eval(s)` — runs in
/// the global scope. The two call the SAME function value, so nothing at run
/// time can tell them apart. The difference is in the syntax, which exists only
/// while this crate is looking at it.
///
/// So the direct form becomes its own entry point, carrying the environment
/// object beside the source. A name lexically bound to `eval` — a parameter, a
/// local, an import — is NOT this: the callee then names that binding, and the
/// ordinary call is what runs. `scope.lookup` answers exactly that, and
/// `ctx.globals` answers the sloppy case where the program assigns `eval`
/// itself.
///
/// A spread argument (`eval(...parts)`) is left to the ordinary call: the
/// environment could still be passed, but the source is then whatever the spread
/// produced first and this engine has no way to hand a list to the entry. Left
/// as the indirect path rather than answered wrongly.
fn direct_eval(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    let ExprKind::Ident(name) = &callee.kind else {
        return Ok(None);
    };
    if ctx.names.text(*name) != "eval" || scope.lookup(*name).is_some() {
        return Ok(None);
    }
    let [Spreadable::Single(source)] = arguments else {
        return Ok(None);
    };
    let source = emit_expr(builder, scope, ctx, source)?;
    // The environment the caller's captured names live in. A body that mentions
    // `eval` has every name in its scope forced into one — see
    // `emit::function` — so this is what makes those names reachable from
    // source compiled afterwards. `None` is a body with nothing in scope at
    // all, where `undefined` says truthfully that there is nothing to see.
    let environment = match scope.environment() {
        Some(environment) => environment,
        None => expr::undefined(builder, ctx),
    };
    let answered = expr::call(builder, ctx, RuntimeOp::EvalDirect, &[source, environment])?[0];
    Ok(Some(answered))
}

/// `Math.sqrt(x)` and its four siblings, as the instruction the hardware has.
///
/// # Three conditions, and each one is a proof rather than a guess
///
/// The program must not disturb `Math` anywhere — `primordial::untouched`,
/// computed over the whole tree before anything was emitted. No enclosing scope
/// may bind the name, which the scope answers exactly. And the argument must
/// ALREADY be a proven double: a guard here would be correct too, but the
/// operand of a square root in a loop is proven by the type pass in the case
/// that matters, and emitting a guard for the rest would cost a branch to
/// discover what the call would have found anyway.
///
/// Answers `None` for anything else, and the ordinary call follows.
///
/// # Why the language decides this and not the machine
///
/// `Inst::FloatUnary` knows nothing about `Math` — rule 2 of the machine's own
/// README, no source-language knowledge there. Which name means a square root
/// is a fact about JavaScript, so it is decided here, in the crate that is
/// allowed to know.
fn machine_operation(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    if !ctx.math_primordial {
        return Ok(None);
    }
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &callee.kind
    else {
        return Ok(None);
    };
    let ExprKind::Ident(name) = &object.kind else {
        return Ok(None);
    };
    if ctx.names.text(*name) != "Math" || scope.lookup(*name).is_some() {
        return Ok(None);
    }
    // `Math.random()` takes no argument and answers a double, so it needs no
    // proven operand — only the same whole-program proof. Not an instruction:
    // there is no opcode for a generator. What it skips is the PATH — the
    // property read through the chain cache and the generic call machinery —
    // which is where its 40 ns were, since the generator itself is a
    // thread-local xorshift.
    if ctx.names.text(*property) == "random" && arguments.is_empty() {
        let drawn = super::expr::call(builder, ctx, RuntimeOp::MathRandom, &[])?[0];
        return Ok(Some(super::expr::tagged(builder, drawn)));
    }
    let op = match ctx.names.text(*property) {
        "sqrt" => FloatOp::Sqrt,
        "floor" => FloatOp::Floor,
        "ceil" => FloatOp::Ceil,
        "trunc" => FloatOp::Trunc,
        "abs" => FloatOp::Abs,
        _ => return Ok(None),
    };
    let [Spreadable::Single(only)] = arguments else {
        return Ok(None);
    };
    let argument = super::expr::emit_expr(builder, scope, ctx, only)?;
    if builder.repr_of(argument) != Repr::F64 {
        return Ok(None);
    }
    let answered = builder.float_unary(op, argument)?;
    Ok(Some(super::expr::tagged(builder, answered)))
}

/// What a `TypeError` should call the callee, from how it was spelled.
///
/// Only the spellings the language can put a name to: `f` and `o.f`, which
/// between them are most of the 64 files a bare "is not a function" left
/// untriaged. `o[k]()`, `super.f()` and a callee that is itself an expression
/// answer `None` — there is no single right spelling for "the fourth element
/// of an array" — and the caller falls back to what the value's KIND was.
pub(super) fn callee_spelling(ctx: &mut Ctx, callee: &Expr) -> Option<u32> {
    let text = match &spelled(callee).kind {
        ExprKind::Member {
            object, property, ..
        } => {
            let base = match &object.kind {
                ExprKind::Ident(name) => ctx.names.text(*name).to_owned(),
                // V8's own placeholder for a receiver with no single
                // spelling — `getObj().foo()` reads `getObj(...)`.
                _ => "(intermediate value)".to_owned(),
            };
            format!("{base}.{}", ctx.names.text(*property))
        }
        ExprKind::Ident(name) => ctx.names.text(*name).to_owned(),
        _ => return None,
    };
    Some(ctx.literal(&text))
}

/// A callee with its type assertions peeled off.
///
/// `x as T` and `<T>x` are KEPT in the tree as their own node rather than
/// applied ([`ExprKind::Asserted`]) — a claim is weighed where claims are
/// weighed, not by the parser. So every place that decides something from the
/// *spelling* of a callee has to look through one, and the two below are the
/// places this module decides anything from a spelling.
///
/// Without it an asserted callee fell to the plain-call arm, which passes
/// `undefined` as the receiver — so a type assertion silently lost `this`.
/// Measured: `(s.at as any)(0)` answered `undefined` where `(s.at)(0)` answered
/// `"h"`, and `(o.m as any)()` answered `undefined` where `(o.m)()` answered
/// `"object"`. A `TypeError` naming the callee lost the name the same way.
///
/// `sloppy.rs` peels for its own question — whether an object names
/// `globalThis` — and the two are not one rule stated twice: that one asks what
/// an OBJECT is, this one asks what a CALL is.
fn spelled(callee: &Expr) -> &Expr {
    match &callee.kind {
        ExprKind::Asserted { value, .. } => spelled(value),
        _ => callee,
    }
}

/// What a call site calls, and what it calls it ON.
///
/// Extracted so a tagged template reaches the same answer: `` o.tag`x` `` passes
/// `o` as its receiver exactly as `o.tag(x)` does, and deciding that a second
/// time is how the two spellings would come to disagree about `this`.
pub(super) fn callee_and_receiver(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
) -> EmitResult<(ValueId, ValueId)> {
    let callee = spelled(callee);
    let pair = match &callee.kind {
        ExprKind::Member {
            object,
            property,
            optional,
        } => {
            if *optional {
                return Err(EmitError::Unsupported {
                    construct: "an optional call",
                });
            }
            // Which form to read the callee with. The POSITION says
            // "prototype", because that is where a method lives — but where the
            // program declared this member as a FIELD of the class it claims the
            // receiver is, the position is wrong and the cheap form is right.
            // `c.cb()` on `class C { cb: () => void }` is an own read.
            //
            // Both are legal emissions of the same read, which is what makes an
            // unsound resolution safe here: a type name and a value name are one
            // `Name`, and an interface is erased entirely, so this answers wrong
            // sometimes. Wrong costs a site that re-resolves — what it did before
            // the chain form existed — and never an answer.
            let own = match &object.kind {
                crate::syntax::ExprKind::Ident(name) => ctx.reads_own_field(*name, *property),
                _ => false,
            };
            let receiver = emit_expr(builder, scope, ctx, object)?;
            let function = match own {
                true => super::property::emit_read(builder, ctx, receiver, *property)?,
                false => super::property::emit_read_indirect(builder, ctx, receiver, *property)?,
            };
            (receiver, function)
        }
        // `o[k]()` is a method call, exactly as `o.k()` is. It fell into the
        // plain-call arm below and was called with `undefined` as its receiver,
        // so `arr["push"](1)` pushed onto nothing and `(255)["toString"](16)`
        // read its own `this` as `NaN`.
        //
        // Which spelling was written decides how the KEY is resolved and nothing
        // else — the same rule `computed.rs` states for the read side, arrived at
        // here from the call side.
        ExprKind::Index {
            object,
            index,
            optional,
        } => {
            if *optional {
                return Err(EmitError::Unsupported {
                    construct: "an optional call",
                });
            }
            // The object once, into a value both the read and the call use.
            // Emitting it twice would evaluate it twice, which is the mistake
            // this whole `(receiver, function)` pair exists to prevent —
            // `f()["m"]()` must call `f` once.
            let receiver = emit_expr(builder, scope, ctx, object)?;
            let key = emit_expr(builder, scope, ctx, index)?;
            let function = expr::call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
            (receiver, function)
        }
        // `super.m()` looks up above the home object and calls with **this**
        // activation's receiver. Both halves matter and it used to have
        // neither: falling into the plain-call arm below passed `undefined`, so
        // the parent's method ran with no receiver and every `this.x` in it read
        // absent. The method was found — `emit_super_member` was already right —
        // and then called as if it were a loose function.
        ExprKind::SuperMember { property } => {
            let function = super::class::emit_super_member(builder, scope, ctx, property)?;
            let receiver = scope.this_value().ok_or(EmitError::Unsupported {
                construct: "`super.m()` where there is no receiver",
            })?;
            (receiver, function)
        }
        _ => {
            // A plain call has no receiver, and `undefined` is what the
            // specification passes in strict code. Sloppy mode substitutes the
            // global object, which does not exist here — named as the reason
            // rather than left as a coincidence, because the day globals arrive
            // this line is one of the two that has to change.
            let function = emit_expr(builder, scope, ctx, callee)?;
            let undefined = expr::undefined(builder, ctx);
            (undefined, function)
        }
    };
    Ok(pair)
}

/// Emits a call whose callee and receiver are already values, with the
/// callee's source spelling for a "not a function" message where one exists.
///
/// Split out for `super(…)` and for `optional.rs`'s `walk`, neither of which
/// has a member expression to hand [`emit_call`] — `super(…)`'s callee comes
/// from the class environment and has nothing to name, so it passes `None`;
/// `walk` has the member/index expression the chain wraps, so it passes
/// [`callee_spelling`]'s answer for it. Everything after that — the arity
/// check, the order, the padding — is the same rule, and a second copy of it
/// is where either caller would come to pad, or name, differently from a
/// plain method call.
pub(super) fn emit_call_with_name(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    function: ValueId,
    receiver: ValueId,
    arguments: &[Spreadable],
    name: Option<u32>,
) -> EmitResult<ValueId> {
    // Past what the convention carries, the arguments go in an array and the
    // runtime holds it for the activation. The common call is unchanged and
    // allocates nothing — which is the whole reason this is a second operation
    // rather than the only one.
    // A spread takes this path whatever the written count is, because how many
    // values it contributes is not known while compiling — one written
    // argument may become none or nine.
    if arguments.len() > ARGUMENT_SLOTS || has_spread(arguments) {
        let vector = emit_argument_vector(builder, scope, ctx, arguments)?;
        emit_set_call_name(builder, ctx, name)?;
        return Ok(expr::call(
            builder,
            ctx,
            RuntimeOp::CallWithArgs,
            &[function, receiver, vector],
        )?[0]);
    }

    let mut values = Vec::with_capacity(ARGUMENT_SLOTS);
    for argument in arguments {
        let Spreadable::Single(value) = argument else {
            return Err(EmitError::Unsupported {
                construct: "a spread argument",
            });
        };
        values.push(emit_expr(builder, scope, ctx, value)?);
    }
    emit_set_call_name(builder, ctx, name)?;
    issue(builder, ctx, function, receiver, &values)
}

/// Records the callee's spelling for the call about to be issued, if it has
/// one — emitted last, after every argument, so an argument that calls
/// something of its own cannot overwrite what this call site just recorded
/// for itself. See `RuntimeOp::SetCallName`.
fn emit_set_call_name(builder: &mut FuncBuilder, ctx: &mut Ctx, name: Option<u32>) -> EmitResult<()> {
    let Some(literal) = name else {
        return Ok(());
    };
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(u64::from(literal)),
    });
    let id = builder.use_const(id);
    expr::call(builder, ctx, RuntimeOp::SetCallName, &[id])?;
    Ok(())
}

/// Emits a call whose arguments are already values.
///
/// # Why this exists beside [`emit_call_with_name`]
///
/// A tagged template's first argument is not written in the program — it is the
/// strings object, built by the emitter — so there is no expression to hand the
/// function above. What must NOT be duplicated is what happens after the
/// arguments exist: the padding, the order it happens in, and the choice between
/// the convention and the argument vector. A second copy of that is where a
/// tagged template would come to pad differently from a call.
pub(super) fn issue(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    function: ValueId,
    receiver: ValueId,
    values: &[ValueId],
) -> EmitResult<ValueId> {
    if values.len() > ARGUMENT_SLOTS {
        // Through the shared list builder: the first four go in one crossing
        // and the rest are appended, where this was one crossing to make the
        // array and one per value. See `expr::value_list`.
        let vector = expr::value_list(builder, ctx, values)?;
        return Ok(expr::call(
            builder,
            ctx,
            RuntimeOp::CallWithArgs,
            &[function, receiver, vector],
        )?[0]);
    }

    let mut passed = Vec::with_capacity(3 + ARGUMENT_SLOTS);
    passed.push(function);
    passed.push(receiver);
    // HOW MANY arguments were written, which nothing carried and which the
    // runtime was reconstructing by dropping `undefined` from the end. That
    // reconstruction is wrong for every program that passes one on purpose:
    // `console.log(undefined)` printed an empty line, `[].push(undefined)`
    // pushed nothing, and `f(undefined)` reached a rest parameter as `[]`.
    //
    // An operand rather than a call before the jump: `SetCallName` is the
    // precedent for that second shape and it costs a crossing per call site,
    // where this costs one register the convention already had room for.
    passed.push(expr::count_constant(builder, values.len()));
    passed.extend_from_slice(values);
    // Padded after the written arguments were evaluated, so the padding cannot
    // get between two of them and change the order side effects happen in.
    while passed.len() < 3 + ARGUMENT_SLOTS {
        let undefined = expr::undefined(builder, ctx);
        passed.push(undefined);
    }

    Ok(expr::call(builder, ctx, RuntimeOp::Call, &passed)?[0])
}

/// Emits `new f(…)`.
///
/// # Why this is not a call with a flag
///
/// It does not pass a receiver — it *makes* one, from the callee's `prototype`
/// — and it answers the object rather than what the callee returned, unless the
/// callee returned an object of its own. Three differences, none of which a
/// flag on a call site could express without the call site knowing all of them.
///
/// So the arguments are padded exactly as a call's are, and everything else is
/// the runtime's.
pub fn emit_construct(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    let function = emit_expr(builder, scope, ctx, callee)?;
    emit_construction(builder, scope, ctx, function, arguments, RuntimeOp::Construct)
}

/// The same, for a callee that is already a value.
///
/// `super(…)` needs it: the parent comes from the class environment rather than
/// from an expression, and it reaches a **different** runtime operation —
/// `super()` must not establish a new `new.target`, because the object the chain
/// builds has to inherit from the prototype of the class `new` actually named.
/// Everything else — the arity, the order, the padding — is one rule, and a
/// second copy is where the two would come to pad differently.
pub fn emit_super_construct(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    function: ValueId,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    emit_construction(builder, scope, ctx, function, arguments, RuntimeOp::SuperConstruct)
}

/// The written arguments, as an ordinary array.
///
/// An array literal rather than a second kind of storage: the elements are
/// evaluated in source order and written at their own indices, which is what
/// `[a, b, c]` already does — so a call with six arguments and an array literal
/// of six elements produce the same thing, and there is one answer to what a
/// sequence of values is.
pub(super) fn emit_argument_vector(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    arguments: &[Spreadable],
) -> EmitResult<ValueId> {
    // No spread: every value is one element, the count is known while
    // compiling, and the shared list builder puts the first four in one
    // crossing. `g(1, 2, 3, 4, 5, 6)` was `ArrayNew` plus six appends — seven
    // crossings for a call — and is three.
    if !has_spread(arguments) {
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let crate::syntax::Spreadable::Single(value) = argument else {
                unreachable!("no argument is a spread on this path")
            };
            values.push(emit_expr(builder, scope, ctx, value)?);
        }
        return expr::value_list(builder, ctx, &values);
    }

    // Started empty and appended to, rather than sized once and written at
    // fixed indices: a spread contributes a count nothing knows while
    // compiling, so every index after one is unknown too.
    let length = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(0),
    });
    let length = builder.use_const(length);
    let array = expr::call(builder, ctx, RuntimeOp::ArrayNew, &[length])?[0];
    for argument in arguments {
        let (value, op) = match argument {
            Spreadable::Single(value) => (value, RuntimeOp::ArrayAppend),
            Spreadable::Spread(value) => (value, RuntimeOp::ArrayAppendAll),
        };
        let value = emit_expr(builder, scope, ctx, value)?;
        expr::call(builder, ctx, op, &[array, value])?;
    }
    Ok(array)
}

/// The shared body, differing only in which operation is dialled.
fn emit_construction(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    function: ValueId,
    arguments: &[Spreadable],
    op: RuntimeOp,
) -> EmitResult<ValueId> {
    // Past what the convention carries, the same trade the call side makes.
    // `super()` cannot use `ConstructWithArgs`: that operation SETS
    // `new.target`, and `super()` must not — the object the whole chain builds
    // has to keep inheriting from the prototype of the class `new` actually
    // named, however many arguments this `super()` spreads.
    // `RuntimeOp::SuperConstructWithArgs` is the vector-shaped, `new.target`
    // inert counterpart, for exactly this case.
    if arguments.len() > ARGUMENT_SLOTS || has_spread(arguments) {
        let vector = emit_argument_vector(builder, scope, ctx, arguments)?;
        let vector_op = if op == RuntimeOp::SuperConstruct {
            RuntimeOp::SuperConstructWithArgs
        } else {
            RuntimeOp::ConstructWithArgs
        };
        return Ok(expr::call(builder, ctx, vector_op, &[function, vector])?[0]);
    }

    let mut values = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Spreadable::Single(value) = argument else {
            return Err(EmitError::Unsupported {
                construct: "a spread argument",
            });
        };
        values.push(emit_expr(builder, scope, ctx, value)?);
    }
    construct_with(builder, ctx, function, &values, op)
}

/// `new f(…)` where both the callee and every argument are already values.
///
/// The emitter builds a construction of its own in one place — `binding`'s
/// dead-zone read, which throws a `ReferenceError` the program can catch and
/// inspect — and it has no expressions to evaluate. Sharing the padded path
/// rather than writing a second one is rule 3: the arity and the padding are one
/// rule, and a second copy is where the two would come to pad differently.
///
/// Fewer than [`ARGUMENT_SLOTS`] arguments only; a caller with more has an
/// argument vector to build, which needs the syntax this does not take.
pub(super) fn construct_value(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    function: ValueId,
    values: &[ValueId],
) -> EmitResult<ValueId> {
    debug_assert!(values.len() <= ARGUMENT_SLOTS);
    construct_with(builder, ctx, function, values, RuntimeOp::Construct)
}

/// The padded crossing both of the above end in.
fn construct_with(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    function: ValueId,
    values: &[ValueId],
    op: RuntimeOp,
) -> EmitResult<ValueId> {
    let mut passed = Vec::with_capacity(1 + ARGUMENT_SLOTS);
    passed.push(function);
    passed.extend_from_slice(values);
    while passed.len() < 1 + ARGUMENT_SLOTS {
        let undefined = expr::undefined(builder, ctx);
        passed.push(undefined);
    }
    Ok(expr::call(builder, ctx, op, &passed)?[0])
}

/// Whether any argument is a spread.
///
/// Asked before the arity is, because a spread makes the arity unknowable: one
/// written argument may contribute none or nine, so the padded path cannot be
/// taken however few were spelled.
fn has_spread(arguments: &[Spreadable]) -> bool {
    arguments
        .iter()
        .any(|argument| matches!(argument, Spreadable::Spread(_)))
}
