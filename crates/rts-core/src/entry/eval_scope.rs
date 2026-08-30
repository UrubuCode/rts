//! What makes a DIRECT `eval` direct: the caller's scope, carried.
//!
//! # Why this is not in [`super::eval`]
//!
//! That module is `Function(…)` — text compiled into a CALLABLE, whose free
//! names resolve against the globals like any other program's. This one is the
//! opposite question: a fragment whose free names resolve against a frame that
//! already exists, which is the only thing `eval` has that `Function` does not.
//! Keeping them apart is also what keeps either file readable — the two share
//! only the text-reading step, which is here because both doors of `eval` take
//! it and `Function` takes a different one.
//!
//! # What was refused before this, and why refusing was right then
//!
//! `eval` used to parse its argument and stop, with a plain `Error` for source
//! that parsed. The reason is still true of the alternative it refused:
//! compiling the fragment against the GLOBALS would answer `eval("x")` inside a
//! function whose own `x` shadows a global — silently, with the wrong value.
//! What changed is that the emitter now hands over the caller's environment, so
//! there is a third option that is neither the refusal nor the wrong answer.
//!
//! # What a direct `eval` still does not do, named rather than discovered
//!
//! **It does not DECLARE into the caller.** `eval("var z = 1")` binds `z` in the
//! fragment and the fragment ends; a later `z` in the caller is unbound. The
//! specification puts a `var` from sloppy `eval` in the enclosing variable
//! environment, which means writing a binding into an object the caller's
//! compiled code resolved at fixed hop counts — so it needs the caller to have
//! been compiled expecting it, which is a decision in `rts-codegen` rather than
//! something this side can add.
//!
//! **It does not carry `this`.** The fragment is entered as a script with no
//! receiver, so `eval("this")` answers `undefined` where an engine answers the
//! caller's receiver. `__rts_this` is in the chain only for a body that hands
//! `this` to an arrow, and [`environment_names`] skips the emitter's own names
//! rather than half-answering from one that is usually absent.
//!
//! Both are wrong answers for source of those shapes. They are `undefined`
//! rather than another frame's variable, which is the distinction that decides
//! whether a gap ships: the failure the refusal was protecting against was
//! reading the WRONG binding, and neither of these does that.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::value::Value;

/// How a host RUNS source text, in a scope the running program hands it.
///
/// The second argument is the environment object the caller's captured bindings
/// live in — `undefined` for an INDIRECT `eval`, which the specification says
/// runs in the global scope and which therefore has no caller frame to see.
///
/// The answer is the value the source completed with, or `None` for source that
/// did not parse, emit or place — which the caller turns into a `SyntaxError`,
/// the same failure Node reports for the same input.
///
/// # Why this is not [`FunctionCompiler`]
///
/// That one answers a CALLABLE, from a parameter list and a body, and its
/// compilation resolves free names against the globals. `eval` needs neither
/// half: it has no parameters, it answers a completion value rather than a
/// function, and the whole of what makes a direct `eval` direct is that its free
/// names resolve against the environment handed here. Giving `eval` the function
/// compiler is exactly what this module refused before this existed, and the
/// reason is in [`eval_direct`].
pub type EvalCompiler = fn(&str, u64) -> Option<u64>;

/// The host callback shape for evaluating source with an explicit receiver.
pub type EvalCompilerWithReceiver = fn(&str, u64, u64) -> Option<u64>;

/// Installs the host's evaluator for `eval`. See [`EvalCompiler`].
pub fn declare_eval_compiler(context: &mut Context, compiler: EvalCompiler) {
    context.eval_compiler = Some(compiler);
}

/// Installs the host callback used by APIs that supply an explicit receiver.
pub fn declare_eval_compiler_with_receiver(
    context: &mut Context,
    compiler: EvalCompilerWithReceiver,
) {
    context.eval_compiler_with_receiver = Some(compiler);
}

/// Evaluates source against an existing environment object.
///
/// This is the public counterpart of the direct-`eval` path for host-backed
/// facilities such as `node:vm`. The compiler is still owned by `rts-host`; this
/// function only forwards the source and environment through the installed seam,
/// so the runtime does not duplicate compilation or placement policy.
pub fn evaluate_in_scope(source: &str, environment: u64) -> Option<u64> {
    let compiler = with_current(|context| context.eval_compiler)?;
    compiler(source, environment)
}

/// Evaluates source against an environment and an explicit JavaScript receiver.
pub fn evaluate_in_scope_with_receiver(
    source: &str,
    environment: u64,
    receiver: u64,
) -> Option<u64> {
    let compiler = with_current(|context| context.eval_compiler_with_receiver)?;
    compiler(source, environment, receiver)
}

/// The global `eval` VALUE — which is to say, an INDIRECT `eval`.
///
/// `(0, eval)("x")`, `const e = eval; e("x")` and `globalThis.eval("x")` all
/// reach this, and all three run in the GLOBAL scope by specification: an
/// indirect `eval` never sees the caller's bindings. So the environment handed
/// to the host is `undefined`, and free names resolve exactly as they do in a
/// program compiled on its own.
///
/// A DIRECT `eval` — a call whose callee is literally the identifier `eval` —
/// does not come here at all. It is a syntactic form rather than a value, so
/// only the emitter can recognise it, and it reaches [`eval_direct`] with the
/// caller's environment beside the source.
///
/// A non-string argument is answered unchanged, which is what the specification
/// says and needs no compiler at all.
pub(in crate::entry) extern "C" fn eval_source(_e: u64, _this: u64, source: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let global = with_current(|context| undefined_of(context));
    evaluate(source, global)
}

/// `eval(source)` where the callee was written as the bare name `eval`.
///
/// # Why the emitter has to be the one to say so
///
/// Direct and indirect `eval` differ in nothing a value carries: the same
/// function object, called two ways, sees the caller's scope in one and the
/// global scope in the other. The distinction is **syntactic**, so the only
/// place it can be made is where the syntax still exists — which is why this
/// entry point takes an environment nothing else passes.
///
/// `environment` is the object the caller's captured bindings live in, and
/// `emit::function` forces every name in scope of a body that mentions `eval`
/// into one for exactly this reason: a name left in a register is not reachable
/// from source compiled afterwards. Answering the global `x` for a local one
/// would be the silent wrong answer this module refused to ship before.
///
/// # The intrinsic check
///
/// A program may replace `globalThis.eval`, and then `eval(s)` is an ordinary
/// call to whatever it now names. So this asks whether the global still IS the
/// one built here, and hands the call over when it is not — which is what makes
/// taking the syntactic shortcut honest rather than an assumption.
#[rtse::entry]
pub fn eval_direct(source: u64, environment: u64) -> u64 {
    match global_eval_replacement() {
        // Not ours any more: an ordinary call, with no scope of the caller's,
        // because whatever this now names is an ordinary function.
        Some(replacement) => {
            let absent = with_current(|context| undefined_of(context));
            super::functions::call(replacement, absent, source, absent, absent, absent)
        }
        None => evaluate(source, environment),
    }
}

/// Whatever `globalThis.eval` names, when it is no longer the one built here.
///
/// `None` while it is still the intrinsic — including before anything has read
/// it, since a name nothing asked for has not been replaced either.
fn global_eval_replacement() -> Option<u64> {
    let value = super::global::provided_value("eval")?;
    let cell = Value(value).as_slot()?;
    let intrinsic = eval_source as *const () as usize as u64;
    with_current(|context| match context.callable_at(cell) {
        Some((code, _)) if code == intrinsic => None,
        _ => Some(value),
    })
}

/// What both doors do: read the text, hand it to the host, answer what it
/// completed with.
///
/// Source that does not parse raises a `SyntaxError` — which is the whole
/// answer for a program that only asks whether text is well formed, and
/// `eval("obj.#x")` is a `SyntaxError` in every engine.
fn evaluate(source: u64, environment: u64) -> u64 {
    // Whether it is a string at all, and its text, in one borrow — and released
    // before anything raises, since building an error re-enters the context.
    let (is_text, text) = with_current(|context| {
        match Value(source).as_slot().and_then(|cell| context.text_at(cell)) {
            Some(held) => (true, held.to_rust()),
            None => (false, None),
        }
    });
    if !is_text {
        return source;
    }
    let Some(text) = text else {
        // A lone surrogate is a legal string and no parser here reads one, so
        // it is refused rather than lossily replaced — the same call
        // `compile_from_text` makes about its arguments.
        super::throw::syntax_error(
            "an eval argument that is not expressible as text cannot be parsed",
        );
        return with_current(|context| undefined_of(context));
    };
    let compiler = with_current(|context| context.eval_compiler);
    let Some(compiler) = compiler else {
        // The parser alone still answers the half of `eval` that asks whether
        // text is well formed, which is what a host with no evaluator can do.
        let parser = with_current(|context| context.source_parser);
        if let Some(parser) = parser
            && let Some(message) = parser(&text)
        {
            super::throw::syntax_error(&message);
        } else {
            super::throw::syntax_error(
                "this host installed no evaluator, so eval cannot run its argument",
            );
        }
        return with_current(|context| undefined_of(context));
    };
    match compiler(&text, environment) {
        Some(completed) => completed,
        None => {
            // A throw already in flight is the source's own — re-raising over
            // it would replace the program's error with ours, which is rule 8
            // read from the reporting side.
            if !super::throw::in_flight() {
                super::throw::syntax_error(&format!(
                    "the eval source did not compile: {}",
                    super::eval::shortened(&text)
                ));
            }
            with_current(|context| undefined_of(context))
        }
    }
}

/// Every name reachable through an environment chain, with how far out it is.
///
/// What a host needs to compile a direct `eval`: the fragment's free names have
/// to resolve to the caller's bindings, and a binding is a property of one of
/// these objects. The chain is walked HERE rather than in the host because the
/// link's spelling and the shape of an environment are this engine's, and the
/// host reaching into them would be a second statement of both.
///
/// Nearest wins, which is what shadowing means: a name found at one hop is not
/// offered again from further out. Names the emitter minted for itself are
/// skipped — `__rts_outer` is the link and `__rts_this` is the receiver an arrow
/// borrows, and neither is something source text can spell.
pub fn environment_names(environment: u64) -> Vec<(String, u32)> {
    let mut found: Vec<(String, u32)> = Vec::new();
    let mut walking = environment;
    // A bound rather than a `while let`: an environment chain is built by the
    // emitter and cannot cycle, but a bound costs nothing and turns a defect
    // into a short answer instead of a hang.
    for hops in 0..64u32 {
        let Some(cell) = Value(walking).as_slot() else {
            break;
        };
        // As chaves próprias E as herdadas. Um objeto de ambiente pode ser o
        // GLOBAL OBJECT de uma página — o `window` — e a superfície dele
        // (`document`, `location`, `setTimeout`, `self`) são acessores do
        // PROTÓTIPO da sua classe, não propriedades da instância. Ler só as
        // próprias devolvia uma lista vazia, e todo o nome livre de todo o
        // `<script>` respondia `ReferenceError: window is not defined`.
        //
        // É também o que a linguagem diz: num browser `toString` livre resolve,
        // porque o global object herda de `Object.prototype`. Um nome herdado
        // está em escopo tanto quanto um próprio.
        let texts = with_current(|context| {
            let mut texts = Vec::new();
            let mut seen = walking;
            for _ in 0..16u32 {
                let Some(cell) = Value(seen).as_slot() else { break };
                texts.extend(super::array::key_texts(context, seen, false));
                // `context.prototype_at` e não `chain::get_prototype`: aquele
                // entra no contexto por sua conta, e isto já está dentro de um.
                seen = match context.prototype_at(cell) {
                    Some(found) => found,
                    None => break,
                };
            }
            texts
        });
        for text in texts {
            let Some(text) = text.to_rust() else {
                continue;
            };
            if text.starts_with("__rts_") {
                continue;
            }
            if found.iter().any(|(seen, _)| *seen == text) {
                continue;
            }
            found.push((text, hops));
        }
        let outer = with_current(|context| {
            let key = context.interner.intern(
                &crate::text::Str::from_str(OUTER_LINK),
                &mut context.keys,
            );
            super::objects::read_property(context, cell, crate::object::Key::Name(key))
                .map(|found| found.bits())
                .unwrap_or_else(|| undefined_of(context))
        });
        walking = outer;
    }
    found
}

/// The property an environment reaches its enclosing one through.
///
/// The property an environment reaches its enclosing one through.
///
/// Written here as well as in `emit::binding` because the two sides are in
/// different crates and nothing links them — the same shape as every other
/// agreement `rts-host` asserts. A disagreement would make a chain stop at its
/// first link, so a direct `eval` would see one function's bindings and no more.
const OUTER_LINK: &str = "__rts_outer";
