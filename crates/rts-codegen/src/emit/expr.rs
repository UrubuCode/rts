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
//! A gap is named rather than approximated, and the list of names is the work
//! queue. What is left needs a mechanism this module does not have — a global
//! object for a bare name to be a property of, a protected region to throw out
//! of, an argument vector for spread.
//!
//! A string literal was the longest-standing entry and is no longer one: two
//! occurrences of `"a"` in a program are the SAME string, so an immediate would
//! have produced a value that is not a string and compares wrongly with
//! everything. What the code carries is which literal; the runtime holds the
//! text.
//!
//! # Where the rest of an expression lives
//!
//! `choice.rs` holds the four that may not evaluate part of themselves — `?:`,
//! `&&`, `||`, `??` — together with the merge that turns two paths back into
//! one value. `unary.rs` holds the operators with a single operand and the two
//! that also write back. Both are here rather than in this file because they
//! create blocks and nothing else in this file does, and because this file is
//! within sight of the thousand-line ceiling rule 8 sets.

use rts_cranelift::ir::inst::{CmpOp, NumOp};
use rts_cranelift::ir::BitOp;
use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;
use rts_cranelift::tags;

use super::{Ctx, EmitError, EmitResult, Scope, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{AssignOp, BinaryOp};
use crate::syntax::{AssignTarget, Expr, ExprKind, Literal};
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
pub(super) fn singleton(builder: &mut FuncBuilder, ctx: &mut Ctx, which: Singleton) -> ValueId {
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

        ExprKind::Ident(name) => super::binding::read(builder, scope, ctx, *name),

        ExprKind::Binary { op, left, right } => {
            // A literal on both sides has no side effect to preserve, so the
            // ordering rule below does not apply and folding it at compile
            // time is free: it removes a runtime call site (~100us of
            // Cranelift codegen each, measured — see `fold.rs`) rather than
            // just precomputing a run-time operation. Anything else — a
            // proven-constant LOCAL, say — is deliberately not attempted
            // here: proving that is safe needs knowing the read has no
            // intervening write, which `fold.rs` has no way to ask.
            if let (ExprKind::Literal(l), ExprKind::Literal(r)) = (&left.kind, &right.kind) {
                if let Some(folded) = super::fold::fold_binary(*op, l, r) {
                    return emit_literal(builder, ctx, &folded);
                }
            }
            // `#x in o` is the one binary form whose LEFT operand is not a
            // value. A private name is a key and nothing else — `parse/mod.rs`
            // interns `#x` as `"@@#x"`, an ordinary property in a reserved name
            // space — so there is nothing to evaluate on that side, and asking
            // `emit_expr` for one is what refused this by name.
            //
            // Only the object is evaluated, which is also the order the language
            // states: there is no other operand to have a side effect.
            if let (BinaryOp::In, ExprKind::PrivateName(held)) = (op, &left.kind) {
                let object = emit_expr(builder, scope, ctx, right)?;
                let text = ctx.names.text(*held).to_owned();
                let key = emit_literal(builder, ctx, &Literal::String(text))?;
                return emit_binary(builder, ctx, BinaryOp::In, key, object);
            }
            // Left before right, unconditionally. Every JavaScript binary
            // operator evaluates its operands in source order even where it
            // then converts them in the other order, and emitting in the wrong
            // order changes which side effect happens first.
            // Counted before emitting, while the operands are still
            // EXPRESSIONS: `emit_binary` sees two `ValueId`s and a value has no
            // name to have been annotated. This is the census phase 1 exists
            // for, and the number it is expected to report is that a claimed
            // `number` here buys nothing, because the guard it would ask for
            // is emitted either way.
            count_claimed_operands(ctx, left, right);
            // Whether speculating is worth EMITTING is decided from the
            // operands while they are still expressions, because a value has
            // no name to have been annotated.
            let speculate = speculation_is_worth_emitting(ctx, left, right);
            let a = emit_expr(builder, scope, ctx, left)?;
            let b = emit_expr(builder, scope, ctx, right)?;
            emit_binary_speculating(builder, ctx, *op, a, b, speculate)
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

        ExprKind::Assign { target, value, op } => {
            emit_assign(builder, scope, ctx, target, value, *op)
        }

        // Every remaining form, named. The list is the deliverable: it is the
        // work queue for the phases after this one, and a reader can check it
        // against `PLAN.md` §E without running anything.
        ExprKind::Call {
            callee, arguments, ..
        } => super::call::emit_call(builder, scope, ctx, callee, arguments),
        ExprKind::New {
            callee, arguments, ..
        } => super::call::emit_construct(builder, scope, ctx, callee, arguments),
        ExprKind::Member {
            object,
            property,
            optional,
        } => {
            // A local whose object the escape analysis replaced has no object to
            // read from: the property IS a local, so the read is free rather
            // than a guard, a cache and a possible call. `escape.rs` says what
            // has to be true for that to be the same program.
            if let Some(field) = super::escape::field_of(ctx, object, *property, *optional) {
                return super::binding::read(builder, scope, ctx, field);
            }
            // `property` is a name, not a key: `o[e]` is `Index`, a different
            // node. So there is no computed case to refuse here.
            let receiver = emit_expr(builder, scope, ctx, object)?;
            super::property::emit_read(builder, ctx, receiver, *property)
        }
        ExprKind::Index { object, index, .. } => {
            // A read a DESUGARING proved: the receiver is an array it made and
            // the index is a counter it minted, so none of the questions
            // `GetIndexed` asks has an answer the caller did not already have.
            // See `RuntimeOp::ElementAt`, and `foreach.rs` for the one producer.
            if let (ExprKind::Ident(array), ExprKind::Ident(at)) = (&object.kind, &index.kind)
                && ctx.is_proven_element(*array, *at)
            {
                // With the run hoisted, the read is an INSTRUCTION: a bounded
                // load from the base the loop asked for once. The bound is the
                // machine's, not a comparison written here — see
                // `Inst::ElementLoad`.
                //
                // Only when the counter is a PROVEN double, which is the same
                // condition `foreach.rs` puts on hoisting the run at all:
                // `to_int32` takes a double, and a counter that stayed generic
                // has no such proof. Asking anyway is `WrongDomain` at
                // emission, which refuses the whole program rather than this
                // read — measured as 37 fixtures that stopped compiling, every
                // generator among them. Rule 5: what cannot be proven becomes
                // generic, visibly.
                let position = emit_expr(builder, scope, ctx, index)?;
                if let Some((base, count)) = ctx.element_run()
                    && builder.repr_of(position) == Repr::F64
                {
                    let position = builder.to_int32(position)?;
                    return Ok(builder.element_load(base, position, count)?);
                }
                let receiver = emit_expr(builder, scope, ctx, object)?;
                // The counter again as the KEY, and it is the value already
                // emitted above rather than a second emission of the same
                // identifier: reading a local twice is harmless, emitting it
                // twice is a second instruction for one read.
                let key = tagged(builder, position);
                return Ok(call(builder, ctx, RuntimeOp::ElementAt, &[receiver, key])?[0]);
            }
            // Receiver first, then the key: `a()[b()]` runs `a` before `b`.
            let receiver = emit_expr(builder, scope, ctx, object)?;
            // A key written as a literal is a key the COMPILER knows, so it can
            // take the named path — the inline cache, and no key to convert at
            // run time. See [`literal_name`] for why this is worth doing and
            // when it is refused.
            if let Some(name) = literal_name(ctx, index) {
                return super::property::emit_read(builder, ctx, receiver, name);
            }
            let key = emit_expr(builder, scope, ctx, index)?;
            Ok(call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0])
        }
        ExprKind::Object { properties } => super::object::emit_object(builder, scope, ctx, properties),
        ExprKind::Array { elements } => emit_array(builder, scope, ctx, elements),
        ExprKind::Function(function) => {
            super::function::emit_closure(builder, scope, ctx, function)
        }
        ExprKind::Class(class) => super::class::emit_class(builder, scope, ctx, class),
        ExprKind::Unary { op, operand } => {
            // `-x` on a literal number, folded the same way a binary literal
            // pair is: no side effect to preserve, and a removed call site.
            if *op == crate::syntax::UnaryOp::Negate {
                if let ExprKind::Literal(literal) = &operand.kind {
                    if let Some(folded) = super::fold::fold_negate(literal) {
                        return emit_literal(builder, ctx, &folded);
                    }
                }
            }
            super::unary::emit_unary(builder, scope, ctx, *op, operand)
        }
        ExprKind::Update {
            op,
            position,
            target,
        } => super::unary::emit_update(builder, scope, ctx, *op, *position, target),
        ExprKind::Logical { op, left, right } => {
            super::choice::emit_logical(builder, scope, ctx, *op, left, right)
        }
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => super::choice::emit_conditional(
            builder,
            scope,
            ctx,
            condition,
            then_branch,
            else_branch,
        ),
        // Refused inside an arrow, which takes `this` from where it was
        // written rather than from how it is called — see `Scope::set_this`.
        // In a derived constructor `this` is a name in the environment rather
        // than the receiver, because there is no receiver until `super()`
        // answers one. Asked first, so a body that reads it takes the binding
        // rather than the parameter — which holds `undefined` there and would
        // silently be the wrong object.
        ExprKind::This => match scope.late_this() {
            Some(name) => super::binding::read(builder, scope, ctx, name),
            None => scope.this_value().ok_or(EmitError::Unsupported {
                construct: "`this` inside an arrow function",
            }),
        },
        // The promise is produced first and the instruction waits on it. What
        // waiting MEANS is the machine's: `Inst::Await` lowers to a call that
        // drains until the promise settles, so this frame keeps the machine
        // rather than yielding to its caller. That divergence is stated in
        // `rts-core`'s `promise/machine.rs`, which is the half that does it.
        // A rejected promise raises through the in-flight throw INSIDE
        // `promise_await` (`rts-core`), the same mechanism a native uses
        // under rule 8 — but that native is reached by a raw machine call
        // `rts-cranelift` emits for `Inst::Await`, never through this crate's
        // `call` helper, so nothing asked afterward. `raise_if_thrown` is the
        // same branch-and-reraise `check_for_throw` gives every `RuntimeOp`
        // call, applied here explicitly because this is not one.
        ExprKind::Await(inner) => {
            let produced = emit_expr(builder, scope, ctx, inner)?;
            let promise = as_value(builder, produced);
            let awaited = builder.await_(promise);
            raise_if_thrown(builder, ctx)?;
            Ok(awaited)
        }
        // Two operations, not one. The call hands the value over; the suspension
        // parks the frame, and its RESULT is what the next resumption delivers —
        // which is what `const x = yield 1` reads. There is deliberately no
        // instruction carrying a value: the machine's suspension is generic, and
        // teaching it what a generator produces would put a language fact in the
        // layer that is defined by having none.
        //
        // `yield*` is refused here rather than by a pre-pass over the body: it
        // forwards `next`, `throw` and `return` to an inner iterator and yields
        // whatever that yields, so it is a loop over the iteration protocol and
        // not a single suspension.
        ExprKind::Yield { value, delegate } => match delegate {
            true => match value {
                Some(subject) => super::delegate::emit_delegated(builder, scope, ctx, subject),
                // `yield*` with no operand does not parse, so nothing reaches
                // this — named rather than left to panic if something ever can.
                None => gap("`yield*` with nothing to delegate to"),
            },
            false => {
                let produced = match value {
                    Some(value) => {
                        let produced = emit_expr(builder, scope, ctx, value)?;
                        as_value(builder, produced)
                    }
                    None => undefined(builder, ctx),
                };
                call(builder, ctx, RuntimeOp::GeneratorYield, &[produced])?;
                Ok(builder.suspend())
            }
        },
        ExprKind::Template { parts, expressions } => {
            super::template::emit_template(builder, scope, ctx, parts, expressions)
        }
        ExprKind::TaggedTemplate {
            tag,
            parts,
            expressions,
        } => super::template::emit_tagged_template(builder, scope, ctx, tag, parts, expressions),
        ExprKind::Chain(inner) => super::optional::emit_chain(builder, scope, ctx, inner),
        ExprKind::SuperMember { property } => {
            super::class::emit_super_member(builder, scope, ctx, property)
        }
        ExprKind::SuperCall { arguments } => {
            super::class::emit_super_call(builder, scope, ctx, arguments)
        }
        ExprKind::PrivateName(_) => gap("a private name"),
        ExprKind::NewTarget => gap("`new.target`"),
        ExprKind::ImportMeta => gap("`import.meta`"),
        ExprKind::ImportCall { .. } => gap("`import()`"),
        // `e as T` and `<T>e` are erased, not proved. Rule 4 says an annotation
        // is a CLAIM, not a proof, and TypeScript gives no guarantee that this
        // one is true — `x as string` on a number compiles and runs, and at
        // run time `x` is still a number. Emitting the assertion as a guard, or
        // as evidence for `Repr::F64`, would be treating the program's word for
        // "trust me" as something that checked anything, which is the mistake
        // this rule exists to name. So the assertion carries no weight here: it
        // lowers to exactly what its inner expression lowers to, and whatever
        // representation THAT earns on its own is what this value gets.
        ExprKind::Asserted { value, .. } => emit_expr(builder, scope, ctx, value),
    }
}

/// The name a computed key spells, when it spells one the compiler can resolve.
///
/// # Why this is worth a special case
///
/// `o["alpha"]` and `o.alpha` are the same property written two ways, and they
/// were compiled completely differently: the second reaches the inline cache and
/// a hit is one load, the first called `GetIndexed`, which converts a value to a
/// key at run time and takes the slow path every time. **Measured at 150x** —
/// 200 000 reads cost 0.65 ms named and 98 ms computed.
///
/// A literal key is not a computed key in any sense that matters. The program
/// wrote the name down; only the syntax differs, and the syntax is exactly what
/// this crate is here to see through.
///
/// # Why the refusal is deliberately broader than the rule
///
/// An all-digit key must NOT take this path. `a["0"]` on an array reads
/// **element** zero, which `GetIndexed` finds by asking whether the key is a
/// canonical array index before it converts anything — and a named read never
/// asks, so it would find a property that is not there and answer `undefined`.
///
/// The exact rule is the canonical-index one: a decimal spelling of a number
/// below 2^32-1 with no leading zero. Stating it here would be stating it twice,
/// because `rts-core`'s `as_array_index` already owns it — and this crate
/// cannot call that one, being above it in the graph and forbidden from naming
/// a runtime.
///
/// So the test is **any digit at all**, which is strictly stronger than the rule
/// and therefore cannot be wrong: every array index is all digits, so refusing
/// everything containing one refuses every index. What it costs is `o["1a"]`
/// taking the slow path, which no rule requires and no program notices.
/// Choosing a conservative test over a duplicated rule is the trade, and it is
/// the one this codebase makes elsewhere for the same reason.
fn literal_name(ctx: &mut Ctx, index: &Expr) -> Option<Name> {
    let ExprKind::Literal(Literal::String(text)) = &index.kind else {
        return None;
    };
    if text.chars().any(|character| character.is_ascii_digit()) {
        return None;
    }
    Some(ctx.names.intern(text))
}

/// Widens a value for a position that takes a JavaScript value.
pub fn as_value(builder: &mut FuncBuilder, value: ValueId) -> ValueId {
    tagged(builder, value)
}

/// A named gap.
pub(super) fn gap<T>(construct: &'static str) -> EmitResult<T> {
    Err(EmitError::Unsupported { construct })
}

/// Calls a runtime operation.
///
/// Declaring on demand rather than up front: what a program does not do should
/// not appear in what it links, and a compilation that never concatenates
/// should carry no relocation to the string path.
pub(super) fn call(
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
    let produced = builder.call(ctx.funcs, callee, &args)?;
    check_for_throw(builder, ctx, op)?;
    Ok(produced)
}

/// Emits, after a call, the branch a throw in the callee takes.
///
/// # Why every call site pays this
///
/// A throw leaves ONE frame: the machine records it and returns rather than
/// ending the program, so the frame above only learns what happened by asking.
/// Re-raising here puts the value back into the machine's own hands, which then
/// routes it to a handler in THIS function through the region tree it already
/// computes — or, finding none, returns and lets the frame above ask in turn.
///
/// That is the whole of cross-frame unwinding here, and it is what made `try`
/// around a call compilable. The alternative is an exception table and a
/// personality routine, which is a campaign rather than a branch.
///
/// # Why after nearly every operation and not only after a call
///
/// Because nearly every one of them can run user code. `a + b` calls `valueOf`;
/// a property read calls a getter; a comparison coerces. Listing the ones that
/// cannot would be a list to get wrong, and getting it wrong means a throw that
/// vanishes — so the two operations that ARE the check are the only exceptions,
/// and they are excluded because including them recurses forever.
///
/// # What it costs
///
/// It HAS been measured now, and this paragraph used to say it had not.
/// 2026-08-11, release, `bench/analytic.ts` compiled with the check suppressed
/// against the same file compiled normally: **9% of an operation at the median,
/// and up to 30% on the cheap ones** — an optional chain 44.0 → 30.8 ns, an
/// object literal 13.3 → 9.5 ns, a free call 39.3 → 31.1 ns. The expensive
/// operations barely move, because a call, a compare and a branch is a fixed
/// cost and they are not.
///
/// **That is NOT the size of the prize for making the flag a load, and this
/// paragraph used to say it was.** The flag IS a load now — the address is
/// asked once per activation and `Inst::WordLoad` reads it, see
/// `RuntimeOp::ThrownAddress` — and it collected between 0.1 and 0.6 ns per
/// check, not the 9%. Measured 2026-08-13, release, both binaries rebuilt in
/// full, three reads each: a body of eight checks went 25.00/25.00/25.00 ns to
/// 24.50/24.00/24.50, and `a[i & 7]` went 14.60/14.60/14.70 to
/// 13.30/13.40/13.60 reading and 15.40/15.90/16.20 to 13.80/13.80/13.90
/// writing. Every one of the six moved down, which is the strongest claim
/// available; the per-check attribution differs between the two by six times,
/// which is what code layout does to a measurement here.
///
/// So the suppression number measured the whole check — the call, the compare
/// AND the branch — and what is left after removing the call is most of it.
/// The remaining prize belongs to not asking at all, which needs an operation
/// this layer can prove cannot raise. It is not the size of anything else:
/// the same measurement refuted the idea that a fixed per-operation cost
/// dominates this engine, since `type_of` costs 2.83 ns called from Rust
/// (`rts-core/examples/entry_cost.rs`) and 26 ns from compiled code.
fn check_for_throw(builder: &mut FuncBuilder, ctx: &mut Ctx, op: RuntimeOp) -> EmitResult<()> {
    if matches!(op, RuntimeOp::Thrown | RuntimeOp::TakeThrown) {
        return Ok(());
    }
    raise_if_thrown(builder, ctx)
}

/// The branch-and-reraise `check_for_throw` performs, without the guard that is
/// specific to a `RuntimeOp` call site.
///
/// # Why `Inst::Await` needs this directly
///
/// `await` does not lower through [`call`] — `rts-cranelift` lowers
/// `Inst::Await` straight to `RtEntry::PromiseAwait`, a machine-level call this
/// layer never sees as a `RuntimeOp`. `promise_await` (`rts-core`,
/// `entry/promise/machine.rs`) already raises through the in-flight throw when
/// the awaited promise rejected — its own doc says a `try` around an `await`
/// "reaches this one like any other call site", which was only true if
/// something here asked. Nothing did: `ExprKind::Await` produced the value and
/// moved on, so the rejection stayed flagged and unread until whatever ran next
/// happened to ask — usually nothing did before the top level, which is why it
/// surfaced as an unhandled rejection instead of a caught one.
pub(super) fn raise_if_thrown(builder: &mut FuncBuilder, ctx: &mut Ctx) -> EmitResult<()> {
    // Not inside a cleanup: its block has a shape the machine checks, and a
    // branch breaks it. See `Ctx::in_cleanup` for what that costs.
    if ctx.in_cleanup {
        return Ok(());
    }
    // A load from the address this body asked for once, where there is one.
    // The call remains the fallback rather than being deleted: a body emitted
    // without an entry of its own has no address to load from, and answering
    // that case with the call it always used is what keeps the two spellings
    // agreeing about the same word. See `RuntimeOp::ThrownAddress`.
    let flag = match ctx.thrown_flag {
        Some(address) => builder.word_load(address)?,
        None => {
            let asked = ctx.calls.declare(ctx.funcs, RuntimeOp::Thrown);
            builder.call(ctx.funcs, asked, &[])?[0]
        }
    };
    let zero = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(0),
    });
    let zero = builder.use_const(zero);
    let raised = builder.compare(CmpOp::Ne, flag, zero)?;

    // Created while the protected region is open, so the machine places them in
    // it — which is what makes the re-raise below land in this function's
    // handler rather than leaving the function.
    let unwinding = builder.create_block();
    let carrying_on = builder.create_block();
    builder.branch(raised, (unwinding, &[]), (carrying_on, &[]))?;

    builder.switch_to(unwinding);
    let taken = ctx.calls.declare(ctx.funcs, RuntimeOp::TakeThrown);
    let value = builder.call(ctx.funcs, taken, &[])?[0];
    builder.throw(super::protect::JS_THROW, value);

    builder.switch_to(carrying_on);
    Ok(())
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
    to_boolean(builder, ctx, value)
}

/// `ToBoolean` of a value already emitted.
///
/// Split from [`emit_condition`] because `&&` needs it and has no expression to
/// hand over: it asks about a value it emitted itself and may not evaluate the
/// other one at all. Two copies would be two answers to "is a proven boolean
/// already the answer", and the day one of them stopped short-circuiting the
/// check the other would keep calling the runtime for nothing.
pub(super) fn to_boolean(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    value: ValueId,
) -> EmitResult<ValueId> {
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
        Literal::Boolean(value) => Ok(boolean_constant(builder, *value)),
        Literal::Singleton(which) => Ok(singleton(builder, ctx, *which)),
        // Not an immediate. A string is a heap value and two occurrences of
        // `"a"` in a program are the *same* string, so what the code carries is
        // WHICH literal and the runtime holds the text — the same shape as a
        // property key, and for the same reason. An immediate here would be a
        // number that is not a string and compares wrongly with everything.
        Literal::String(text) => string_literal(builder, ctx, text),

        // Never a constant — `super::regex` says what hoisting one would break.
        Literal::Regex { pattern, flags } => super::regex::literal(builder, ctx, pattern, flags),

        // Read, held, and not emitted: a BigInt needs arbitrary-precision
        // arithmetic, which is a value this engine has no way to MAKE rather
        // than a construct it cannot express. The tree carries it so the front
        // end stops refusing a program it reads perfectly well.
        // The digits reach the runtime as an interned string and the parse
        // happens there, which is the same shape a regular-expression literal
        // takes: `1n` and `BigInt("1")` are one path, where a table of values
        // baked here would have served the literal and had nothing to say about
        // the call.
        Literal::BigInt(digits) => {
            let text = string_literal(builder, ctx, digits)?;
            Ok(call(builder, ctx, RuntimeOp::BigIntNew, &[text])?[0])
        }
    }
}

/// Whether the guarded form is worth emitting for these operands.
///
/// # The one thing a claim is allowed to do here
///
/// Take a guard AWAY that the emitter was going to add. `emit_binary`
/// speculates unconditionally: an operand pair nothing proved gets two guards,
/// an instruction and a slow path, on the bet that the operands turn out to be
/// doubles. Where the program itself says both are something else, that bet is
/// one the emitter is making against the only evidence available.
///
/// So this answers false only when BOTH operands carry a definite claim and
/// NEITHER is a number. Both, because one unknown operand can still be a
/// double and the pair can still take the instruction. Definite, because a
/// union claims nothing.
///
/// # Why a wrong claim is safe here and would not be everywhere
///
/// Because the thing it selects is the emission the compiler would have used
/// anyway when the guards failed. A `s: string` that really holds a number
/// loses its fast path and takes the runtime call — slower, and the same
/// answer. That is the campaign's rule in its cheapest form: the claim chooses
/// between two already-legal emissions and removes no check, because the path
/// it selects never had one.
fn speculation_is_worth_emitting(ctx: &Ctx, left: &Expr, right: &Expr) -> bool {
    // A body that claims nothing pays one comparison rather than two hash
    // lookups per operator, and most bodies claim nothing: the corpus is
    // JavaScript conformance tests. Measured before this line existed, the
    // predicate cost 3% of the corpus's compile time to save four blocks in
    // the handful of files that annotate.
    if ctx.claims_empty() {
        return true;
    }
    let claimed = |side: &Expr| match &side.kind {
        ExprKind::Ident(name) => ctx.claimed(*name).map(|held| held.kind()),
        _ => None,
    };
    let (Some(a), Some(b)) = (claimed(left), claimed(right)) else {
        return true;
    };
    a == super::types::Kind::Number || b == super::types::Kind::Number
}

/// Records that a binary operator's operand carried a claim.
///
/// A census hook and nothing else: it changes no emission. It lives here
/// because this is the last place the operands are expressions — one step
/// later they are values, and a value cannot have been annotated.
fn count_claimed_operands(ctx: &mut Ctx, left: &Expr, right: &Expr) {
    if !ctx.counting_claims {
        return;
    }
    for side in [left, right] {
        if let ExprKind::Ident(name) = &side.kind
            && let Some(speculation) = ctx.claimed(*name)
        {
            ctx.census.operand_claimed(speculation.kind());
        }
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
/// The same, with the speculation the claims said was pointless left out.
///
/// A separate entry point rather than a flag on the one below, so that a
/// caller with nothing to say cannot accidentally say it: `emit_binary` is
/// what eight callers want, and only the one place that has the operands as
/// expressions can answer the question this takes.
pub(super) fn emit_binary_speculating(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
    speculate: bool,
) -> EmitResult<ValueId> {
    emit_binary_inner(builder, ctx, op, a, b, speculate)
}

pub(super) fn emit_binary(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
) -> EmitResult<ValueId> {
    emit_binary_inner(builder, ctx, op, a, b, true)
}

/// The one implementation. `speculate` is false only where a claim said the
/// guarded form would never take its fast path.
fn emit_binary_inner(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    op: BinaryOp,
    a: ValueId,
    b: ValueId,
    speculate: bool,
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
            Some(Proven::Bits(bit)) => {
                let left = builder.to_int32(a)?;
                let right = builder.to_int32(b)?;
                let bits = builder.bitwise(bit, left, right)?;
                return Ok(builder.to_f64(bits)?);
            }
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
    // `speculate` is the claim's one word here, and it can only ever say
    // "do not bother": where both operands are claimed something that is not
    // a number, the two guards below fail on every pass and the call happens
    // anyway. Emitting it directly is four blocks, two guards and a join
    // fewer, and the SAME call.
    if speculate && let Some(instruction) = proven_binary(op) {
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
        // what keeps `runtime/` and `rts-core` from stating different sets —
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
        // `!==` is `!(a === b)` and is emitted as exactly that, through the one
        // definition of each half. Writing it as its own runtime entry point
        // would be a second statement of what strict equality means, and the
        // pair drifting is how `a !== b` starts disagreeing with `!(a === b)`
        // for some operand nobody tested.
        BinaryOp::StrictNotEqual => {
            let equal = call(builder, ctx, RuntimeOp::StrictEquals, &[a, b])?[0];
            return super::choice::from_bool(builder, equal, true);
        }
        // `!=` is the negation of `==`, through the one definition of each
        // half — the same reasoning `!==` follows.
        BinaryOp::LooseEqual => return Ok(compared(builder, ctx, RuntimeOp::LooseEquals, a, b)?),
        BinaryOp::LooseNotEqual => {
            let equal = call(builder, ctx, RuntimeOp::LooseEquals, &[a, b])?[0];
            return super::choice::from_bool(builder, equal, true);
        }
        BinaryOp::Exponent => RuntimeOp::Exponent,
        BinaryOp::BitAnd => RuntimeOp::BitAnd,
        BinaryOp::BitOr => RuntimeOp::BitOr,
        BinaryOp::BitXor => RuntimeOp::BitXor,
        BinaryOp::Shl => RuntimeOp::ShiftLeft,
        BinaryOp::Shr => RuntimeOp::ShiftRight,
        BinaryOp::UShr => RuntimeOp::ShiftRightUnsigned,
        // The key is written on the LEFT, and the runtime takes it in that
        // order — `k in o`, not `o has k`. Getting this backwards produces a
        // program that runs and answers about the wrong operand.
        BinaryOp::In => return Ok(compared(builder, ctx, RuntimeOp::HasProperty, a, b)?),
        // The value on the left and the constructor on the right, which is
        // how it is written — and the runtime takes them that way.
        BinaryOp::InstanceOf => return Ok(compared(builder, ctx, RuntimeOp::InstanceOf, a, b)?),
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
        // A destructuring assignment: the same walk a declaration takes, in the
        // role where a bare name writes an existing binding instead of
        // introducing one. `destructure` holds both because everything else
        // about them — the iteration protocol, when a default fires, how a rest
        // is built — is one rule.
        let AssignTarget::Pattern(pattern) = target else {
            return gap("an assignment target that is neither a place nor a pattern");
        };
        if op != AssignOp::Plain {
            // `[a] += xs` does not parse, and a compound assignment to a
            // pattern is a grammar error rather than a gap. Refused by name so
            // a tree that produced one is a defect that reports itself.
            return gap("a compound assignment to a pattern");
        }
        // Evaluated once, before anything is written: `[a, b] = f()` calls `f`
        // a single time whatever the pattern does with the result.
        let source = emit_expr(builder, scope, ctx, value)?;
        let source = tagged(builder, source);
        super::destructure::assign(builder, scope, ctx, pattern, source, value.at)?;
        // An assignment is an expression and what it produces is the SOURCE,
        // not the last element written: `let x = ([a] = [1])` is the array.
        return Ok(source);
    };
    // A property write is the other assignment target that exists today, and
    // it is handled before the local case because it does not go through the
    // scope at all: the binding is on the heap.
    if let ExprKind::Member {
        object,
        property,
        optional,
    } = &place.kind
    {
        // The replaced case first, and it does not go through a receiver at all
        // — there is no object. The order it has to keep is trivially kept: the
        // "receiver" is an identifier, so nothing was evaluated before the value.
        if let Some(field) = super::escape::field_of(ctx, object, *property, *optional) {
            let assigned = match op {
                AssignOp::Plain => emit_expr(builder, scope, ctx, value)?,
                // Still one read of the target, which is what the tree carrying
                // the operator is for.
                AssignOp::Compound(binary) => {
                    let current = super::binding::read(builder, scope, ctx, field)?;
                    let operand = emit_expr(builder, scope, ctx, value)?;
                    emit_binary(builder, ctx, binary, current, operand)?
                }
                // Unreachable: the analysis refuses a candidate this is written
                // on, precisely so that replacing an object cannot turn a
                // refusal into a program. Stated rather than assumed, because
                // the two facts live in different files.
                AssignOp::Logical(_) => return gap("`&&=`, `||=` or `??=` on a property"),
            };
            return super::binding::write(builder, scope, ctx, field, assigned);
        }
        // Receiver first, then the value: `a().x = b()` runs `a` before `b`.
        let receiver = emit_expr(builder, scope, ctx, object)?;
        let assigned = match op {
            AssignOp::Plain => emit_expr(builder, scope, ctx, value)?,
            // `o.x += v` evaluates `o` once, reads the property, and only then
            // evaluates `v` — which is the order the specification gives and
            // the order a rewrite to `o.x = o.x + v` loses, by evaluating `o`
            // twice.
            AssignOp::Compound(binary) => {
                let current = super::property::emit_read(builder, ctx, receiver, *property)?;
                let operand = emit_expr(builder, scope, ctx, value)?;
                emit_binary(builder, ctx, binary, current, operand)?
            }
            // Not the same as `o.x = o.x && v`. When the left side decides,
            // the specification performs no write at all — which is observable
            // through a setter, and through a property on a frozen object. So
            // the write below sits *inside* the branch that ran, through
            // `emit_logical_write`, rather than after it unconditionally.
            AssignOp::Logical(logical) => {
                let current = super::property::emit_read(builder, ctx, receiver, *property)?;
                let property = *property;
                return super::choice::emit_logical_write(
                    builder,
                    scope,
                    ctx,
                    logical,
                    current,
                    value,
                    |builder, _scope, ctx, new_value| {
                        super::property::emit_write(builder, ctx, receiver, property, new_value)
                    },
                );
            }
        };
        return super::property::emit_write(builder, ctx, receiver, *property, assigned);
    }

    // `super.x = v`. There was no arm here at all before this — the tree
    // could hold `SuperMember` as an assignment target and `emit_assign`
    // refused it by name regardless. The setter search and the receiver are
    // two different objects, the same way the read is — see
    // `class::emit_super_member_write`.
    if let ExprKind::SuperMember { property } = &place.kind {
        let assigned = match op {
            AssignOp::Plain => emit_expr(builder, scope, ctx, value)?,
            AssignOp::Compound(binary) => {
                let current = super::class::emit_super_member(builder, scope, ctx, property)?;
                let operand = emit_expr(builder, scope, ctx, value)?;
                emit_binary(builder, ctx, binary, current, operand)?
            }
            // The same reason the property case refuses it: when the left
            // side decides, the specification performs no write at all,
            // which a setter observes.
            AssignOp::Logical(_) => return gap("`&&=`, `||=` or `??=` on `super.x`"),
        };
        return super::class::emit_super_member_write(builder, scope, ctx, property, assigned);
    }

    // `o[e] = v`, where the key is computed. The receiver and the key are
    // evaluated before the value, which is the order the specification gives —
    // `a()[b()] = c()` runs `a`, then `b`, then `c`.
    if let ExprKind::Index { object, index, .. } = &place.kind {
        let receiver = emit_expr(builder, scope, ctx, object)?;
        // A literal key takes the named path here too. Doing only the read side
        // would leave `o["a"] = 1; o["a"]` writing a property the cache then had
        // to miss on — the two halves have to agree about which path a key
        // takes, or the optimisation costs more than it saves.
        if let Some(name) = literal_name(ctx, index) {
            let assigned = match op {
                AssignOp::Plain => emit_expr(builder, scope, ctx, value)?,
                AssignOp::Compound(binary) => {
                    let current = super::property::emit_read(builder, ctx, receiver, name)?;
                    let operand = emit_expr(builder, scope, ctx, value)?;
                    emit_binary(builder, ctx, binary, current, operand)?
                }
                AssignOp::Logical(logical) => {
                    let current = super::property::emit_read(builder, ctx, receiver, name)?;
                    return super::choice::emit_logical_write(
                        builder,
                        scope,
                        ctx,
                        logical,
                        current,
                        value,
                        |builder, _scope, ctx, new_value| {
                            super::property::emit_write(builder, ctx, receiver, name, new_value)
                        },
                    );
                }
            };
            return super::property::emit_write(builder, ctx, receiver, name, assigned);
        }
        let key = emit_expr(builder, scope, ctx, index)?;
        let assigned = match op {
            AssignOp::Plain => emit_expr(builder, scope, ctx, value)?,
            AssignOp::Compound(binary) => {
                let current = call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
                let operand = emit_expr(builder, scope, ctx, value)?;
                emit_binary(builder, ctx, binary, current, operand)?
            }
            // The same as the named case: when the left side decides, the
            // specification performs no write, which a setter observes.
            AssignOp::Logical(logical) => {
                let current = call(builder, ctx, RuntimeOp::GetIndexed, &[receiver, key])?[0];
                return super::choice::emit_logical_write(
                    builder,
                    scope,
                    ctx,
                    logical,
                    current,
                    value,
                    |builder, _scope, ctx, new_value| {
                        let new_value = tagged(builder, new_value);
                        Ok(call(
                            builder,
                            ctx,
                            RuntimeOp::SetIndexed,
                            &[receiver, key, new_value],
                        )?[0])
                    },
                );
            }
        };
        let assigned = tagged(builder, assigned);
        return Ok(call(
            builder,
            ctx,
            RuntimeOp::SetIndexed,
            &[receiver, key, assigned],
        )?[0]);
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
            let current = super::binding::read(builder, scope, ctx, *name)?;
            let operand = emit_expr(builder, scope, ctx, value)?;
            emit_binary(builder, ctx, binary, current, operand)?
        }
        // `&&=` does not evaluate its right side when the target already
        // decided. On a local the "no write happens" clause is unobservable —
        // a binding has no setter and rebinding it to what it already holds is
        // the same program — so the operator is the short-circuit and nothing
        // else. On a property it is not, which is why that case is refused
        // above rather than routed here.
        AssignOp::Logical(logical) => {
            let current = super::binding::read(builder, scope, ctx, *name)?;
            super::choice::emit_logical_from(builder, scope, ctx, logical, current, value)?
        }
    };

    // An assignment is an expression: `x = (y = 1)` needs the inner one's
    // value, and it is the assigned value rather than the binding.
    super::binding::write(builder, scope, ctx, *name, result)
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

/// One of the two boolean values, encoded.
///
/// Shared by the literal and by every emitter that answers a boolean without
/// one having been written — `!x` and `a !== b` both produce one from a branch,
/// and three copies of the encoding is three places to get the payload wrong.
/// How many arguments a call site wrote, as the operand `RuntimeOp::Call` takes.
///
/// Here rather than at each of the three sites that emit a call, because the
/// three have to agree with the runtime about what the operand MEANS — and two
/// of them were found by a measurement rather than by a reader: they kept
/// passing six operands after the count made it seven, and every program with a
/// destructuring pattern or a `yield*` stopped compiling.
pub(super) fn count_constant(builder: &mut FuncBuilder, count: usize) -> ValueId {
    let written = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(count as u64),
    });
    builder.use_const(written)
}

pub(super) fn boolean_constant(builder: &mut FuncBuilder, value: bool) -> ValueId {
    let payload = if value {
        tags::BOOL_TRUE
    } else {
        tags::BOOL_FALSE
    };
    constant(builder, tags::encode(tags::TAG_BOOL, payload))
}

/// A number, as a value the machine knows is a number.
pub(super) fn number_constant(builder: &mut FuncBuilder, value: f64) -> ValueId {
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
pub(super) fn tagged(builder: &mut FuncBuilder, value: ValueId) -> ValueId {
    builder.widen(value)
}

/// An array holding values the emitter already has, in as few crossings as it
/// can.
///
/// # Why this exists beside the array literal's own fast path
///
/// Because the same list is built in three places — an array literal past four
/// elements, the argument vector of a call with more arguments than the
/// convention carries, and the same vector for `new` — and only the first had
/// the fast path. The other two paid one crossing to make the array and one
/// more per value: eight for a six-argument call, each a thread-local, a
/// `RefCell` borrow and a bounds decision, to store values this compiler had
/// already produced.
///
/// # What the measurement said, and what it changed about this
///
/// The first draft put the first four through `ArrayOf` and APPENDED the rest,
/// on the reasoning that fewer crossings is faster. It is not: an eight-element
/// array literal went from 293/303/307 ns to 493/493/507 — **65% worse** — and
/// the literal's own path had been doing the right thing all along. An append
/// grows the element store and reconciles `length` every time; `ArrayNew(n)`
/// sizes it ONCE and every write lands in place.
///
/// So the rule here is the repository's own, arrived at from the other side:
/// **born at the size it will reach**, not walked up to. Four or fewer is one
/// crossing through `ArrayOf`, which is presized by construction; past that it
/// is `ArrayNew(n)` and a write per element, which is exactly what an array
/// literal emits and what measured best.
///
/// Takes values rather than expressions on purpose: a caller that has to
/// evaluate in source order has already done so, and one that has a value the
/// program never wrote has no expression to give.
pub(super) fn value_list(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    values: &[ValueId],
) -> EmitResult<ValueId> {
    let size = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(values.len() as u64),
    });
    let size = builder.use_const(size);

    if values.len() <= 4 {
        // Padding for the slots the count says are not real. Tagged rather
        // than `I64`, because the signature says so and a raw integer there
        // fails to widen — which is what the machine answered when the
        // literal's own path was written.
        let absent = builder.declare_const(ConstDecl::Scalar {
            repr: UNPROVEN,
            bits: ScalarBits(0),
        });
        let mut args = Vec::with_capacity(5);
        args.push(size);
        for value in values {
            args.push(tagged(builder, *value));
        }
        while args.len() < 5 {
            args.push(builder.use_const(absent));
        }
        return Ok(call(builder, ctx, RuntimeOp::ArrayOf, &args)?[0]);
    }

    let array = call(builder, ctx, RuntimeOp::ArrayNew, &[size])?[0];
    for (position, value) in values.iter().enumerate() {
        let value = tagged(builder, *value);
        let at = number_constant(builder, position as f64);
        call(builder, ctx, RuntimeOp::SetIndexed, &[array, at, value])?;
    }
    Ok(array)
}

/// What a proven pair of doubles turns an operator into.
enum Proven {
    /// A machine arithmetic instruction.
    Arith(NumOp),
    /// A machine comparison, which yields a proven boolean.
    Compare(CmpOp),
    /// A machine bitwise instruction, over the two operands read as the 32-bit
    /// integers the language says the bitwise operators read.
    ///
    /// Three instructions rather than one — two conversions and the operation —
    /// and still no call. Measured before this existed: `(a * 3) | 0` in a loop
    /// cost 17.8 ns an iteration against ~0 for the same loop without the
    /// `| 0`, because the `|` was `Call __rts_bit_or` and the `*` was already an
    /// instruction.
    Bits(BitOp),
}

/// The instruction an operator becomes when both operands are proven doubles.
///
/// `None` for the ones that stay calls whatever is known: `==` has its own
/// conversion table, and `in` and `instanceof` ask the heap.
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

        // The bitwise operators, which are pure computation over two doubles
        // and were runtime calls until this line. `ToInt32` is what made them
        // reachable — see its own documentation for why the code generator's
        // conversions could not be used directly.
        BinaryOp::BitAnd => Proven::Bits(BitOp::And),
        BinaryOp::BitOr => Proven::Bits(BitOp::Or),
        BinaryOp::BitXor => Proven::Bits(BitOp::Xor),

        // `<<` and `>>` need the shift count masked to five bits before the
        // instruction, which is one more step than this table can carry, and
        // `>>>` answers a value outside `i32` for a negative operand — its
        // result is `ToUint32`, so widening a proven `I32` would report a
        // negative number where the language says a large positive one. All
        // three stay calls, and each stays for a stated reason rather than
        // because nobody looked.
        _ => return None,
    })
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

    // O JOIN É `Bool` QUANDO OS DOIS LADOS PRODUZEM UM BOOLEANO, e essa é a
    // diferença entre uma comparação que o consumidor usa direto e uma que ele
    // manda para o runtime.
    //
    // Ele era `UNPROVEN` sempre. Como `to_boolean` atalha um `Repr::Bool` e mais
    // nada, todo `i < n` cujos operandos não fossem os dois duplos PROVADOS
    // virava `__rts_to_boolean` mais a checagem de `throw` que segue toda
    // chamada — e parâmetro de função chega sempre tagueado, então esse é o caso
    // ORDINÁRIO e não uma quina.
    //
    // Os NEGADOS ficam de fora, e o motivo é que as duas arestas discordariam:
    // `!==` e `!=` não têm entry point próprio (igualdade estrita é dita uma
    // vez), então o caminho lento deles é a chamada de igualdade com a resposta
    // invertida por `from_bool`, que responde um booleano TAGUEADO.
    let (_, negated) = runtime_binary(op).expect("every instruction has a runtime operation");
    let boolean_join = matches!(instruction, Proven::Compare(_)) && !negated;
    let result = builder.add_block_param(
        join,
        match boolean_join {
            true => Repr::Bool,
            false => UNPROVEN,
        },
    );

    builder.guard(a, Repr::F64, (left_is_number, &[]), (slow, &[]))?;

    // ONE value guarded twice is one guard. `x + x` hands the same SSA value
    // to both sides, and an SSA value's contents cannot change — so inside
    // `left_is_number`, which the first guard's success dominates, testing it
    // again can only succeed. `x + x + x` emitted nine guards for three
    // additions; three of them were this.
    //
    // Scoped to ONE emission on purpose. The general form — remembering a
    // narrowed value across operators — is unsound as it stands and useless if
    // made safe the obvious way: the join at the bottom of this function is
    // reachable from the slow path too, so the narrowed value does not reach
    // it, and a memo cleared at that join never survives to the next operator.
    // Carrying it would mean the join taking a parameter the slow path has no
    // value for.
    builder.switch_to(left_is_number);
    match a == b {
        // Jumped to rather than skipped: `both` already exists and every block
        // this builder creates must be terminated — the verifier says so, and
        // it said so about the first attempt at this. Handing `left` across
        // the edge is what makes the two operands one value.
        true => builder.jump(both, &[left])?,
        false => builder.guard(b, Repr::F64, (both, &[]), (slow, &[]))?,
    }
    builder.switch_to(both);
    let fast = match instruction {
        Proven::Arith(num) => builder.arith(num, left, right)?,
        Proven::Compare(cmp) => builder.compare(cmp, left, right)?,
        Proven::Bits(bit) => {
            let left = builder.to_int32(left)?;
            let right = builder.to_int32(right)?;
            let bits = builder.bitwise(bit, left, right)?;
            builder.to_f64(bits)?
        }
    };
    // `builder.compare` já responde `Repr::Bool`; alargar aqui era jogar fora a
    // única prova que este bloco produziu.
    let fast = match boolean_join {
        true => fast,
        false => tagged(builder, fast),
    };
    builder.jump(join, &[fast])?;

    builder.switch_to(slow);
    let (runtime, _) = runtime_binary(op).expect("every instruction has a runtime operation");
    let answered = call(builder, ctx, runtime, &[a, b])?[0];
    // `!==` has no runtime operation of its own — deliberately, so that strict
    // equality is stated once — so the slow path is the equality call with the
    // answer inverted, exactly as the unspeculated path at [`emit_binary`]
    // writes it. Reaching for a `StrictNotEquals` entry point that does not
    // exist is what this used to do, and it panicked the compiler for any
    // `a !== b` whose operands were not both proven numbers.
    if boolean_join {
        // O ESTREITAMENTO É UM GUARD e não uma afirmação, e a diferença importa.
        // Uma comparação do runtime só pode responder `true` ou `false`, então a
        // aresta de falha é inalcançável — mas escrevê-la como uma constante
        // seria o compilador AFIRMANDO algo que ele não provou, e o dia em que
        // um entry point respondesse outra coisa o programa continuaria rodando
        // com um booleano inventado. Traba: uma prova quebrada para o processo,
        // em vez de um valor errado que segue adiante.
        let impossible = builder.create_block();
        builder.guard(answered, Repr::Bool, (join, &[]), (impossible, &[]))?;
        builder.switch_to(impossible);
        builder.trap(rts_cranelift::ir::inst::TrapCode::Unreachable);
    } else {
        let answered = match negated {
            true => super::choice::from_bool(builder, answered, true)?,
            false => tagged(builder, answered),
        };
        builder.jump(join, &[answered])?;
    }

    builder.switch_to(join);
    Ok(result)
}

/// The runtime operation an operator falls back to.
///
/// Every operator `proven_binary` names has one, which is why this cannot fail
/// for a caller that asked that question first — and why the two lists are
/// beside each other rather than in different files.
/// The runtime operation an instruction falls back to, and whether the answer
/// is inverted.
///
/// The flag exists for exactly one row. `!==` has no entry point of its own —
/// strict equality is stated once, and a second definition is how `a !== b`
/// comes to disagree with `!(a === b)` for an operand nobody tested — so its
/// slow path is the equality call negated. Answering `None` for it, which is
/// what this did, made the speculative path panic on any `a !== b` whose
/// operands were not both proven numbers.
fn runtime_binary(op: BinaryOp) -> Option<(RuntimeOp, bool)> {
    Some(match op {
        BinaryOp::Add => (RuntimeOp::Add, false),
        BinaryOp::Sub => (RuntimeOp::Subtract, false),
        BinaryOp::Mul => (RuntimeOp::Multiply, false),
        BinaryOp::Div => (RuntimeOp::Divide, false),
        BinaryOp::Rem => (RuntimeOp::Remainder, false),
        BinaryOp::Less => (RuntimeOp::Less, false),
        BinaryOp::LessEqual => (RuntimeOp::LessEqual, false),
        BinaryOp::Greater => (RuntimeOp::Greater, false),
        BinaryOp::GreaterEqual => (RuntimeOp::GreaterEqual, false),
        // O caminho LENTO dos bitwise: quando um dos lados nao e um duplo, a
        // conversao e ToPrimitive e nao ToInt32, e isso le a pilha e pode
        // chamar codigo do utilizador. O rapido e tres instrucoes; este e a
        // chamada que ja existia.
        BinaryOp::BitAnd => (RuntimeOp::BitAnd, false),
        BinaryOp::BitOr => (RuntimeOp::BitOr, false),
        BinaryOp::BitXor => (RuntimeOp::BitXor, false),
        BinaryOp::StrictEqual => (RuntimeOp::StrictEquals, false),
        BinaryOp::StrictNotEqual => (RuntimeOp::StrictEquals, true),
        _ => return None,
    })
}
/// A string constant, from text this module already has.
///
/// The same path a string literal takes — `ctx.literal` numbers it and the
/// runtime holds the text — so two occurrences of one piece across a program
/// are one string, exactly as two occurrences of one literal are. Shared with
/// `template.rs`, whose literal pieces are strings nothing wrote as one.
pub(super) fn string_literal(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    text: &str,
) -> EmitResult<ValueId> {
    let which = ctx.literal(text);
    let index = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(u64::from(which)),
    });
    let index = builder.use_const(index);
    Ok(call(builder, ctx, RuntimeOp::StringConst, &[index])?[0])
}

/// Emits an array literal.
///
/// The length is known here, so the store is sized once and each element is
/// written at its own index. Not a fresh array grown per element: `[1, 2, 3]`
/// has three elements before any of them is evaluated, and a program that read
/// `length` from a `valueOf` in the middle would see the finished count.
///
/// A **hole** is not `undefined`. `[,1]` has no element zero, and `0 in [,1]`
/// is false where `[undefined,1]` answers true — so a hole is refused rather
/// than written as `undefined`, which would be the same array as far as this
/// runtime can tell and a different one as far as the language is.
fn emit_array(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    elements: &[Option<crate::syntax::Spreadable>],
) -> EmitResult<ValueId> {
    // A spread makes every index after it unknowable, so a literal containing
    // one is built by appending rather than by writing at fixed positions. The
    // ordinary literal keeps the sized-once path, which is what stops the
    // common case paying for the rare one.
    if elements
        .iter()
        .any(|element| matches!(element, Some(crate::syntax::Spreadable::Spread(_))))
    {
        let written: Vec<crate::syntax::Spreadable> = elements
            .iter()
            .map(|element| element.clone().ok_or(()))
            .collect::<Result<_, ()>>()
            .map_err(|()| EmitError::Unsupported {
                construct: "a hole beside a spread in an array literal",
            })?;
        return super::call::emit_argument_vector(builder, scope, ctx, &written);
    }

    // Up to four elements, none of them a hole, in ONE crossing. It was one to
    // make the array and one per element — five for `[a, b, c, d]` — each a
    // thread-local, a `RefCell` borrow and a bounds decision, to write values
    // the compiler had already produced.
    //
    // Four because the arguments are scalars across an `extern "C"` boundary
    // and a fifth would be a fifth register. A hole sends the literal down the
    // path below, which is what keeps an absent position absent: this entry
    // point writes exactly the elements it is given.
    if elements.len() <= 4 && elements.iter().all(|e| matches!(e, Some(crate::syntax::Spreadable::Single(_)))) {
        let count = builder.declare_const(ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(elements.len() as u64),
        });
        let count = builder.use_const(count);
        let absent = builder.declare_const(ConstDecl::Scalar {
            // Padding for the slots `count` says are not real. Tagged and
            // not I64 because the signature says so, and a raw integer there
            // fails to widen — which is what the machine answered first.
            repr: UNPROVEN,
            bits: ScalarBits(0),
        });
        let mut args = vec![count];
        for element in elements {
            let Some(crate::syntax::Spreadable::Single(value)) = element else {
                unreachable!("every element was checked to be a plain value")
            };
            let value = emit_expr(builder, scope, ctx, value)?;
            args.push(tagged(builder, value));
        }
        while args.len() < 5 {
            args.push(builder.use_const(absent));
        }
        return Ok(call(builder, ctx, RuntimeOp::ArrayOf, &args)?[0]);
    }

    let length = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(elements.len() as u64),
    });
    let length = builder.use_const(length);
    let array = call(builder, ctx, RuntimeOp::ArrayNew, &[length])?[0];

    for (position, element) in elements.iter().enumerate() {
        // UM BURACO É UMA POSIÇÃO QUE NÃO SE ESCREVE.
        //
        // `ArrayNew` preenche com o marcador de ausência, então pular a escrita
        // deixa a posição genuinamente AUSENTE — `[,1][0]` responde `undefined`
        // e `0 in [,1]` responde falso, que são respostas diferentes e agora o
        // motor as distingue.
        //
        // Este literal era recusado por nome exatamente porque escrevê-lo como
        // `undefined` mudaria a segunda resposta. O que destravou não foi mudar
        // de ideia sobre isso — foi o runtime passar a ter um marcador.
        let Some(element) = element else { continue };
        let crate::syntax::Spreadable::Single(value) = element else {
            return gap("a spread in an array literal beside a hole");
        };
        let value = emit_expr(builder, scope, ctx, value)?;
        let value = tagged(builder, value);
        let at = number_constant(builder, position as f64);
        call(builder, ctx, RuntimeOp::SetIndexed, &[array, at, value])?;
    }
    Ok(array)
}
