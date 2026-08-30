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
                let key = emit_literal(builder, ctx, &Literal::String(text.into()))?;
                return emit_binary(builder, ctx, BinaryOp::In, key, object);
            }
            if let Some(answer) =
                super::settled::typeof_equals_literal(builder, scope, ctx, *op, left, right)?
            {
                return Ok(answer);
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
        // A class field initialiser, wrapped by `emit/class.rs` so that this
        // arm can put the flag around exactly the initialiser and nothing else
        // of the constructor it was written into. Taken apart here rather than
        // in `call.rs`, because it is not a call and never becomes one.
        ExprKind::Call { .. } if super::class::field_initialiser(expr).is_some() => {
            let inner = super::class::field_initialiser(expr).expect("just tested");
            let enclosing = ctx.in_field_initializer;
            ctx.in_field_initializer = true;
            let produced = emit_expr(builder, scope, ctx, inner);
            ctx.in_field_initializer = enclosing;
            produced
        }
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
        ExprKind::Index {
            object,
            index,
            optional,
        } => {
            // A local array proven not to escape is represented by scalar bindings;
            // no array allocation, element write or indexed lookup is needed.
            if let Some(field) = super::escape::array_field_of(ctx, object, index, *optional) {
                return super::binding::read(builder, scope, ctx, field);
            }
            // A read a DESUGARING proved: the receiver is an array it made and
            // the index is a counter it minted, so none of the questions
            // `GetIndexed` asks has an answer the caller did not already have.
            // See `RuntimeOp::ElementAt`, and `foreach.rs` for the one producer.
            if let (ExprKind::Ident(array), ExprKind::Ident(at)) = (&object.kind, &index.kind)
                && ctx.is_proven_element(*array, *at)
            {
                // A call, and NOT a bounded load from an address the loop
                // hoisted once. That form existed and was removed: the address
                // belongs to the copy `Iterate` made, hoisting it replaced the
                // last read of the tagged reference to that copy, and the
                // collector then reclaimed a run the loop was still reading.
                // `foreach.rs` carries the measurement and what would make the
                // load sound.
                //
                // What survives is the proof itself, which is worth as much as
                // it ever was: `ElementAt` skips every question `GetIndexed`
                // asks, because the receiver is an array this compiler made and
                // the index is a counter it minted.
                let position = emit_expr(builder, scope, ctx, index)?;
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
            // Through a cache, which this site had none of: a literal key took
            // the inline cache above and a computed one called the runtime every
            // time, which is 36 ns against 3. `emit_read_keyed` keeps the very
            // same call as its miss path, so nothing this used to answer is
            // answered differently — only faster when the key repeats.
            super::property::emit_read_keyed(builder, ctx, receiver, key)
        }
        ExprKind::Object { properties } => super::object::emit_object(builder, scope, ctx, properties),
        ExprKind::Array { elements } => emit_array(builder, scope, ctx, elements),
        ExprKind::Function(function) => {
            // A field initialiser's `new.target` is `undefined`; a plain
            // function WRITTEN inside one has a `new.target` of its own, which
            // is whatever the call that reaches it turns out to be. An ARROW
            // does not — it takes the enclosing one, the same way it takes
            // `this` — so the flag is cleared for one and kept for the other.
            let enclosing = ctx.in_field_initializer;
            ctx.in_field_initializer = enclosing && function.captures_this;
            let produced = super::function::emit_closure(builder, scope, ctx, function);
            ctx.in_field_initializer = enclosing;
            produced
        }
        ExprKind::Class(class) => {
            // A class written inside a field initialiser is not inside it: its
            // methods and its own constructor answer for their own activations.
            let enclosing = ctx.in_field_initializer;
            ctx.in_field_initializer = false;
            let produced = super::class::emit_class(builder, scope, ctx, class);
            ctx.in_field_initializer = enclosing;
            produced
        }
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
        // The promise is produced first, and what happens next depends on who
        // resumes this frame — which is `ctx.async_parks`.
        //
        // Inside a plain `async function` it PARKS: the promise is handed out
        // at a suspension and a promise reaction re-enters the body. Everywhere
        // else — an `async function*`, whose frame `next()` already steps, and
        // a module's top level, which the host drives — `Inst::Await` lowers to
        // a call that drains until the promise settles, so the frame keeps the
        // machine. That divergence is stated in `rts-core`'s `promise/machine.rs`,
        // which is the half that does it.
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
            // The parking form, and it is `yield` with a different driver:
            // the promise is handed out at the suspension exactly as a yielded
            // value is, the frame parks, and what the suspension ANSWERS is
            // what the resumption delivered. The reaction that resumes it is
            // attached by `RuntimeOp::AsyncStart`'s runtime half, so nothing
            // here has to know that a promise is involved — which is what lets
            // one suspension mechanism serve both.
            //
            // A rejection needs nothing emitted: the resumption is made with
            // `ResumeMode::Unwind`, and the rewrite raises AT this suspension,
            // inside the regions the `await` was written in. That is why a
            // `try` around an `await` catches, and it is the same path
            // `g.throw(e)` takes.
            if ctx.async_parks {
                call(builder, ctx, RuntimeOp::GeneratorYield, &[promise])?;
                return Ok(builder.suspend());
            }
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
        // Asked of the runtime rather than proved here, because the answer is a
        // fact about the ACTIVATION — was this function reached through `new`?
        // — and the calling convention carries no bit that says so. What this
        // crate would need to decide to answer it locally is a machine
        // question, which rule 2 says belongs below rather than worked around
        // here.
        //
        // An arrow takes `new.target` from where it was WRITTEN, exactly as it
        // takes `this`, and this arm does not do that yet: an arrow is its own
        // compiled function, so the runtime answers for the arrow's own
        // activation and gives `undefined`. That is right whenever the arrow is
        // not lexically inside a constructor, which is every arrow the corpus
        // has, and wrong inside one. Named rather than silently accepted — see
        // `emit/capture.rs::arrow_reads_this` for the mechanism the fix is a
        // twin of.
        ExprKind::NewTarget if ctx.in_field_initializer => {
            // `undefined`, without asking: the specification enters a field
            // initialiser through `Call` and not `Construct`, so the answer is
            // fixed by where the code was written. The runtime could not tell
            // — the initialisers run in the constructor's own activation here
            // — which is why this is decided in the language layer.
            Ok(undefined(builder, ctx))
        }
        ExprKind::NewTarget => Ok(call(builder, ctx, RuntimeOp::NewTarget, &[])?[0]),
        ExprKind::ImportMeta => super::module::emit_import_meta(builder, ctx),
        ExprKind::ImportCall { specifier, options } => {
            super::module::emit_import_call(builder, scope, ctx, specifier, options.as_deref())
        }
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
    // A key the name table cannot hold takes the slow path. `names` is Rust
    // text, so `o["\uD83D"]` has no `Name` to intern — and the slow path is
    // where a key that is a runtime string is looked up anyway, so this loses
    // an optimisation rather than an answer.
    let text = text.as_rust()?;
    if text.chars().any(|character| character.is_ascii_digit()) {
        return None;
    }
    Some(ctx.names.intern(&text))
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
/// a property read calls a getter; a comparison coerces.
///
/// **This paragraph used to say that listing the ones that cannot "would be a
/// list to get wrong", and that the two operations which ARE the check were
/// therefore the only exceptions.** The premise was right and the conclusion was
/// not: getting it wrong does mean a throw that vanishes, which is an argument
/// for writing the list carefully and asserting it, not for refusing to write
/// it. Three places in this tree already asserted an exemption the code did not
/// implement — `runtime/mod.rs`'s own text on `NumberRemainder`, this file
/// further down, and `rts-host/tests/remainder.rs` — so the list existed in
/// prose and disagreed with the compiler, which is the worse of the two states.
///
/// It is `runtime::raising::CANNOT_RAISE` now: eight operations, each naming
/// the `rts-core` body it was read against, each closed under reading, and each
/// asserted to still exist by `rts-host`. Everything not on it raises, which is
/// the conservative default.
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
    // Two exemptions, and they are exemptions for different reasons — see
    // `runtime::raising`, which holds both lists and says why they are kept
    // apart rather than merged into one predicate.
    //
    // These two ARE the check. Asking after them asks the question the call was
    // asking, forever.
    if matches!(op, RuntimeOp::Thrown | RuntimeOp::TakeThrown) {
        return Ok(());
    }
    // And these cannot fill the slot at all. The paragraph above used to say
    // listing them "would be a list to get wrong" — it is a list now, it is
    // eight entries long, every one names the `rts-core` body it was read
    // against, and `rts-host` asserts each still exists. What made it writable
    // is that the cost of being wrong in each direction was priced rather than
    // assumed: `runtime::raising`'s module doc.
    if !op.can_raise() {
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
    let flag = match ctx.body.flag {
        Some(address) => builder.word_load(address)?,
        None => {
            let asked = ctx.calls.declare(ctx.funcs, RuntimeOp::Thrown);
            builder.call(ctx.funcs, asked, &[])?[0]
        }
    };
    // The body's own zero where it has one, and a fresh one where it does not.
    //
    // It was always fresh, and that was 1 066 `Inst::Const` in
    // `bench/analytic.ts` — a third of every constant in the file, all of them
    // the same number, one per check. `BodyState::zero` says why the entry
    // block is the right place for it and why the cost is not new.
    //
    // The fallback is not dead: a body that PARKS has neither, for the reason
    // `emit/function.rs` states, and this is the same shape as `flag`'s
    // fallback two statements up rather than a second rule.
    let zero = match ctx.body.zero {
        Some(held) => held,
        None => {
            let declared = builder.declare_const(ConstDecl::Scalar {
                repr: Repr::I64,
                bits: ScalarBits(0),
            });
            builder.use_const(declared)
        }
    };
    let raised = builder.compare(CmpOp::Ne, flag, zero)?;

    // The re-raise is SHARED among every check in the same protected region,
    // and built the first time that region asks. It was one per site, and in
    // `bench/analytic.ts` that was 1 069 copies of the identical three lines —
    // a block header, a call to `__rts_take_thrown`, and a `Throw` — which is
    // 20% of every basic block in the file.
    //
    // Sound because the block reads NOTHING from the site that branches to it:
    // no parameters, and its only instruction is a call with no arguments. So
    // there is no value that has to dominate anything, and two sites reaching
    // one copy cannot disagree about what it computes.
    //
    // The region is the key and not an optimisation detail. Where a `Throw`
    // lands is decided by the region its block is in — which is what the
    // sentence that used to be here was about: "created while the protected
    // region is open, so the machine places them in it, which is what makes the
    // re-raise land in this function's handler rather than leaving the
    // function." Sharing one block between a site inside a `try` and a site
    // outside it would route the outer one into a handler that never protected
    // it. `BodyState::reraise_in` holds that argument.
    // The ORDER below is the order this function always had, and that is not
    // incidental. An earlier draft of the sharing created the block, switched
    // into it, emitted the re-raise, switched back, and only then terminated the
    // block it had left — so the block being built sat without a terminator
    // while another was filled. It compiled, and it broke a program HEAD
    // compiles: `['a','b'].forEach(x => { s = s + x; if (x === 'b') throw … })`
    // inside a `try` reached Cranelift's verifier with "uses value from
    // non-dominating inst". So: terminate first, then fill, exactly as before.
    let region = builder.innermost_open_region();
    let carrying_on = builder.create_block();
    match ctx.body.reraise_in(region) {
        // Already built for this region by an earlier check. Nothing to emit —
        // this is the whole of what the sharing does.
        Some(built) => builder.branch(raised, (built, &[]), (carrying_on, &[]))?,
        None => {
            let made = builder.create_block();
            builder.branch(raised, (made, &[]), (carrying_on, &[]))?;
            builder.switch_to(made);
            let taken = ctx.calls.declare(ctx.funcs, RuntimeOp::TakeThrown);
            let value = builder.call(ctx.funcs, taken, &[])?[0];
            builder.throw(super::protect::JS_THROW, value);
            ctx.body.remember_reraise(region, made);
        }
    }

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

    // And a boolean that was WIDENED is still the answer. `compared` widens
    // because a comparison written in expression position is a value; a
    // condition then asks for the proof back, and without this the way to get
    // it is a call to the runtime that undoes an instruction emitted three
    // lines earlier.
    //
    // It fires wherever a comparison could not take the guarded form —
    // `typeof x === "string"` is the everyday one, since a string operand makes
    // that form's instruction unreachable. Removing the speculation there was
    // tried first and measured SLOWER for exactly this reason: the guards were
    // paying for a proof, and dropping them without this left the call.
    if let Some(source) = builder.widened_source(value)
        && builder.repr_of(source) == Repr::Bool
    {
        return Ok(source);
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
        Literal::String(text) => string_literal_units(builder, ctx, text.units()),

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
    // Before the double speculation, because a comparison against a singleton
    // this emitter materialised is settled without any of it — and the
    // speculation is actively wrong for that shape: the guard against a
    // constant `undefined` cannot succeed.
    if let Some(answer) = super::settled::singleton_equality(builder, ctx, op, a, b)? {
        return Ok(answer);
    }
    if let Some(answer) = super::settled::loose_null_equality(builder, ctx, op, a, b)? {
        return Ok(answer);
    }

    // The whole point of the pass. Two proven doubles turn every one of these
    // into a machine instruction, because the decision the runtime call exists
    // to make has exactly one answer once the operands are known.
    //
    // `+` included, and it is the one worth being explicit about: it is a call
    // in general BECAUSE it might concatenate, and proving both sides numeric
    // is precisely the evidence that it cannot.
    if builder.repr_of(a) == Repr::F64 && builder.repr_of(b) == Repr::F64 {
        // `===` on two numbers is IEEE equality, and that is not an
        // approximation of it: NaN !== NaN and +0 === -0 are what the hardware
        // comparison already answers. Which instruction each operator becomes
        // is `proven_instruction`'s to say, here and in the guarded path both.
        if let Some(instruction) = proven_binary(op) {
            return match proven_instruction(builder, instruction, a, b) {
                Ok(emitted) => Ok(emitted),
                // Only a remainder the machine cannot answer exactly reaches
                // here, and the unboxed call is what is left of it.
                Err(_) => {
                    let Some(Proven::NumberCall(entry)) = proven_binary(op) else {
                        unreachable!("only a remainder can be refused by the machine")
                    };
                    Ok(call(builder, ctx, entry, &[a, b])?[0])
                }
            };
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

/// `a === b` as the PROVEN boolean, without widening it back into a value.
///
/// [`compared`] is the expression form and ends in a widening because an
/// expression is a value. A test chain wants the opposite: it feeds a branch,
/// which takes the proof directly. `switch` is the one such chain, and it used
/// to reach the runtime unconditionally — so `switch (i & 7)` over numeric
/// labels made one call per label, with the throw check each call implies,
/// where the operands were already proven doubles at every one of them.
///
/// Which instruction a proven pair becomes is NOT decided here: it is read from
/// [`proven_binary`], the same table [`emit_binary_inner`] consumes. A second
/// answer to "what is `===` on two doubles" is how the two come to disagree.
///
/// Deliberately does NOT speculate when nothing is proven. [`emit_guarded`] is
/// right for one operator in expression position and wrong for a chain: a
/// switch with eight labels would emit eight guard pairs against the same
/// subject, and seven of them cannot tell it anything the first did not.
/// Guarding the subject ONCE, ahead of the chain, is the shape that would pay —
/// it is a different change and is not smuggled in here.
pub(super) fn strict_equals_proof(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    a: ValueId,
    b: ValueId,
) -> EmitResult<ValueId> {
    if builder.repr_of(a) == Repr::F64
        && builder.repr_of(b) == Repr::F64
        && let Some(Proven::Compare(cmp)) = proven_binary(BinaryOp::StrictEqual)
    {
        return Ok(builder.compare(cmp, a, b)?);
    }
    Ok(call(builder, ctx, RuntimeOp::StrictEquals, &[a, b])?[0])
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

/// WHICH literal spells the callee, for the call about to be issued, or `-1`
/// for a callee with nothing to name.
///
/// An operand rather than a crossing of its own. `SetCallName` was a whole
/// runtime call before every named call — a jump, a context borrow and a table
/// read — to record a name used only in the message of a `TypeError` that most
/// calls never raise. Measured 2026-08-28 by ablation, minimum of four
/// alternations per binary over the call rows of `bench/analytic.ts`: **2.3 to
/// 2.9 ns on every named call**, against a static-call control that did not
/// move (2.86 → 2.88).
///
/// `-1` rather than an `Option` because the operand is an `i64` in the calling
/// convention and the runtime already reads its literal table by index — so
/// "no name" has to be a number, and the one number that cannot be an index is
/// the honest choice.
pub(super) fn name_constant(builder: &mut FuncBuilder, name: Option<u32>) -> ValueId {
    let which = name.map_or(-1i64, i64::from);
    let spelled = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(which as u64),
    });
    builder.use_const(spelled)
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
    if ctx.holds_int32(name) && builder.repr_of(value) == Repr::F64 {
        // The binding's representation IS the integer one, so this is where the
        // value enters it. Free wherever it matters: the value reaching here is
        // the `to_f64` a bitwise operator ended with, and the machine folds
        // `ToInt32(ToF64(x))` back to `x` before lowering — so the pair this
        // looks like it forms with `binding::read` costs nothing inside a loop
        // and one conversion each way at its edges.
        //
        // Guarded on `F64` rather than trusted, because `to_int32` refuses
        // anything else and a proof that turned out not to hold would otherwise
        // be a refused program rather than a slower one. `int32::analyse` admits
        // only names `holds_number` already proved, so the guard is expected to
        // be redundant — it is here so that being wrong is survivable.
        return builder
            .to_int32(value)
            .expect("a proven-numeric binding holds Repr::F64");
    }
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
#[derive(Clone, Copy)]
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
    /// The same, for the two shifts whose count the language masks first.
    ///
    /// A separate variant rather than a flag on [`Proven::Bits`] because it is
    /// one more step and the table above could not carry it — which is what its
    /// own comment said, and what kept `<<` and `>>` runtime calls while `&`,
    /// `|` and `^` became instructions in the same expression.
    ///
    /// Measured before this existed, 20 000 000 iterations of the same loop
    /// shape: `(x << 3) >> 1` cost **1 727 ms** against **341 ms** for
    /// `(x & 255) | 1`. Five times, or about 35 ns per shift, for an operator
    /// the machine has had an instruction for all along — `BitOp::Shl` and
    /// `BitOp::Shr` were in `rts-cranelift` with no producer anywhere.
    ///
    /// `>>>` is separate from this signed shift because its result is unsigned
    /// and therefore needs a different conversion back to the numeric domain.
    Shift(BitOp),
    /// A logical right shift whose result is the unsigned 32-bit value as f64.
    ///
    /// It cannot reuse [`Proven::Shift`]: that path returns a signed `I32`, while
    /// `>>>` must preserve values from `2^31` through `2^32 - 1`.
    UnsignedShift,
    /// A runtime call that both takes and answers PROVEN doubles.
    ///
    /// (`Proven` is `Copy` because two paths ask the same value twice: the
    /// instruction is attempted, and on the machine's refusal the operator is
    /// consulted again for the call to fall back to.)
    ///
    /// The odd member, and the reason it is a `Proven` variant rather than
    /// being left on the generic path: what makes an operator "proven" here is
    /// not that it becomes an instruction — it is that the operand proofs are
    /// **spent** and a proof comes back out. `%` cannot become an instruction
    /// (`rts_cranelift::ir::inst::NumOp` carries the proof that no exact one
    /// exists) and can still do that.
    ///
    /// What the site saves against the generic call: two widenings, one
    /// narrowing, and the thrown-value check — this entry cannot run user code,
    /// so there is nothing to ask about afterwards.
    ///
    /// What it saves everywhere ELSE is larger, and is why this exists at all.
    /// `emit/proven.rs` would not prove a local reassigned through `%`, so
    /// every operator downstream of such a local went generic too.
    NumberCall(RuntimeOp),
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
        // `%` stays a call — there is no exact instruction for a double
        // remainder and `NumOp` in the machine layer carries why — but it is a
        // call that SPENDS the proofs and hands one back, which is what lets
        // `emit/proven.rs` keep proving a local that is reassigned through it.
        // Before this line, one `%` in a loop made every operator downstream
        // of that local generic as well.
        BinaryOp::Rem => Proven::NumberCall(RuntimeOp::NumberRemainder),
        // `**` for the same reason and by the same mechanism. There is no
        // exponentiation instruction on any target here, so it stays a call —
        // but a call that SPENDS the proofs and hands one back, which is what
        // lets `emit/proven.rs` keep proving a local reassigned through it.
        //
        // What the site saves against the generic entry: two widenings, one
        // narrowing and the thrown-value check. What the RUNTIME saves is
        // larger — `__rts_exponent` asks `bigint_class::binary` inside a
        // context borrow and then runs `ToPrimitive`, and a `Repr::F64` is the
        // proof that neither applies. 35.22 ns before this line.
        BinaryOp::Exponent => Proven::NumberCall(RuntimeOp::NumberExponent),
        BinaryOp::Less => Proven::Compare(CmpOp::Lt),
        BinaryOp::LessEqual => Proven::Compare(CmpOp::Le),
        BinaryOp::Greater => Proven::Compare(CmpOp::Gt),
        BinaryOp::GreaterEqual => Proven::Compare(CmpOp::Ge),
        BinaryOp::StrictEqual => Proven::Compare(CmpOp::Eq),
        BinaryOp::StrictNotEqual => Proven::Compare(CmpOp::Ne),

        // `==` and `!=` are the SAME instruction, once both operands are proven
        // doubles, and this is not an approximation of the loose comparison —
        // it is what the specification says it becomes.
        //
        // `IsLooselyEqual(x, y)` branches on the types of its operands, and the
        // arm for two Numbers is `Number::equal(x, y)`: the identical operation
        // `IsStrictlyEqual` performs. Everything that makes `==` notorious —
        // `null == undefined`, `"1" == 1`, an object's `valueOf` running — lives
        // in the arms this path has already ruled out by proving both sides are
        // doubles.
        //
        // The two cases worth naming, because they are the ones an approximation
        // would get wrong and `CmpOp::Eq` gets right: `NaN == NaN` is FALSE and
        // `+0 == -0` is TRUE, in loose and strict equality alike, and an IEEE
        // compare answers both that way.
        //
        // Measured before this line, `analytic.ts`: `loose equals int` 8.73 ns
        // against `strict equals int` 1.53 — the same comparison, 5.7 times the
        // price, because one had a table row here and the other did not.
        BinaryOp::LooseEqual => Proven::Compare(CmpOp::Eq),
        BinaryOp::LooseNotEqual => Proven::Compare(CmpOp::Ne),

        // The bitwise operators, which are pure computation over two doubles
        // and were runtime calls until this line. `ToInt32` is what made them
        // reachable — see its own documentation for why the code generator's
        // conversions could not be used directly.
        BinaryOp::BitAnd => Proven::Bits(BitOp::And),
        BinaryOp::BitOr => Proven::Bits(BitOp::Or),
        BinaryOp::BitXor => Proven::Bits(BitOp::Xor),

        // `<<` and `>>` need the shift count masked to five bits before the
        // instruction. That used to be "one more step than this table can
        // carry", and the table grew a variant instead: `Proven::Shift` is
        // `Proven::Bits` plus the mask, which is one `Bitwise(And, count, 31)`.
        //
        // The masking rule is the language's, and it is the same for both:
        // `ToInt32` the left, `ToUint32` the right, take the low five bits of
        // the count. `ToInt32` and `ToUint32` produce the SAME thirty-two bits
        // and differ only in how they are read, and the low five of those are
        // the same bits either way — so one conversion serves both, which is
        // why this needs no `ToUint32` the machine does not have.
        BinaryOp::Shl => Proven::Shift(BitOp::Shl),
        BinaryOp::Shr => Proven::Shift(BitOp::Shr),

        // `>>>` now has its own machine result path. The operands still enter as
        // the same `ToInt32` bit patterns, but the shift is logical and the
        // result rejoins `F64` through an unsigned conversion, so values with
        // bit 31 set remain positive instead of being interpreted as negative.
        BinaryOp::UShr => Proven::UnsignedShift,

        _ => return None,
    })
}


/// What a [`Proven`] operator becomes over two operands already in `Repr::F64`.
///
/// # Why this is a function and not written at each of its two call sites
///
/// Because there are exactly two — the direct path in [`emit_binary_inner`],
/// where both operands arrived proven, and the fast block of [`emit_guarded`],
/// where a guard established it — and rule 3 says a semantic rule is stated
/// once. Written twice, the two would drift: the guarded copy already had a
/// remainder fallback the direct copy did not, which is the shape of that
/// drift starting.
///
/// # The one refusal it can answer
///
/// `Err` means the MACHINE declined, which only [`Proven::NumberCall`] can
/// provoke: a remainder whose divisor is not one the machine can answer
/// exactly. Every caller's answer to that is the same call it would have made
/// anyway, and each writes it, because the two callers reach their runtime
/// operation differently.
fn proven_instruction(
    builder: &mut FuncBuilder,
    instruction: Proven,
    left: ValueId,
    right: ValueId,
) -> EmitResult<ValueId> {
    Ok(match instruction {
        Proven::Arith(num) => builder.arith(num, left, right)?,
        Proven::Compare(cmp) => builder.compare(cmp, left, right)?,
        Proven::Bits(bit) => {
            let left = builder.to_int32(left)?;
            let right = builder.to_int32(right)?;
            let bits = builder.bitwise(bit, left, right)?;
            builder.to_f64(bits)?
        }
        Proven::Shift(bit) => {
            let left = builder.to_int32(left)?;
            let right = builder.to_int32(right)?;
            // The count, masked to five bits, which is what the language says
            // and not what the machine happens to do. Cranelift's shifts take
            // their count modulo the type's width, so this `and` is very likely
            // folded away — but "very likely" is a claim about a backend, and
            // rule 12 says unproven behaviour fails safely. Emitting the mask
            // makes the IR say what JavaScript means; letting the backend say
            // it would make the meaning depend on which backend.
            let mask = builder.declare_const(ConstDecl::Scalar {
                repr: Repr::I32,
                bits: ScalarBits(31),
            });
            let mask = builder.use_const(mask);
            let count = builder.bitwise(BitOp::And, right, mask)?;
            let bits = builder.bitwise(bit, left, count)?;
            builder.to_f64(bits)?
        }
        Proven::UnsignedShift => {
            let left = builder.to_int32(left)?;
            let right = builder.to_int32(right)?;
            let mask = builder.declare_const(ConstDecl::Scalar {
                repr: Repr::I32,
                bits: ScalarBits(31),
            });
            let mask = builder.use_const(mask);
            let count = builder.bitwise(BitOp::And, right, mask)?;
            let bits = builder.bitwise(BitOp::ShrUnsigned, left, count)?;
            builder.to_f64_unsigned(bits)?
        }
        // The machine FIRST, and the call only where it refuses. `%` by a power
        // of two has an exact instruction sequence, and which divisors qualify
        // is the machine's question rather than this layer's — rule 2.
        //
        // WHICH operation, and this arm discarded it. It read
        // `Proven::NumberCall(_) => builder.arith(NumOp::Rem, ...)`, which was
        // correct for exactly as long as the remainder was the only member of
        // the variant — and stopped being correct the moment `**` joined it, in
        // the same change that added it.
        //
        // What it did is worth writing down, because it is what a silent wrong
        // answer looks like: `2 ** 9` was RIGHT and `3 ** 2` answered 1. Nine is
        // not a power of two, so the machine refused and the call happened; two
        // is, so the machine accepted and computed `3 % 2`. The operator with
        // the smaller, rounder operands was the one that broke.
        //
        // Caught by `running.rs::exponent_is_right_associative`, which asserts
        // `2 ** 3 ** 2 == 512` and got 2.
        Proven::NumberCall(RuntimeOp::NumberRemainder) => {
            builder.arith(NumOp::Rem, left, right)?
        }
        // Everything else in this variant is a call and nothing but a call.
        // `**` has no instruction on any target here — `powf` is a library
        // function — so there is no machine attempt to make, and offering one
        // would be asking the machine about an operation it was never asked to
        // express.
        //
        // `Err` from this function still means "the machine declined", and the
        // callers still answer it with the call they would have made. This arm
        // simply never produces one.
        Proven::NumberCall(_) => {
            return Err(super::EmitError::Unsupported {
                construct: "a proven-number call the machine has no instruction for",
            });
        }
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
    // An operand ALREADY proven does not get a block of its own.
    //
    // `FuncBuilder::guard` folds the test away for one of these — it asks
    // `fold::guard_answer` before widening, so nothing is boxed to be unboxed —
    // but the folded form is still a `jump` into a block whose PARAMETER
    // carries the value. That parameter is the problem: it is a different
    // `ValueId` from the constant that reaches it, so every question the
    // machine answers by looking at what defined a value stops working.
    //
    // Measured 2026-08-20: that is exactly why `bench/monte_carlo_pi.ts` did
    // not move when `% 2^k` became instructions. Its `rngState` is module-
    // scoped, so the operands arrive unproven and the `%` takes this path —
    // where `fold::divisor_is_power_of_two` was handed a block parameter
    // instead of the literal `4294967296` and refused.
    //
    // So a proven operand is used where it stands. The saving is not the
    // guard, which was already folded; it is the block, the parameter, and the
    // indirection that hid a constant from a layer built to look at constants.
    let a_proven = builder.repr_of(a) == Repr::F64;
    let b_proven = builder.repr_of(b) == Repr::F64;

    // Both proven never reaches here through `emit_binary_inner`, which takes
    // the direct form first. Handled anyway rather than assumed: the blocks
    // below are only terminated by the guards, so a call with nothing left to
    // guard would leave `slow` unterminated and the verifier would reject a
    // function for a reason nowhere near the mistake.
    if a_proven && b_proven {
        return proven_instruction(builder, instruction, a, b);
    }

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

    // Each operand is narrowed only if something still has to be established
    // about it. A proven one answers itself.
    //
    // ONE value guarded twice is one guard. `x + x` hands the same SSA value
    // to both sides, and an SSA value's contents cannot change — so inside the
    // block the first guard's success dominates, testing it again can only
    // succeed. `x + x + x` emitted nine guards for three additions; three of
    // them were this.
    //
    // Scoped to ONE emission on purpose. The general form — remembering a
    // narrowed value across operators — is unsound as it stands and useless if
    // made safe the obvious way: the join at the bottom of this function is
    // reachable from the slow path too, so the narrowed value does not reach
    // it, and a memo cleared at that join never survives to the next operator.
    // Carrying it would mean the join taking a parameter the slow path has no
    // value for.
    let left = match a_proven {
        true => a,
        false => {
            let narrowed = builder.create_block();
            let param = builder.add_block_param(narrowed, Repr::F64);
            builder.guard(a, Repr::F64, (narrowed, &[]), (slow, &[]))?;
            builder.switch_to(narrowed);
            param
        }
    };
    let right = if b_proven {
        b
    } else if a == b {
        // The same value, and `left` is what it narrowed to.
        left
    } else {
        let narrowed = builder.create_block();
        let param = builder.add_block_param(narrowed, Repr::F64);
        builder.guard(b, Repr::F64, (narrowed, &[]), (slow, &[]))?;
        builder.switch_to(narrowed);
        param
    };

    let fast = proven_instruction(builder, instruction, left, right)
        .or_else(|_| -> EmitResult<ValueId> {
            // Only `NumberCall` can refuse — a remainder whose divisor the
            // machine cannot answer exactly — and the call is what is left.
            let Proven::NumberCall(op) = instruction else {
                unreachable!("only a remainder can be refused by the machine")
            };
            Ok(call(builder, ctx, op, &[left, right])?[0])
        })?;
    // `builder.compare` já responde `Repr::Bool`; alargar aqui era jogar fora a
    // única prova que este bloco produziu.
    let fast = match boolean_join {
        true => fast,
        false => tagged(builder, fast),
    };
    builder.jump(join, &[fast])?;

    builder.switch_to(slow);
    let (runtime, _) = runtime_binary(op).expect("every instruction has a runtime operation");
    // The WIDENED operands, always: this path is the generic one, and its
    // entry point takes JavaScript values. A proven operand that skipped the
    // guard above still has to be boxed to be handed over here.
    //
    // Widened HERE, inside the slow block, and that is the whole of this
    // change. The two `tagged` calls used to sit above the guards, in the block
    // the FAST path runs through — so every pass of a loop paid for a value
    // whose only consumer is this call, on the path it does not take. Widening
    // an `F64` is a bitcast, an `iconst(CANONICAL_NAN)`, an `fcmp` and a
    // `select` (`lower/value.rs`): a GPR/XMM domain crossing and a cmov.
    //
    // Nothing removes it for us. `target/mod.rs` records that Cranelift's
    // default `opt_level` is `none`, which gates out the whole egraph mid-end —
    // no GVN, no LICM, no sinking. A value computed on the fast path and used
    // only on the slow one stays exactly where it was put.
    //
    // Sound because `a` and `b` dominate this block: they were defined before
    // the block the guards were emitted in, and every path here goes through
    // one of those guards.
    //
    // And passing the RAW operand to `guard` above is not a second change — it
    // emits the same IR or better. `FuncBuilder::guard` calls
    // `fold::guard_answer` before widening and `widen_if_needed` after, so the
    // widening still happens where it is needed; what changes is that the fold
    // now sees the operand rather than a `Widen` of it, which is the same
    // "indirection that hid a constant from a layer built to look at constants"
    // this function's own comment names further up.
    let widened_a = tagged(builder, a);
    let widened_b = tagged(builder, b);
    let answered = call(builder, ctx, runtime, &[widened_a, widened_b])?[0];
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
        // The generic exponentiation, which is what the guarded path falls back
        // to when an operand is not a proven double — the same pairing `Rem`
        // has one line up, and the same rule that put the shifts and the loose
        // equalities here: this table and `proven_binary` must agree about which
        // operators the guarded path can take.
        BinaryOp::Exponent => (RuntimeOp::Exponent, false),
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
        // The two shifts, for the same reason and by the same rule: this table
        // and `proven_binary` must agree about which operators the guarded path
        // can take, because that path emits the instruction on the fast side
        // and THIS call on the slow one.
        //
        // They disagreed for exactly one build. `proven_binary` learned `<<`
        // and `>>` and this did not, and the guarded path's
        // `expect("every instruction has a runtime operation")` fired on
        // `1 << 0` — which is the invariant working, and is why that `expect`
        // is an `expect` and not a silent fallback.
        //
        // `>>>` has the same fallback as its new fast path, but a separate
        // machine instruction because the result is unsigned.
        BinaryOp::Shl => (RuntimeOp::ShiftLeft, false),
        BinaryOp::Shr => (RuntimeOp::ShiftRight, false),
        BinaryOp::UShr => (RuntimeOp::ShiftRightUnsigned, false),
        // The loose pair, and they are here for the same reason the shifts are:
        // `proven_binary` names them, so the guarded path emits an instruction
        // on the fast side and needs THIS call on the slow one. Adding a row
        // there and not here is what fired the `expect` below on `1 << 0`.
        //
        // `LooseEquals` is a different entry point from `StrictEquals` and not a
        // flag on it: the whole of `==` is the arms for operands that are not
        // both numbers, and the slow path is exactly where those arrive.
        BinaryOp::LooseEqual => (RuntimeOp::LooseEquals, false),
        BinaryOp::LooseNotEqual => (RuntimeOp::LooseEquals, true),
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
    string_const(builder, ctx, which)
}

/// The same, for a literal whose text is code units.
///
/// A JavaScript string literal is UTF-16, and `"\uD83D"` is one unit that no
/// `&str` can hold — see `crate::syntax::Text`. The `&str` form above stays
/// because the emitter also synthesises literals from names it holds as Rust
/// text, and both mint from one table so the two spellings of one string are
/// one string.
pub(super) fn string_literal_units(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    units: &[u16],
) -> EmitResult<ValueId> {
    let which = ctx.literal_units(units);
    string_const(builder, ctx, which)
}

/// The call that turns a literal's number into its value.
fn string_const(builder: &mut FuncBuilder, ctx: &mut Ctx, which: u32) -> EmitResult<ValueId> {
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
        // A HOLE among them is not a reason to refuse. `[1, , ...xs]` is
        // ordinary elision, and the appending path can express it: the marker
        // `ArrayNew` already fills an unwritten position with is a value like
        // any other here, so appending it lengthens the array while leaving the
        // position genuinely absent — `1 in [1, , ...[]]` stays false.
        //
        // Not routed through `call::emit_argument_vector`, which is the same
        // loop for a CALL: an argument list has no holes to express, and giving
        // it a case for one would put a shape the callers cannot produce into
        // the path every call takes.
        if elements.iter().any(Option::is_none) {
            let zero = builder.declare_const(ConstDecl::Scalar {
                repr: Repr::I64,
                bits: ScalarBits(0),
            });
            let zero = builder.use_const(zero);
            let array = call(builder, ctx, RuntimeOp::ArrayNew, &[zero])?[0];
            let hole = ctx.model.hole().word();
            for element in elements {
                let (value, op) = match element {
                    None => {
                        let marker = constant(builder, hole);
                        call(builder, ctx, RuntimeOp::ArrayAppend, &[array, marker])?;
                        continue;
                    }
                    Some(crate::syntax::Spreadable::Single(value)) => {
                        (value, RuntimeOp::ArrayAppend)
                    }
                    Some(crate::syntax::Spreadable::Spread(value)) => {
                        (value, RuntimeOp::ArrayAppendAll)
                    }
                };
                let value = emit_expr(builder, scope, ctx, value)?;
                let value = tagged(builder, value);
                call(builder, ctx, op, &[array, value])?;
            }
            return Ok(array);
        }
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
            // Not a gap any more: a literal containing a spread returned at the
            // top of this function, so reaching here with one would be that
            // branch failing to fire rather than anything a program can write.
            unreachable!("a literal with a spread took the appending path above")
        };
        let value = emit_expr(builder, scope, ctx, value)?;
        let value = tagged(builder, value);
        let at = number_constant(builder, position as f64);
        call(builder, ctx, RuntimeOp::SetIndexed, &[array, at, value])?;
    }
    Ok(array)
}
