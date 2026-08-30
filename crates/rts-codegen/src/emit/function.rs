//! A function: its own body of machine code, and the value that reaches it.
//!
//! # The convention, and why it is fixed
//!
//! Every compiled JavaScript function has one shape:
//!
//! ```text
//! (environment, this, a0, a1, a2, a3) -> value
//! ```
//!
//! JavaScript's arity is dynamic — a call may pass fewer arguments than the
//! function declares, or more. Fewer is padded with `undefined` at the call,
//! because the callee's parameters exist whether or not anything was passed.
//!
//! More goes in a vector the **runtime** holds, reached by `call_with_args`.
//! Not a caller-allocated stack slot, which is what a real engine hands over
//! and what this compiler cannot emit — and choosing something else *because*
//! of that would be rule 2's mistake. This is not that: where the arguments of
//! a running call live is a runtime question, the same kind as where a string's
//! text lives, and this layer says "call with these" without learning the
//! answer. The stack-slot convention is still the end state, and what it buys
//! is that the vector stops being allocated at all.
//!
//! What stays fixed at [`ARGUMENT_SLOTS`] is the **declaration**: a fifth
//! parameter has no slot to arrive in, so it is refused by name rather than
//! reading `undefined` forever.
//!
//! Both extra parameters earn their place. `environment` is what makes a
//! closure a closure; `this` cannot be one of the arguments because
//! `f(a)` and `o.f(a)` pass the same arguments and differ only in it.
//!
//! # Why the body is a second `Function` and not an inlined block
//!
//! Because a function is called, possibly from somewhere that does not exist
//! yet, and possibly more than once. Inlining at the point of definition would
//! be a compiler deciding a program's structure from its syntax — and it would
//! be wrong the first time a closure escaped.
//!
//! # What the environment is
//!
//! An ordinary object whose properties are the captured names, plus a link to
//! the environment of the function that created it. Ordinary on purpose: it
//! reuses the shape tree, the property caches and the allocator rather than
//! introducing a second kind of storage with its own layout rules.
//!
//! Two closures made in one activation are handed the **same** object, which is
//! what makes them share the variable — and that is the whole observable content
//! of a closure, so it is what the tests pin.

use rts_cranelift::ir::{FuncBuilder, FuncId, Function as MachineFunction, Signature, ValueId};

use super::{Ctx, EmitResult, Loops, Scope, UNPROVEN};
use super::{binding, capture, expr};
use crate::names::Name;
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{
    BindingKind, Expr, ExprKind, ForEachTarget, ForInit, Function, FunctionBody, Stmt, StmtKind,
};

/// Which entry-block parameter is the environment.
pub const ENVIRONMENT_PARAM: usize = 0;

/// Which is the receiver.
pub const THIS_PARAM: usize = 1;

/// How many parameters a compiled function has in total.
const TOTAL_PARAMS: usize = 2 + ARGUMENT_SLOTS;

/// The shape every compiled JavaScript function has.
///
/// Every parameter and the return are `Tagged`, because that is what a
/// JavaScript function is at the boundary: a caller cannot know what it is
/// handing over and a callee cannot know what it will get back. A signature
/// claiming anything narrower would be a claim about the program that nothing
/// proved.
pub fn signature() -> Signature {
    Signature {
        params: vec![UNPROVEN; TOTAL_PARAMS],
        returns: vec![UNPROVEN],
        ..Signature::default()
    }
}

/// Emits a function, and answers the callable value at the site that wrote it.
///
/// Two things happen and they are in different functions: the body becomes its
/// own machine function, added to what this compilation will place, and *here*
/// a closure is made from that function's address and the environment currently
/// in scope.
pub fn emit_closure(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
) -> EmitResult<ValueId> {
    emit_closure_with(
        builder,
        scope,
        ctx,
        function,
        None,
        Definition::Expression,
        Constructs::Maybe,
    )
}

/// The same, for a METHOD — of a class body or of an object literal.
///
/// A method is not a constructor. `({ m() {} }).m.prototype` is `undefined` and
/// `new obj.m()` is a `TypeError`, which is the specification's own distinction:
/// a method definition has no `[[Construct]]`, so it never gets the `prototype`
/// object that having one requires.
///
/// A named entry rather than a flag on [`emit_closure`], because the three call
/// sites that want it — a class method, and an object literal's shorthand and
/// getter/setter forms — are naming a language rule, and the file already spells
/// that difference this way for a declaration and for a late `this`.
pub fn emit_closure_method(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
) -> EmitResult<ValueId> {
    emit_closure_with(
        builder,
        scope,
        ctx,
        function,
        None,
        Definition::Expression,
        Constructs::Never,
    )
}

/// Whether a function may be reached by `new`, which decides whether it is given
/// a `prototype` object at all.
///
/// # Why the language answers this and the runtime does not
///
/// The runtime sees a code address and an environment. Whether that address came
/// from an arrow, a method, or a declaration is a fact about the SOURCE, and this
/// is the only layer that has it. `closure_new` used to build a `prototype` and a
/// `constructor` back-link for every function there is, which is wrong for two of
/// the three — measured against Node on 2026-08-25: `arrow.prototype` answered an
/// object where the language says `undefined`, and `new arrow()` did not throw.
#[derive(Clone, Copy, PartialEq)]
enum Constructs {
    /// A method definition. Never constructible, whatever it is written over.
    Never,
    /// Anything else — subject to the arrow test below, which is the other half
    /// of the same question and is asked where `captures_this` is already read.
    Maybe,
}

/// The same, for a function DECLARATION.
///
/// One caller — [`hoist`], which is the only place a declaration becomes a
/// closure. The difference is [`Definition`]'s, and it is a language rule
/// rather than a convenience.
pub fn emit_closure_declared(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
) -> EmitResult<ValueId> {
    emit_closure_with(
        builder,
        scope,
        ctx,
        function,
        None,
        Definition::Declaration,
        Constructs::Maybe,
    )
}

/// The same, for a body that holds `this` rather than being handed it.
///
/// One caller: a derived constructor, where `this` does not exist until
/// `super()` answers one. See [`Scope::bind_this_late`] for why that cannot be
/// the block parameter every other function's `this` is.
pub fn emit_closure_binding_this_late(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
    held: Name,
) -> EmitResult<ValueId> {
    emit_closure_with(
        builder,
        scope,
        ctx,
        function,
        Some(held),
        Definition::Expression,
        Constructs::Maybe,
    )
}

/// All three, differing only in whether `this` is held and in how the function
/// was named.
fn emit_closure_with(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
    late_this: Option<Name>,
    definition: Definition,
    constructs: Constructs,
) -> EmitResult<ValueId> {
    // TWO questions, not one, and conflating them was a bug: a generator HAS a
    // `prototype` and is NOT constructible. The matrix below was read off Node
    // 25.9 on 2026-08-25 rather than derived — sixteen forms, every combination
    // of arrow, method, `async`, generator and class:
    //
    //     decl / function expression   prototype: yes   new: ok
    //     class                        prototype: yes   new: ok
    //     generator (any form)         prototype: yes   new: THROWS
    //     arrow, async arrow           prototype: no    new: THROWS
    //     async function               prototype: no    new: THROWS
    //     method (obj or class)        prototype: no    new: THROWS
    //
    // A "plain" function is one that is neither an arrow nor a method:
    // `captures_this` is what says arrow — this struct's own documentation calls
    // it "the one thing arrows actually change" — and the caller says method by
    // reaching `emit_closure_method`.
    let plain = constructs == Constructs::Maybe && !function.captures_this;
    // A generator's `prototype` is not for construction at all: it is the object
    // its iterator inherits from, which is why it survives on a form that `new`
    // refuses, and why it is there even for a generator METHOD.
    let has_prototype = function.is_generator || (plain && !function.is_async);
    let constructs = plain && !function.is_async && !function.is_generator;
    // An ARROW reads `this` as a NAME, from the environment the enclosing
    // function put it in. `Scope::late_this` is the mechanism — it already
    // exists for a derived constructor, where `this` is also a name rather than
    // the receiver — so nothing new is invented here, only pointed at a second
    // case.
    let late_this = match function.captures_this {
        // Only when the enclosing function actually handed one over. It is
        // reachable exactly when that function declared it, which it does when a
        // walk found an arrow inside it reading `this` — so asking the scope is
        // asking the same question from the other side, and it cannot answer
        // yes where the declaration did not happen.
        //
        // Setting it unconditionally made every arrow take the name path,
        // including the ones in bodies where nothing was declared, and 739 files
        // failed with `Unbound("__rts_this")`.
        true => {
            let held = ctx.names.intern("__rts_this");
            scope
                .reachable()
                .iter()
                .any(|(name, _)| *name == held)
                .then_some(held)
        }
        false => late_this,
    };
    let id = emit_function(ctx, scope, function, late_this, definition, has_prototype, constructs)?;

    // The address is not a number known here — it is a relocation the
    // destination fills in, which is the whole reason the machine had to grow
    // `FuncAddr` before any of this could be written.
    let code = builder.func_addr(ctx.funcs, id)?;

    // What the function closes over is the environment of whoever is defining
    // it. A function defined where nothing is captured closes over nothing, and
    // `undefined` is what it is handed — reading a name through it would be a
    // defect in the analysis rather than something to guard against here.
    let environment = match scope.environment() {
        Some(environment) => environment,
        None => expr::undefined(builder, ctx),
    };
    Ok(expr::call(builder, ctx, RuntimeOp::ClosureNew, &[code, environment])?[0])
}

/// Emits a function's body as a machine function, and answers its id.
fn emit_function(
    ctx: &mut Ctx,
    enclosing: &Scope,
    function: &Function,
    late_this: Option<Name>,
    definition: Definition,
    has_prototype: bool,
    constructs: bool,
) -> EmitResult<FuncId> {
    // Two refusals and not one, because they are two constructs and the
    // measurement that ranks this crate's gaps counts by this string. Merged,
    // they were the largest single entry in that ranking and it was impossible
    // to tell which of the two it was — the corpus turned out to hold 52 async
    // functions across 15 files and 203 generators across 34, which is not the
    // split the merged number suggested.
    // A generator is no longer refused here. What is still refused is `yield*`,
    // and it is refused where it is emitted rather than by a pre-pass over the
    // body: it forwards `next`, `throw` and `return` to an inner iterator, which
    // is a loop over the iteration protocol rather than a suspension. See
    // `expr::yielded`.
    let (parameters, mut prologue, parameter_claims) = bind_parameters(ctx, function);
    // A rest parameter that destructures is the same desugaring as a parameter
    // that does: bind the vector to a name, then unpack it with an ordinary
    // declaration. It was refused separately, and the refusal only made sense
    // while a parameter could not be unpacked either.
    let rest = match &function.rest_parameter {
        Some(crate::syntax::Pattern::Name(name)) => Some(*name),
        Some(pattern) => {
            let held = ctx.names.intern("__rts_rest");
            prologue.push(Stmt {
                kind: StmtKind::Declare {
                    kind: crate::syntax::BindingKind::Let,
                    bindings: vec![crate::syntax::Binding {
                        target: pattern.clone(),
                        value: Some(Expr {
                            kind: ExprKind::Ident(held),
                            at: function.at,
                        }),
                        claim: None,
                    }],
                },
                at: function.at,
            });
            Some(held)
        }
        None => None,
    };

    // A concise arrow body is `x => e`, which produces `e`. Wrapped in a
    // synthetic `return` here rather than in the parser, because the tree
    // deliberately records which was written — `x => ({a: 1})` needs its
    // parentheses precisely because a `{` would have been a block, and a
    // rewrite at parse time loses that distinction for every later reader.
    let synthesised;
    let body: &[Stmt] = match &function.body {
        FunctionBody::Block(body) => body,
        FunctionBody::Expression(value) => {
            synthesised = [Stmt {
                kind: StmtKind::Return(Some((**value).clone())),
                at: function.at,
            }];
            &synthesised
        }
    };

    // The prologue goes FIRST, so a destructured or defaulted parameter is a
    // binding before any statement can read it. Prepended rather than emitted
    // separately, because it is ordinary code: `capture.rs` has to see these
    // declarations to decide whether a nested function captures one of them.
    let body: Vec<Stmt> = match prologue.is_empty() {
        true => body.to_vec(),
        false => prologue.into_iter().chain(body.iter().cloned()).collect(),
    };
    let body = body.as_slice();

    // An async body may contain `Await`, and the verifier refuses that
    // instruction in a function whose signature does not say so. Declared on the
    // BODY and not on the wrapper: the wrapper only calls and settles, and a
    // signature claiming a capability the function does not use would be a claim
    // nothing checked.
    let mut shape = signature();
    shape.may_suspend = function.is_async || function.is_generator;
    let sig = ctx.funcs.declare_signature(shape);
    let id = ctx.funcs.declare_function(sig);
    // Whether an `await` in THIS body parks it. Saved and restored around the
    // emission rather than set once, because a nested function's `await` parks
    // the nested frame and an ordinary function nested in an async one has no
    // frame to park at all. Only a PLAIN async function: an `async function*`
    // is already stepped through its own object, so a second party resuming the
    // same frame could not be told apart from `next()`.
    let outer_parks = ctx.async_parks;
    ctx.async_parks = function.is_async && !function.is_generator;
    // Strictness, saved and restored like the flag above and set in ONE
    // direction: `"use strict"` makes this body and every function written
    // inside it strict, and nothing makes a body sloppy that was not already.
    // That asymmetry is the language's, so clearing here is all the inheritance
    // the nested emissions need — they read the cleared flag.
    let outer_sloppy = ctx.sloppy;
    if ctx.sloppy && super::nonstrict::is_strict(&function.directives) {
        ctx.sloppy = false;
    }
    let emitted = emit_body(
        ctx,
        enclosing,
        &parameters,
        &parameter_claims,
        body,
        function.captures_this,
        rest,
        late_this,
        // What the body binds its OWN name to, decided here because only this
        // level has both the tree and the enclosing scope. See [`self_binding`].
        self_binding(enclosing, function, definition),
        &[],
        // A nested function is not a module: it has no specifier and nothing to
        // publish. Passing the enclosing module's would make every closure
        // re-publish its exports on every call.
        None,
        &[],
    );
    ctx.async_parks = outer_parks;
    ctx.sloppy = outer_sloppy;
    let emitted = emitted?;
    // Set on the EMITTED function and not only on the declared signature:
    // `emit_body` builds its own `Function` from `signature()`, so a flag set on
    // the registry copy alone never reached the thing the verifier reads. It
    // refused every `await` with `UndeclaredSuspension` until this line existed.
    let mut emitted = emitted;
    emitted.signature.may_suspend = function.is_async || function.is_generator;
    // The name, for a stack trace. Recorded here rather than derived later:
    // this is the one place that has both the identifier and the tree it came
    // from, and a name recovered from anywhere else would be a second answer.
    // Every function, named or not, and with its ARITY beside the name.
    //
    // It was named ones only, because the one consumer was a stack trace and an
    // unnamed frame has no useful label. `f.length` needs the other half for
    // every function there is — `(function(){}).name` is `""` and its `length`
    // is `0`, and both are properties the language promises. The trace still
    // prints nothing for an empty name; that filter moved to the printer, which
    // is where it was a statement about traces rather than about functions.
    //
    // `length` is the count BEFORE the first default and before the rest, which
    // is what `SetFunctionLength` says: `function f(a, b = 1, ...c)` has
    // `length` 1.
    let arity = function
        .parameters
        .iter()
        .take_while(|parameter| parameter.default.is_none())
        .count() as u32;
    // Its own name, or the one a binding lent it: `const f = function () {}`
    // is called `f`, which is NamedEvaluation. Taken rather than read, so a
    // function nested inside that initialiser does not inherit it — see
    // `Ctx::take_lent_name`.
    let lent = ctx.take_lent_name();
    let text = function
        .name
        .or(lent)
        .map(|name| ctx.names.text(name).to_owned())
        .unwrap_or_default();
    ctx.function_names.push((id, text.clone(), arity, has_prototype, constructs));
    ctx.pending.push((id, emitted));
    // The id recorded above is the BODY's, and for a generator or an `async`
    // function that is not the one a closure is made from: both return a
    // WRAPPER below, and `emit_closure_with` takes the address of whatever this
    // answers. So the runtime looked the wrapper's address up, found nothing,
    // and fell through to its "undescribed" answer — which is why
    // `(async function af(){}).name` and `.length` read `undefined` where Node
    // reads `"af"` and `2`, and why an `async function` kept a `prototype` the
    // language says it does not have. One defect, three symptoms, measured
    // 2026-08-25.
    //
    // BOTH are recorded rather than moving the entry, because the two consumers
    // want different addresses and always did: a stack trace names the frame
    // that is RUNNING, which is the body, while `.name`, `.length` and
    // constructibility are properties of the callable a program holds, which is
    // the wrapper. Moving it would have closed these three and opened a hole in
    // `trace`.
    let named = |ctx: &mut Ctx, wrapper: FuncId| {
        if wrapper != id {
            ctx.function_names.push((wrapper, text, arity, has_prototype, constructs));
        }
        wrapper
    };
    if function.is_generator {
        // The body is not called here and is not called by the caller either:
        // the wrapper hands its ADDRESS to the runtime, which parks a frame
        // against it. `ctx.generators` is what tells the host to put this
        // function through `resumable_form` before placing it.
        ctx.generators.push(id);
        let made = super::wrap::generator(ctx, id)?;
        // `is_async` is asked AFTER the generator wrapper rather than instead of
        // it, and that order is the whole of what an `async function*` is: it
        // parks a frame like a generator AND it is stepped by the async
        // protocol. This returned here, so `async function* g()` answered a
        // plain generator — one with no `[Symbol.asyncIterator]`, which is the
        // only thing `for await` asks a source for, so every `for await` over a
        // declared async generator died on `undefined is not a function`.
        let wrapped = match function.is_async {
            false => made,
            true => super::wrap::async_generator(ctx, made)?,
        };
        return Ok(named(ctx, wrapped));
    }
    match function.is_async {
        false => Ok(id),
        // The body is a PARKED frame, exactly as a generator's is, so it goes
        // through the same rewrite: `ctx.generators` is the list the host puts
        // through `frame::resumable_form`. What differs is who resumes it —
        // a promise reaction rather than `next()` — and that lives entirely in
        // the runtime half of `RuntimeOp::AsyncStart`.
        true => {
            ctx.generators.push(id);
            let wrapped = super::wrap::async_function(ctx, id)?;
            Ok(named(ctx, wrapped))
        }
    }
}

/// Which of the two ways a function came to be named — the whole of what the
/// body's binding for its own name depends on. See [`self_binding`].
#[derive(Clone, Copy, PartialEq)]
enum Definition {
    /// `function f() {}` as a statement.
    Declaration,
    /// Everything else: a function expression, an arrow, a method.
    Expression,
}

/// What a body binds its own name to, and when it binds one at all.
///
/// The binding is `RuntimeOp::RunningFunction` — the callable currently being
/// invoked. That is exactly right for a function EXPRESSION, whose name exists
/// for the body and nowhere else, and it is the ONLY answer available there.
///
/// # Why a declaration is withheld
///
/// Its name is an ordinary WRITABLE binding of the enclosing scope, and a
/// program writes it. Shadowing it made the write land on a value binding local
/// to the body, invisible outside, so
///
/// ```text
/// let init = 0;
/// function lazy(n) { init++; lazy = m => m + 100; return lazy(n); }
/// lazy(1); lazy(2);
/// ```
///
/// ran the first body TWICE: the inner call saw the new function, the outer name
/// never moved. That is how every Babel-transpiled `async` function memoises
/// itself, so it is not an exotic program.
///
/// Only where the enclosing binding is one the body can actually REACH, which is
/// the environment form. A declaration nothing captures stays a value binding of
/// the enclosing activation, invisible from inside a second machine function —
/// withholding there would turn `function fact(n) { … fact(n - 1) … }` from a
/// working recursion into an unbound name.
///
/// # Why a generator is withheld too
///
/// For a different reason. The function currently running inside a generator is
/// the BODY — a `resumable_form` taking a frame reference and answering a
/// finished flag — so `function* self(n) { self(n - 1) }` called something that
/// is not the generator at all: it hands a heap cell where a frame pointer
/// belongs. The enclosing binding is the wrapper, which is what every other
/// caller receives, so where one exists it is the right answer and this is not.
///
/// A named generator EXPRESSION keeps the binding, wrong value and all: nothing
/// else in scope answers, and turning a wrong value into an unbound name is a
/// second defect rather than a fix.
fn self_binding(enclosing: &Scope, function: &Function, definition: Definition) -> Option<Name> {
    let name = function.name?;
    if function.is_generator || function.is_async {
        return enclosing.lookup(name).is_none().then_some(name);
    }
    if definition == Definition::Declaration
        && matches!(
            enclosing.lookup(name),
            Some(super::scope::Binding::InEnvironment { .. })
        )
    {
        return None;
    }
    Some(name)
}

/// Emits a body against the convention, with the environment chain set up.
/// `imports` is bound before the first statement and is empty for every body but
/// a module's own: an import introduces a name in the scope it is written in,
/// which is this one, and nothing downstream learns that it came from a module.
/// `parameter_claims` is what the parameters were annotated with, which the
/// tree cannot be asked for here: a plain name with no default binds directly at
/// entry and has no `Binding` to carry its claim, so `bind_parameters` is the
/// only place it can be read. Empty for the two module-level callers, which have
/// no parameter list to annotate.
pub(super) fn emit_body(
    ctx: &mut Ctx,
    enclosing: &Scope,
    parameters: &[Name],
    parameter_claims: &[(Name, crate::syntax::Claim)],
    body: &[Stmt],
    captures_this: bool,
    rest: Option<Name>,
    late_this: Option<Name>,
    self_name: Option<Name>,
    imports: &[crate::syntax::Import],
    module: Option<&str>,
    publications: &[super::module::Publication],
) -> EmitResult<MachineFunction> {
    // A function WRITTEN INSIDE a `with` body closes over the `with` scope: its
    // free names resolve against the object first, every time it is called,
    // wherever it is called from. This engine cannot express that — the objects
    // are SSA values of the enclosing activation, and a separately compiled
    // function has no access to them — so it is refused rather than emitted
    // with lexical resolution, which would answer a DIFFERENT binding without
    // saying so.
    //
    // Refusing by name is what this crate does with what it cannot answer, and
    // it is what `with` did as a whole until this change; the cost of guessing
    // instead is a program that runs and reads the wrong variable.
    if !ctx.with_objects.is_empty() {
        return super::expr::gap("a function declared inside `with`");
    }
    // An import is bound at entry and may be read from inside a nested function,
    // exactly as a parameter is — so it is a capture CANDIDATE like one. Without
    // this the name is declared as a plain local, a closure that reads it finds
    // nothing reachable, and every `describe(… test(…) …)` in the corpus reported
    // `test` as a name nothing introduces.
    let mut candidates = parameters.to_vec();
    // `...rest` is bound the same way a parameter is — directly, at entry,
    // never through a statement `declared_by_statement` would see — so a
    // closure reading it needs the same listing a parameter gets. Absent
    // here, `values` in `strings.reduce((acc, s, i) => …values[i]…)` inside
    // a function declared `(...values)` was a mention nothing counted as
    // captured, and the arrow found no slot for it: `Unbound("values")`.
    if let Some(name) = rest {
        candidates.push(name);
    }
    for import in imports {
        for binding in &import.bindings {
            candidates.push(match binding {
                crate::syntax::ImportBinding::Named { local, .. } => *local,
                crate::syntax::ImportBinding::Default(local) => *local,
                crate::syntax::ImportBinding::Namespace(local) => *local,
            });
        }
    }
    // The CommonJS five, for the reason the imports one line up are candidates:
    // they are bound at entry and read from inside a nested function — a
    // `module.exports = function () { return require("./x") }` is the ordinary
    // way the whole Node corpus is written, and a name nothing lists as a
    // candidate is a plain local no closure can reach.
    //
    // Listed only when the body is a module's and mentions them, so an ES
    // module and every ordinary function pay nothing.
    if module.is_some() {
        candidates.extend(super::common_js::mentioned(body, ctx));
    }
    // `arguments` the same way, and for the same reason one line up: it is bound
    // at entry and an ARROW inside reads the enclosing function's. Without it in
    // the candidate list the arrow finds nothing reachable — which is exactly
    // what happened, and the identical fault the import line above records.
    // A NAMED FUNCTION EXPRESSION binds its own name for its own body. Added as
    // a capture candidate for the same reason a parameter is: a nested function
    // may read it, and a name nothing lists as a candidate is declared as a
    // plain local that no closure can reach.
    //
    // Only when the body mentions it, so the ordinary function pays nothing —
    // the binding costs a call, and a function that never names itself has no
    // use for one.
    let self_name = self_name.filter(|name| capture::mentions(body, *name));
    if let Some(name) = self_name {
        candidates.push(name);
    }
    let named_arguments = ctx.names.intern("arguments");
    let binds_arguments = !captures_this && capture::mentions(body, named_arguments);
    if binds_arguments {
        candidates.push(named_arguments);
    }
    // `this` for the arrows inside, under a name a program cannot write. An
    // arrow takes `this` from where it was written rather than from how it is
    // called, so the only function that can answer is this one — and it answers
    // by handing it over as an ordinary captured name.
    let held_this = ctx.names.intern("__rts_this");
    let hands_this_to_an_arrow = !captures_this && capture::arrow_reads_this(body);
    if hands_this_to_an_arrow {
        candidates.push(held_this);
    }
    // TWICE, AND THE SECOND TIME IS NOT A CORRECTION OF THE FIRST.
    //
    // A name reaches an environment because some nested function mentions it.
    // `omit::omittable` decides which helper closures are not built at all, and
    // it needs this set to decide — a name a nested function captures is one it
    // refuses. So the first answer is what omission is decided FROM.
    //
    // Once that is known, the helpers it approved will not exist: each body is
    // spliced into this one at every call site and reads what it reads as
    // ordinary bindings of this activation. The names they alone mentioned no
    // longer need an environment, and the second answer is the set with those
    // declarations not walked into.
    //
    // It is a second walk rather than a filter because the environment is
    // DECIDED by this set — `escape::analyse` reads it and `Scope::for_function`
    // binds from it — and a name removed after the fact would be bound in a
    // layer nothing ever wrote.
    //
    // Measured 2026-08-30, release, min of 9, the analytic benchmark's own row:
    // `for (…) { const c = (x) => x + i; a = c(a) | 0; }` at 46.33 ns with the
    // object still built.
    let mut captured = capture::captured(body, &candidates, capture::nothing_omitted());
    // A name this module PUBLISHES is read by the publication, which is emitted
    // after the body and is not a statement the body's own walks can see. So it
    // is forced in here, exactly as `rest` and the imports are one screen up and
    // for the identical reason: a mention the analysis cannot find is still a
    // read.
    //
    // What it costs to leave out: the escape analysis flattens `const x = { a:
    // 1 }` into plain locals when nothing makes the object escape, and a
    // publication was nothing. So `export default { n: 1 }` dissolved the object,
    // left `@@default` bound to nowhere, and the publication read it as a GLOBAL
    // — the module died with `ReferenceError: @@default is not defined`, and an
    // importer saw `cannot resolve module`, naming the wrong problem entirely.
    //
    // Only a literal with no method reached it: one containing a function is not
    // flattenable, which is why every shim in a certain project's `compat/`
    // directory worked and the one written as `{ createAppAt }` did not.
    for publication in publications {
        if let super::module::PublicationSource::Local(name) = &publication.source {
            captured.insert(*name);
        }
    }
    // FORCED into the environment rather than left to the analysis. The arrow
    // reads it through `Scope::late_this`, not through an `Ident`, so nothing
    // the walk can see mentions the name — and a captured set derived from
    // mentions alone would leave the arrow with nothing reachable.
    if hands_this_to_an_arrow {
        captured.insert(held_this);
    }
    // A derived constructor holds `this` in its own environment, so it has one
    // whether or not anything else is captured — the name is added here rather
    // than being special-cased below, so every reader downstream sees an
    // ordinary captured name.
    //
    // `!captures_this` is what keeps that from applying to an ARROW, which also
    // arrives with a `late_this` and does NOT own it. Inserting there put the
    // name in the arrow's own environment at zero hops, shadowing the enclosing
    // function's slot with one nothing ever writes — so every `this` inside an
    // arrow read `undefined` while looking like it worked.
    if let Some(name) = late_this
        && !captures_this
    {
        captured.insert(name);
    }
    // A body that can reach a DIRECT `eval` puts everything it binds on the
    // heap, and this is the one place that can say so.
    //
    // `eval("x += 2")` assigns the caller's `x`, and source compiled while the
    // program runs reaches a binding only through the environment chain — a
    // name left in a register is not addressable from code emitted afterwards.
    // So the choice is between forcing the names out and answering a GLOBAL `x`
    // for a local one, silently, which is the wrong answer this engine refused
    // to ship before there was an alternative.
    //
    // The test is a MENTION of the name anywhere in or under this body, not a
    // direct call: `function outer() { let y = 1; return () => eval("y"); }`
    // needs `y` in `outer`'s environment although the `eval` is a function
    // deeper in, and nothing in the inner text mentions `y` for the capture
    // analysis to find. Over-including costs an environment slot in a body that
    // writes `eval` and never calls it; under-including costs a wrong answer.
    // A `with` needs exactly the same thing, for a reason worth stating rather
    // than inheriting: `emit/with_scope.rs` emits two paths per name, and the
    // lexical path of a WRITE would rebind a `Binding::Value` while the object
    // path stores a property — leaving the two paths disagreeing about what the
    // binding IS, which `Binding::value()` says cannot happen. In memory both
    // paths are stores and only the value needs merging.
    //
    // Extended here rather than written as a second mechanism because the two
    // questions have one answer: "this body's bindings must be addressable from
    // code that does not know their names at compile time".
    let eval_name = ctx.names.intern("eval");
    if capture::mentions(body, eval_name) || capture::has_with(body) {
        captured.extend(candidates.iter().copied());
        for statement in body {
            capture::declared_by_statement(statement, &mut captured);
        }
    }

    let mut func = MachineFunction::new(signature());
    let entry = func.entry;
    let incoming = func
        .block(entry)
        .expect("a function always has its entry block")
        .params
        .clone();

    // What this body proved about its own locals. Saved and restored around the
    // emission because it is a fact about ONE body, and a nested function
    // emitted in the middle of an outer one would otherwise be read against the
    // outer's answers.
    // Escape first, because what it replaces changes what there is to prove: a
    // replaced object has no binding of its own, and its properties have
    // bindings the source never named. It takes `captured` because which names
    // nested code can see is decided once — recomputing it here would be a
    // second chance to say "not captured" about a local a closure holds.
    let flattened = super::escape::analyse(body, parameters, &captured);

    // WHICH HELPER CLOSURES THIS BODY NEED NOT BUILD, asked here and not later
    // because `captured` is recomputed from the answer and the ENVIRONMENT is
    // decided from that.
    //
    // `flattened` is handed over rather than read off `Ctx`, which does not hold
    // it yet. It is deliberately the one computed from the FIRST answer above:
    // recomputing it against the smaller set could flatten a name the omission
    // was decided without, and a second escape pass would then need a third
    // capture pass to check itself. Keeping the conservative one costs a
    // flattening that was available, never an answer.
    let length = ctx.names.intern("length");
    let held_arguments = ctx.names.intern("arguments");
    let omission =
        super::omit::omittable(ctx, body, &captured, &flattened, length, held_arguments);
    // AND THE SECOND CAPTURE ANSWER, which is not a correction of the first.
    //
    // A name reached an environment because some nested function mentioned it.
    // The helpers just approved will not exist — each is spliced into this body
    // at every call site and reads what it reads as ordinary bindings of this
    // activation — so the names they ALONE mentioned no longer need one. A name
    // a second nested function also mentions stays, which is why the skip is per
    // declaration rather than per name.
    //
    // Measured 2026-08-30, release, min of 9, `bench/analytic.ts`'s own row:
    // `for (…) { const c = (x) => x + i; a = c(a) | 0; }` stood at 46.33 ns with
    // the environment still built, down from 241 before the omission work.
    // `uncaptured` and not `omitted`: a helper that WRITES a free name keeps
    // its environment even though its closure is gone. A substituted write
    // rebinds in the layer `emit_substituted` opened and that layer does not
    // outlive the statement, so while the name is captured the write is a store
    // that does. Measured: an accumulator answered 0 where node answers 6.
    if !omission.uncaptured.is_empty() {
        captured = capture::captured(body, &candidates, &omission.uncaptured);
    }
    let builds_environment = !captured.is_empty();
    let mut numeric = super::analyse(body, &flattened);
    // The properties are proved as `(object, key)` pairs, because `proven` has
    // no `Ctx` to mint a name from. Minting them here rather than there keeps
    // the interning out of a loop that runs to convergence.
    numeric.name_fields(|object, property| super::escape::field_name(ctx, object, property));

    // Both saved and restored around the emission because each is a fact about
    // ONE body, and a nested function emitted in the middle of an outer one
    // would otherwise be read against the outer's answers.
    // Beside the proof and AFTER it, because a claim about a name the body
    // already proved is not a second opinion — it is nothing to speculate
    // about, and `analyse` drops it so `Ctx::holds_number` stays the one
    // answer to whether a name holds a number.
    // After `numeric` is final, because every question it asks about an operand
    // is one that pass answers, and asking mid-fixpoint would read a shrinking set.
    let integers = super::int32::analyse(body, &numeric, &captured);
    let claims = super::types::analyse(body, parameter_claims, &numeric);
    // The count of what a PROOF made redundant is the difference, and it is
    // taken here because `Facts` no longer holds what it dropped.
    let seeded = parameter_claims.len();
    let redundant = seeded.saturating_sub(claims.len());
    ctx.census.record(&claims, seeded, redundant);
    ctx.census.record(&claims, parameter_claims.len(), 0);

    let outer_numeric = std::mem::replace(&mut ctx.numeric, numeric);
    let outer_integers = std::mem::replace(&mut ctx.integers, integers);
    let outer_claims = std::mem::replace(&mut ctx.claims, claims);
    let outer_flattened = std::mem::replace(&mut ctx.flattened, flattened);
    // WHICH HELPER CLOSURES THIS BODY NEED NOT BUILD. Asked here and not
    // earlier because it reads `ctx.flattened` and `ctx.inlinable`, and both are
    // in place exactly now: the flattening one line up, the candidates before
    // the body was entered at all.
    //
    // Answered ONCE, for the whole body, and consulted by a declaration rather
    // than by a call site. That is what makes it deterministic: a name is either
    // in this set — and its closure is never emitted — or it is not, and
    // everything happens as it did. There is no fallback path and nothing is
    // decided while emitting. See `omit.rs` for why the lazy shape was refused.
    let outer_omitted = std::mem::replace(&mut ctx.omitted, omission.omitted);
    let outer_local = std::mem::replace(&mut ctx.local_inlinable, omission.local);
    // The one that is an SSA VALUE and not a table, which is why it is taken
    // rather than replaced: the inner body defines its own at its own entry,
    // and reading the outer function's here would name a value defined in a
    // function this one is not in.
    let outer_throw = ctx.body.enter_nested();
    // The `finally` targets, for the same reason and with a sharper failure. A
    // `return` inside a `try` jumps to a block that runs the `finally` — and
    // that block belongs to the function the `try` is in. A nested function
    // emitted inside one would jump to a block of ANOTHER function, which the
    // builder refuses by panicking: "block belongs to this function". Measured
    // on `try { return (() => 2)(); } finally { … }`, which is ordinary code.
    //
    // Cleared rather than saved-and-shared: a `return` in the inner function
    // returns from IT, and owes nothing to a `finally` written outside.
    let outer_returns = std::mem::take(&mut ctx.finally_returns);
    let outer_jumps = std::mem::take(&mut ctx.finally_jumps);
    // AFTER every forced insertion above, never beside `capture::captured`.
    // Three names reach `captured` from outside the walk — a publication, an
    // arrow's `this`, a derived constructor's — and computing this earlier
    // dropped all three. `capture::own_level` records what that cost.
    let own_level = capture::own_level(body, &candidates, &captured);
    let result = emit_body_into(
        ctx,
        enclosing,
        parameters,
        body,
        captures_this,
        &mut func,
        entry,
        &incoming,
        captured,
        // Computed HERE, beside `captured` and from the same candidate list,
        // because this is where that list exists. The two answer different
        // questions — what the environment holds, and what is bound at this
        // function's own level — and `capture::declared_at_own_level` says why
        // they cannot be one set.
        own_level,
        builds_environment,
        rest,
        late_this,
        self_name,
        imports,
        module,
        publications,
    );
    ctx.numeric = outer_numeric;
    ctx.integers = outer_integers;
    ctx.flattened = outer_flattened;
    ctx.omitted = outer_omitted;
    ctx.local_inlinable = outer_local;
    ctx.claims = outer_claims;
    ctx.body.leave_nested(outer_throw);
    ctx.finally_returns = outer_returns;
    ctx.finally_jumps = outer_jumps;
    result?;
    Ok(func)
}

/// The part that holds the builder, split out so the numeric state above is
/// restored on the failing path as well as the succeeding one.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn emit_body_into(
    ctx: &mut Ctx,
    enclosing: &Scope,
    parameters: &[Name],
    body: &[Stmt],
    captures_this: bool,
    func: &mut MachineFunction,
    entry: rts_cranelift::ir::BlockId,
    incoming: &[ValueId],
    captured: std::collections::BTreeSet<Name>,
    // Of `captured`, the names bound at THIS function's own level. A separate
    // set rather than a filter applied here, because the candidate list it is
    // derived from lives in the caller — and because the two are genuinely
    // different questions. See `capture::declared_at_own_level`.
    own_level: std::collections::BTreeSet<Name>,
    builds_environment: bool,
    rest: Option<Name>,
    late_this: Option<Name>,
    self_name: Option<Name>,
    imports: &[crate::syntax::Import],
    module: Option<&str>,
    publications: &[super::module::Publication],
) -> EmitResult<()> {
    let types = ctx.types;
    let mut builder = FuncBuilder::new(func, types, entry);

    // The address of this thread's throw flag, before anything else in the
    // body: every check afterwards is a load from it rather than a call, and
    // an SSA value has to be defined where it dominates every use — which for
    // a check that can appear in any block means the entry block and nowhere
    // else. `expr::raise_if_thrown` reads it back off the context.
    //
    // Emitted for every body rather than only for bodies that check, because
    // whether one checks is not known until it has been emitted. That costs a
    // call per activation and saves one per operation; the trade was measured
    // and is recorded in `RuntimeOp::ThrownAddress`.
    //
    // Through `builder.call` and not `expr::call`: the latter emits a throw
    // check after what it calls, which is the thing this exists to make
    // possible and would be asking with the answer not yet in hand.
    //
    // NOT for a body that parks. `frame::resumable_form` rewrites a suspending
    // function around every suspension point, so a value defined at entry and
    // read after a `yield` is not the value it was — measured as 37 generator
    // files lost in one run, which is what put the check here. Such a body
    // keeps the call it always had; the address is what it cannot hold.
    if !super::suspends::body_suspends(body) {
        let asked = ctx.calls.declare(ctx.funcs, RuntimeOp::ThrownAddress);
        let flag = builder.call(ctx.funcs, asked, &[])?[0];
        ctx.body.flag = Some(flag);
        // And the zero every one of those checks compares the flag against.
        // One instruction here instead of one per check, for the reason the
        // paragraph above gives about the address and for one more: the entry
        // block dominates every block in the function, so a value put here
        // reaches every site that wants it. See `BodyState::zero`.
        //
        // Under the same condition, and not by habit — a constant is as much an
        // SSA value of the pre-rewrite function as the address is.
        let declared = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(0),
        });
        ctx.body.zero = Some(builder.use_const(declared));
    }

    let handed = incoming[ENVIRONMENT_PARAM];
    // A function that captures nothing adds no link to the chain, so what it
    // passes on is exactly what it was handed — which is why the hop counts
    // below only grow for a function that builds one.
    let environment = if builds_environment {
        // The names this function's inner functions capture, plus the link to
        // the environment outside. This is the count that mattered: a module's
        // scope carries every binding at its top level, and analytic.ts has
        // thirty-two of them against a cell's fifteen — so every read past the
        // fifteenth missed its cache, forever.
        let width = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(captured.len() as u64 + 1),
        });
        let width = builder.use_const(width);
        let fresh = expr::call(&mut builder, ctx, RuntimeOp::ObjectNew, &[width])?[0];
        let outer = binding::outer_link(ctx);
        super::property::emit_write(&mut builder, ctx, fresh, outer, handed)?;
        fresh
    } else {
        handed
    };

    // Everything the enclosing functions put in their environments, one link
    // further away for each environment between here and there. A function that
    // builds none adds no link, so what it passes on is what it was handed.
    let further = u32::from(builds_environment);
    let reachable: Vec<_> = enclosing
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + further))
        .collect();

    // A non-strict function called with no receiver sees the GLOBAL OBJECT
    // rather than `undefined`, and it sees it from the first statement — so the
    // substitution happens once here and every reader below takes this value
    // instead of the incoming slot. An arrow is excluded because it has no
    // `this` of its own to bind: it reads the enclosing function's, which was
    // already substituted there if that function was the sloppy one.
    let receiver = match ctx.sloppy && !captures_this {
        true => super::nonstrict::substitute_receiver(&mut builder, ctx, incoming[THIS_PARAM])?,
        false => incoming[THIS_PARAM],
    };

    let mut scope = Scope::for_function(Some(environment), captured, &own_level, &reachable);
    scope.set_this(receiver, captures_this);
    if let Some(name) = late_this {
        // DECLARED only where this function owns the name. A derived constructor
        // does: `this` does not exist in one until `super()` returns, so the
        // slot is seeded with what the caller passed — `undefined` — rather than
        // left absent, so a read before `super()` answers the same `undefined`
        // the language's ReferenceError would have been about.
        //
        // An ARROW does NOT. It borrows the enclosing function's, and declaring
        // here would seed a fresh local from the arrow's own receiver and shadow
        // the one it came for — which is what it did, and every `this` inside an
        // arrow read `undefined` while looking like it worked.
        if !captures_this {
            binding::declare(&mut builder, &mut scope, ctx, name, receiver)?;
        }
        scope.bind_this_late(name);
    }

    // A named function expression binds its OWN name, BEFORE the parameters.
    //
    // The order is the semantics and it is easy to get backwards: the language
    // puts the function's name in a scope that ENCLOSES the parameter scope, so
    // `function f(f) { return f; }` answers the parameter. Declared after them,
    // it overwrote the parameter instead — `shadow(7)` answered the function.
    //
    // The value is the function currently running rather than anything visible
    // here: `const g = function f() { … f() … }` must reach the function itself,
    // not `g`, which may be reassigned and may not exist.
    if let Some(name) = self_name {
        let running = expr::call(&mut builder, ctx, RuntimeOp::RunningFunction, &[])?[0];
        binding::declare(&mut builder, &mut scope, ctx, name, running)?;
    }

    // The first four come from the convention's slots, which is the whole of the
    // common case and costs nothing.
    for (position, name) in parameters.iter().take(ARGUMENT_SLOTS).enumerate() {
        binding::declare(&mut builder, &mut scope, ctx, *name, incoming[2 + position])?;
    }
    // A FIFTH parameter and beyond has no slot to arrive in, and this used to be
    // refused for that. It does not need one: a call passing more than four
    // arguments already builds a vector and goes through `CallWithArgs`, and
    // `rest_arguments` is what reads that vector back — so the extra parameters
    // are read out of the same array `arguments` is, by position.
    //
    // A caller that passed fewer builds no vector, `rest_arguments` falls back
    // to the four slots, and the read answers `undefined` — which is exactly
    // what a parameter nothing was passed for holds.
    if parameters.len() > ARGUMENT_SLOTS {
        let from = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(0),
        });
        let from = builder.use_const(from);
        let given: Vec<ValueId> = (0..ARGUMENT_SLOTS).map(|at| incoming[2 + at]).collect();
        let mut passed = vec![from];
        passed.extend(given);
        let all = expr::call(&mut builder, ctx, RuntimeOp::RestArguments, &passed)?[0];
        for (position, name) in parameters.iter().enumerate().skip(ARGUMENT_SLOTS) {
            let at = expr::number_constant(&mut builder, position as f64);
            let value = expr::call(&mut builder, ctx, RuntimeOp::GetIndexed, &[all, at])?[0];
            binding::declare(&mut builder, &mut scope, ctx, *name, value)?;
        }
    }


    // Before the first statement, and after the parameters: an import is a
    // declaration in this scope, so it is bound where a declaration would be.
    for import in imports {
        super::module::emit_import(&mut builder, &mut scope, ctx, import)?;
    }

    // `module`, `exports`, `require`, `__filename`, `__dirname` — beside the
    // imports because they are the same kind of thing: declarations of this
    // module's scope that the program did not write. Bound before the hoists
    // below, and skipping any name the body declares itself, so a program's own
    // `var require = …` is the one that survives.
    if let Some(specifier) = module {
        let declared = own_declarations(body);
        let (filename, dirname) = ctx.module_paths.clone().unwrap_or_default();
        super::common_js::emit_prologue(
            &mut builder,
            &mut scope,
            ctx,
            body,
            specifier,
            &filename,
            &dirname,
            &declared,
        )?;
    }

    // `...rest` is declared like any other parameter, from an array the runtime
    // builds — out of the vector when the caller allocated one, and out of the
    // four slots when it did not. Which of those happened is not decided here,
    // and could not be: it is a fact about the call, not about the callee.
    if let Some(name) = rest {
        let declared = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(parameters.len() as u64),
        });
        let declared = builder.use_const(declared);
        let given: Vec<ValueId> = (0..ARGUMENT_SLOTS).map(|at| incoming[2 + at]).collect();
        let mut passed = vec![declared];
        passed.extend(given);
        let gathered = expr::call(&mut builder, ctx, RuntimeOp::RestArguments, &passed)?[0];
        binding::declare(&mut builder, &mut scope, ctx, name, gathered)?;
    }

    // `arguments`, when the body mentions it and this function has any of its
    // own. An ARROW has none — it sees the enclosing function's, which is why
    // that one binds it as an ordinary local and the capture analysis carries it
    // in like any other name.
    //
    // What this answers is an array-LIKE object: `length`, the indices, and
    // `Symbol.iterator` — so `[...arguments]` and `Array.from(arguments)` walk
    // it — with `Object.prototype` above it. It used to be `RestArguments`, a
    // real Array, and that was visible on the first line that asked:
    // `Array.isArray(arguments)` answered `true`, and `arguments.map` existed.
    // `arguments.callee` is added below, for a non-strict function.
    //
    // What is still absent is the sloppy-mode ALIAS between a parameter and its
    // index — `function f(a) { a = 9; return arguments[0] }`. A `.ts` module is
    // strict, where the language says there is no alias, so nothing here needs
    // it; a cell that holds one is what a sloppy script would need.
    // `this` handed to the arrows inside, declared before anything can read it.
    // The value is the receiver this function was called with, which is exactly
    // what an arrow written here should see.
    if !captures_this && capture::arrow_reads_this(body) {
        let held_this = ctx.names.intern("__rts_this");
        binding::declare(&mut builder, &mut scope, ctx, held_this, receiver)?;
    }

    let named = ctx.names.intern("arguments");
    if !captures_this && capture::mentions(body, named) {
        {
            let passed: Vec<ValueId> = (0..ARGUMENT_SLOTS).map(|at| incoming[2 + at]).collect();
            let all = expr::call(&mut builder, ctx, RuntimeOp::ArgumentsObject, &passed)?[0];
            // `callee` only where the function is NON-STRICT. A strict one has
            // the name too, as an accessor that throws, and answering the
            // function there would make a program that tests for the throw see
            // a callable instead — the wrong direction to be wrong in.
            if ctx.sloppy {
                super::nonstrict::define_callee(&mut builder, ctx, all)?;
            }
            binding::declare(&mut builder, &mut scope, ctx, named, all)?;
        }
    }

    // Every `var` in the whole body, wherever it is written, exists as
    // `undefined` from here — before the function-declaration hoist below,
    // which is per-block rather than whole-body, and before the first
    // statement, which is what makes reading one before its own `var` line
    // answer `undefined` instead of refusing to compile.
    hoist_vars(&mut builder, &mut scope, ctx, body)?;
    hoist(&mut builder, &mut scope, ctx, body)?;
    // The body's own `let`, `const` and `class` names are in their dead zone
    // until their declarations are reached. Armed after both hoists on purpose:
    // a `var` and a hoisted `function` have no dead zone, and binding them first
    // is what keeps them out of this one.
    let lexical = binding::lexical_names(body);
    scope.expect_lexical(&lexical);

    let mut loops = Loops::default();
    let mut terminated = false;
    for statement in body {
        if super::emit_stmt(&mut builder, &mut scope, ctx, &mut loops, statement)? {
            terminated = true;
            break;
        }
    }
    // A module publishes what it exports here: after its last statement, so the
    // value published is the one it finished with, and before the return, so it
    // happens whether or not anything reads the module's answer.
    //
    // Guarded on `terminated` because a body that returned has no reachable
    // point to emit into. A module cannot contain a top-level `return` — that is
    // a syntax error in a module — so this guard is unreachable for a real
    // module and is here because "unreachable" is a claim the emitter should not
    // have to make about IR it is building.
    if !terminated
        && let Some(specifier) = module
    {
        super::module::emit_publications(&mut builder, &scope, ctx, specifier, publications)?;
        // And what the body left in `module.exports`, in the same place and for
        // the same reason: a module that assigns it on its last line publishes
        // that, not what it held halfway through.
        let declared = own_declarations(body);
        super::common_js::emit_epilogue(&mut builder, &scope, ctx, body, specifier, &declared)?;
    }
    if !terminated {
        // A derived constructor answers its `this`, not `undefined`. That is
        // what `construct` takes back — it allocated nothing, so what the callee
        // returns IS the instance — and a body falling off its end is the
        // ordinary way a constructor is written.
        let answer = match late_this {
            Some(name) => binding::read(&mut builder, &scope, ctx, name)?,
            None => expr::undefined(&mut builder, ctx),
        };
        builder.ret(&[answer]);
    }
    Ok(())
}

/// Every name a `var` introduces anywhere in a function body.
///
/// # Why this is a second traversal rather than `capture`'s shared one
///
/// `var` reaches the enclosing FUNCTION, through a block, an `if`, a loop, a
/// `try`, a `switch`, a label and `with` — but not through a nested function or
/// class, which owns its own `var` scope. `capture::walk_stmt`'s own doc says
/// why it cannot answer this alone: it deliberately drops the `BindingKind` off
/// a `Declare`'s bindings and is silent about a `for`-each target, because its
/// two existing callers want different things from both — and a third caller
/// wanting a third thing is not a reason to grow the shared shape, it is
/// `sloppy.rs`'s reason for its own copy, restated.
///
/// # Why declaration order does not matter here
///
/// Unlike [`Pattern::bound_names`], which orders initialisers for their side
/// effects, this just needs the *set* of spellings that must exist as
/// `undefined` before the first statement runs — so duplicates across two
/// `var x` in different blocks collapse for free at the call site, which
/// declares each name once.
fn collect_vars(body: &[Stmt], into: &mut Vec<Name>) {
    for statement in body {
        collect_vars_stmt(statement, into);
    }
}

/// One statement's contribution to [`collect_vars`].
fn collect_vars_stmt(statement: &Stmt, into: &mut Vec<Name>) {
    match &statement.kind {
        StmtKind::Declare {
            kind: BindingKind::Var,
            bindings,
        } => {
            for binding in bindings {
                binding.target.bound_names(into);
            }
        }
        StmtKind::Declare { .. } => {}
        StmtKind::Block(body) => collect_vars(body, into),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_vars_stmt(then_branch, into);
            if let Some(otherwise) = else_branch {
                collect_vars_stmt(otherwise, into);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            collect_vars_stmt(body, into)
        }
        StmtKind::For { init, body, .. } => {
            if let Some(ForInit::Declare {
                kind: BindingKind::Var,
                bindings,
            }) = init
            {
                for binding in bindings {
                    binding.target.bound_names(into);
                }
            }
            collect_vars_stmt(body, into);
        }
        StmtKind::ForEach { target, body, .. } => {
            if let ForEachTarget::Declare {
                kind: BindingKind::Var,
                target,
            } = target
            {
                target.bound_names(into);
            }
            collect_vars_stmt(body, into);
        }
        StmtKind::Labelled { body, .. } | StmtKind::With { body, .. } => {
            collect_vars_stmt(body, into)
        }
        StmtKind::Switch { clauses, .. } => {
            for clause in clauses {
                for inner in &clause.body {
                    collect_vars_stmt(inner, into);
                }
            }
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            for inner in body {
                collect_vars_stmt(inner, into);
            }
            if let Some(catch) = catch {
                for inner in &catch.body {
                    collect_vars_stmt(inner, into);
                }
            }
            if let Some(finally) = finally {
                for inner in finally {
                    collect_vars_stmt(inner, into);
                }
            }
        }
        // A function or class declaration owns its own `var` scope; `Function`
        // is hoisted separately, by `hoist`, and neither introduces a name into
        // THIS function's. Every other kind — `Expr`, `Return`, `Break`,
        // `Continue`, `Throw`, `Debugger`, `Empty`, `Using` (its binding always
        // behaves as `const`) — introduces no `var`.
        _ => {}
    }
}

/// Declares every `var` in the body as `undefined`, before anything runs.
///
/// # Why this and [`hoist`] are not one function
///
/// A `var` is hoisted ONCE, to the function's own top layer, over the WHOLE
/// body — reading `x` before `var x = 5` anywhere in the function answers
/// `undefined`, not a compile-time refusal. A function declaration is hoisted
/// PER BLOCK, so a nested one can be mutually recursive with a sibling — see
/// `hoist`'s own doc. Folding the two into one traversal would have to carry
/// both rules through one walk, which is the rule-stated-twice failure rule 3
/// warns about, just moved one level down.
///
/// Declared directly into the current (function) layer, which is why this must
/// run before [`Scope::enter`] is called for anything — a `var` inside a block
/// still has to land outside it.
/// Every name a body's own top level declares, however it declares it.
///
/// What it is for: the CommonJS prologue must not bind a name the program
/// binds. A module that writes `const require = createRequire(import.meta.url)`
/// gets its own, and a second binding under the same name in the same layer is
/// not a shadow but a bug — `Scope::declare` pushes an entry per call, and which
/// of the two a read finds is decided by the order they were pushed.
///
/// The three kinds are asked separately because the emitter binds them at three
/// different moments: `var` and functions are hoisted before the first
/// statement, and `let`/`const`/`class` are bound where they are written.
fn own_declarations(body: &[Stmt]) -> Vec<Name> {
    let mut names = Vec::new();
    collect_vars(body, &mut names);
    names.extend(binding::lexical_names(body));
    let mut hoisted = std::collections::BTreeSet::new();
    for statement in body {
        super::capture::declared_by_statement(statement, &mut hoisted);
    }
    names.extend(hoisted);
    names
}

pub fn hoist_vars(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
) -> EmitResult<()> {
    let mut names = Vec::new();
    collect_vars(body, &mut names);
    for name in names {
        // A name declared twice — `var x; var x;`, or one in each of two
        // sibling blocks — collapses here: `Scope::declare` pushes an entry
        // every time it is called, and a second `undefined` for the same
        // spelling would shadow the first in the SAME layer, which is a
        // different bug from the one this function fixes. Checking first is
        // cheap on a per-function list.
        // `declared_in_function` e nao `lookup`: a pergunta e se ESTA funcao ja
        // ligou o nome, e nao se o nome existe algures. Com `lookup`, todo o
        // nome que o escopo ENVOLVENTE tivesse fazia o `var` local nao ser
        // declarado — e num `<script>` de pagina, onde o envolvente e o
        // `window`, isso apanha `parent`, `top`, `self`, `name`, `length` e mais.
        //
        // Medido: `var parent = x; while (parent !== null) { parent = parent.pai; }`
        // dentro de uma funcao escrevia em `window.parent` (que e
        // `[Replaceable]` e IGNORA), lia de volta o `window`, e o laco nunca
        // terminava. Foi o que impediu o React 18 de montar, com o erro a
        // aparecer trinta ficheiros a jusante.
        if !scope.declared_in_function(name) {
            let value = expr::undefined(builder, ctx);
            binding::declare(builder, scope, ctx, name, value)?;
        }
    }
    Ok(())
}

/// Binds every function declared directly in a body, before the body runs.
///
/// # Why this is not just emitting the declaration where it was written
///
/// `function f() { return f(); }` reads `f` inside `f`, and mutual recursion
/// reads the second function before the first has been written. Both are
/// ordinary JavaScript, and both are the reason declarations are hoisted rather
/// than evaluated in order.
///
/// Hoisting is done per block rather than to the top of the function, which is
/// not quite what the specification says for `var`-like function scoping. It is
/// what makes the cases above work, and the difference shows only for a
/// declaration inside a nested block referenced before that block — named here
/// so the gap is a sentence rather than a surprise.
pub fn hoist(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
) -> EmitResult<()> {
    // Two passes, and the first one is the whole point: every name is bound
    // before any body is emitted, so a closure made in the second pass can
    // already see the ones declared after it.
    for statement in body {
        if let StmtKind::Function(function) = &statement.kind {
            let Some(name) = function.name else {
                continue;
            };
            let placeholder = expr::undefined(builder, ctx);
            binding::declare(builder, scope, ctx, name, placeholder)?;
        }
    }
    for statement in body {
        if let StmtKind::Function(function) = &statement.kind {
            let Some(name) = function.name else {
                continue;
            };
            let closure = emit_closure_declared(builder, scope, ctx, function)?;
            binding::write(builder, scope, ctx, name, closure)?;
        }
    }
    Ok(())
}

/// A function's parameter list as plain names, with the statements that unpack
/// what was not one.
///
/// # Why this is a desugaring and not an emission
///
/// `function f({a}, b = 1)` means exactly `function f(p0, p1) { let {a} = p0;
/// let b = p1 === undefined ? 1 : p1; … }`. Both halves already emit: a
/// destructuring declaration is `destructure.rs`, and a conditional is
/// `choice.rs`. Writing a second unpacker here that emitted patterns against
/// parameter slots would be the same rule stated twice, which rule 3 forbids —
/// and the two would eventually disagree about a hole, a nested default, or a
/// rest element.
///
/// # The two things the language decides here
///
/// A default fires on `undefined` and NOT on `null`, which is why the test is
/// `=== undefined` rather than a truthiness or a nullish check. And a default is
/// evaluated at the CALL rather than once at the declaration, which is what
/// keeping it as an expression in the prologue gives for free — `f()` twice with
/// `b = []` must not share one array.
/// The third value is the claims. A plain name with no default is
/// short-circuited below and pushes NO prologue statement, so its annotation
/// exists only on the `Parameter` and never reaches a `Binding` — which is
/// where every other claim in a body is read from. Handing it back here is the
/// only place it can be seen.
fn bind_parameters(
    ctx: &mut Ctx,
    function: &Function,
) -> (Vec<Name>, Vec<Stmt>, Vec<(Name, crate::syntax::Claim)>) {
    let mut names = Vec::with_capacity(function.parameters.len());
    let mut prologue = Vec::new();
    let mut claims = Vec::new();
    for (position, parameter) in function.parameters.iter().enumerate() {
        // A plain name with no default is already what the convention wants, so
        // it costs nothing: no synthetic name, no prologue statement, and the
        // common case stays exactly the code it was before this existed.
        if let (crate::syntax::Pattern::Name(name), None) = (&parameter.target, &parameter.default) {
            names.push(*name);
            if let Some(claim) = &parameter.claim {
                claims.push((*name, claim.clone()));
            }
            continue;
        }
        // Not a name a program can write: a parameter called `__rts_param_0`
        // would otherwise shadow the slot it is being unpacked from.
        let held = ctx.names.intern(&format!("__rts_param_{position}"));
        names.push(held);
        let read = Expr {
            kind: ExprKind::Ident(held),
            at: function.at,
        };
        let value = match &parameter.default {
            None => read,
            Some(default) => Expr {
                kind: ExprKind::Conditional {
                    condition: Box::new(Expr {
                        kind: ExprKind::Binary {
                            op: crate::syntax::BinaryOp::StrictEqual,
                            left: Box::new(read.clone()),
                            right: Box::new(Expr {
                                kind: ExprKind::Ident(ctx.names.intern("undefined")),
                                at: function.at,
                            }),
                        },
                        at: function.at,
                    }),
                    then_branch: Box::new(default.clone()),
                    else_branch: Box::new(read),
                },
                at: function.at,
            },
        };
        prologue.push(Stmt {
            kind: StmtKind::Declare {
                // `let` and not `var`: a parameter is scoped to the function
                // body, and hoisting one to the top would make a name declared
                // in an inner block collide with it.
                kind: crate::syntax::BindingKind::Let,
                bindings: vec![crate::syntax::Binding {
                    target: parameter.target.clone(),
                    value: Some(value),
                    claim: parameter.claim.clone(),
                }],
            },
            at: function.at,
        });
    }
    (names, prologue, claims)
}
