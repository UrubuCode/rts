//! `Function(…)` and `new Function(…)` — a function whose body is text the
//! program produced while running — and the global `eval`, which is here
//! because it asks the same question of the host and answers less of it. See
//! [`eval_source`] for what it refuses and why refusing is the honest answer.
//!
//! # Why this is not [`super::modules::evaluate`]
//!
//! That seam compiles a whole program of its own and **refuses to hand back a
//! reference**, because a reference belongs to the region that made it. A
//! function IS a reference, so it can never answer this question — which is why
//! `new Function` was written against it once and reverted, as
//! [`super::function_proto`]'s module documentation records.
//!
//! What is here instead compiles INTO the context that is running: the same
//! region, the same property-key numbering, the same literal table extended
//! rather than replaced. Only the host can do that — it owns the compiler, the
//! placement and the region — so this is an injection like the evaluator and
//! the clock, and for the same reason: this crate is below the crate that
//! compiles, so the capability can only arrive from above.
//!
//! # What the four agreements cost here, and why they are read rather than
//! assumed
//!
//! A program placed by the host is seeded with a key numbering, a literal
//! table, a singleton numbering and a kind numbering, and `run.rs` says the
//! seeding is *"seeded rather than shared, because the compiler's is finished
//! before this one exists"*. A second compilation happens while the first one's
//! runtime is alive, so the direction reverses: the RUNTIME's numbering is the
//! finished one and the new compilation is what has to be made to agree with
//! it.
//!
//! [`Agreement`] is that numbering, read out of the running context, and
//! [`Addition`] is what the second compilation adds to it. The two are separate
//! calls because compiling happens between them and compiling must not hold a
//! borrow of the context.
//!
//! # Why the body is always SLOPPY here and this engine cannot say so
//!
//! `Function` bodies are non-strict per the specification, so `this` inside one
//! is the global object when the call passes no receiver. This engine has no
//! sloppy-mode receiver substitution anywhere — `functions::invoke` passes what
//! the call site wrote — so a body reading `this` sees `undefined` where Node
//! and Bun see `globalThis`. Stated rather than faked: making it true for
//! `Function` alone would need the substitution, and the substitution belongs
//! to the calling path rather than here.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// How a host turns source text into a CALLABLE inside the running context.
///
/// # Why the parameters cross separately from the body
///
/// Because they are separate in the language. `new Function("a", "b",
/// "return a + b")` declares two parameters, and pasting the parameter texts
/// into the body would make `new Function("a = 1", "return a")` mean something
/// the specification does not.
///
/// `None` for source that did not compile, which is what makes the caller able
/// to raise a `SyntaxError` naming the text rather than answering something
/// uncallable — the failure `new Function` had before this existed.
pub type FunctionCompiler = fn(&[String], &str) -> Option<u64>;

/// Installs the host's function compiler. See [`FunctionCompiler`].
pub fn declare_function_compiler(context: &mut Context, compiler: FunctionCompiler) {
    context.function_compiler = Some(compiler);
}

/// How a host answers whether source text is a well-formed program.
///
/// `Some(message)` when it is NOT, and the message is what the parser said.
/// `None` when it parses — which is all this asks, since parsing is the only
/// half of `eval` this engine can honestly perform. See [`eval_source`].
pub type SourceParser = fn(&str) -> Option<String>;

/// Installs the host's parser. See [`SourceParser`].
pub fn declare_source_parser(context: &mut Context, parser: SourceParser) {
    context.source_parser = Some(parser);
}

/// The global `eval`, made the same way every other provided global function is.
///
/// Not spelled `register_*`: that prefix means a `#[rtse::class]` registration
/// in this crate, and `declared.rs` reads `global.rs` for it so that a class
/// reachable as a global cannot be missing from the types. `eval` is a
/// function VALUE, like `parseInt` — there is no interface for `emit-types` to
/// describe — so it must not look like one.
pub(in crate::entry) fn eval_callable(context: &mut Context) -> u64 {
    super::native::callable(context, eval_source)
}

/// `eval(source)` — as much of it as this engine can answer without lying.
///
/// # What it does, and what it refuses
///
/// A *direct* `eval` sees the scope it was written in: `eval("x += 2")` inside a
/// function assigns that function's `x`. Nothing here can supply that. The
/// compiler this crate is handed by the host compiles into the running region
/// but against the GLOBAL scope, and it cannot be told about a caller's frame —
/// the emitter would have to mark a call to the name `eval` as direct and hand
/// over its bindings, which is a decision in `rts-codegen`, not here.
///
/// So this parses, and stops:
///
/// - source that does not parse raises a `SyntaxError`, which is the whole
///   answer for a program that only asks whether text is well formed —
///   `eval("obj.#x")` is a `SyntaxError` in every engine and two fixtures test
///   exactly that;
/// - source that DOES parse is refused by name, with a plain `Error` saying
///   why.
///
/// The refusal is the point. Compiling the source against the globals and
/// running it would answer for `eval("x")` inside a function whose own `x`
/// shadows a global — silently, with the wrong value. This crate's rule is that
/// a surface which cannot do what its name means does not ship one; an error at
/// the call is worse than nothing only if nothing were correct, and it is not.
///
/// A non-string argument is answered unchanged, which is what the specification
/// says and needs no compiler at all.
extern "C" fn eval_source(_e: u64, _this: u64, source: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
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
    let parser = with_current(|context| context.source_parser);
    let Some(parser) = parser else {
        super::throw::syntax_error(
            "this host installed no parser, so eval cannot look at its argument",
        );
        return with_current(|context| undefined_of(context));
    };
    if let Some(message) = parser(&text) {
        super::throw::syntax_error(&message);
        return with_current(|context| undefined_of(context));
    }
    super::throw::plain_error(&format!(
        "eval parsed this source but this engine cannot run it in the caller's scope, \
         and running it against the globals would answer a shadowed name wrongly: {}",
        shortened(&text)
    ));
    with_current(|context| undefined_of(context))
}

/// The numbering a second compilation has to be made to agree with, read out of
/// the context that is running.
///
/// Everything here is a COPY rather than a borrow, because the caller compiles
/// with it in hand and compiling must not hold the context.
pub struct Agreement {
    /// The text of every property key this context has issued, in key order.
    ///
    /// `None` where the key's text is not expressible as Rust text — a lone
    /// surrogate is a legal JavaScript property name and no `String` carries
    /// one. The POSITION still has to be spent by whoever reproduces this
    /// numbering, which is why the absence is a hole in the list rather than a
    /// shorter list.
    pub keys: Vec<Option<String>>,
    /// The literal table it holds, as code units, in index order.
    pub literals: Vec<Vec<u16>>,
    /// How many tagged-template sites it holds.
    pub templates: usize,
    /// Where its heap starts, and how far apart two cells are.
    pub region_base: u64,
    /// How far apart consecutive cells are.
    pub region_stride: u32,
    /// How many low bits of a reference name the region it belongs to.
    ///
    /// Zero for the single-region addressing `compile` produces. A caller must
    /// refuse anything else: the selector width is in every address computation
    /// of the code already running, and a second compilation that used a
    /// different one would build references the first reads as other cells.
    pub region_selector_bits: u32,
    /// Which singleton number means what, as the running program was told.
    pub singletons: crate::value::Singletons,
    /// Which tag number means which of the language's own kinds.
    pub kinds: crate::value::Kinds,
}

/// What a second compilation adds to the tables the context already holds.
///
/// # Why this is `adopt` and not the `declare_*` calls the host already has
///
/// `declare_literals`, `declare_templates`, `declare_frames` and
/// `declare_function_names` all REPLACE. That is right for a program about to
/// start and fatal for one already running: the code that is executing names a
/// literal by its position, so a table rebuilt from a second compilation's
/// literals would make every string of the first program the wrong one.
///
/// So this appends, and the appending is what makes the second compilation's
/// numbering legal: it was given the first table as its prefix, so its own
/// entries start exactly where that table ended.
pub struct Addition {
    /// The property keys the second compilation minted BEYOND the ones it was
    /// seeded with, in key order.
    pub keys: Vec<String>,
    /// Its whole literal table — the seeded prefix included, since only the
    /// tail past what this context already holds is added.
    pub literals: Vec<Vec<u16>>,
    /// Its whole tagged-template table, read the same way.
    pub templates: Vec<Vec<u32>>,
    /// What a parked frame looks like, per generator body it placed.
    pub frames: Vec<super::FrameShape>,
    /// What each function it placed is called, by the address it went to.
    pub function_names: Vec<(u64, String, u32)>,
}

/// Reads the numbering a second compilation must agree with.
pub fn agreement() -> Agreement {
    with_current(|context| {
        let mut keys = Vec::with_capacity(context.keys.len());
        for number in 0..context.keys.len() as u32 {
            let text = context
                .keys
                .key(number)
                .and_then(|key| context.interner.text(key))
                .and_then(Str::to_rust);
            keys.push(text);
        }
        // Read back out of the heap rather than mirrored beside it. The host
        // seeded this table from the compilation it placed, so it could keep
        // its own copy — but the copy would be a second statement of what the
        // literals ARE, and the tail of the table is written by whoever
        // compiled last rather than by the host alone.
        let literals = context
            .literals
            .iter()
            .map(|held| {
                Value(*held)
                    .as_slot()
                    .and_then(|cell| context.text_at(cell))
                    .map(|text| text.units().collect())
                    .unwrap_or_default()
            })
            .collect();
        Agreement {
            keys,
            literals,
            templates: context.templates.len(),
            region_base: context.region.base(),
            region_stride: context.region.stride(),
            region_selector_bits: context.region.selector_bits(),
            singletons: context.singletons,
            kinds: context.kinds,
        }
    })
}

/// Adds what a second compilation produced to the running context. See
/// [`Addition`].
pub fn adopt(addition: Addition) {
    with_current(|context| {
        // Interned in order, because interning is what mints the numbers — the
        // same rule `declare_keys` states, applied to the tail.
        for text in &addition.keys {
            let text = Str::from_str(text);
            context.interner.intern(&text, &mut context.keys);
        }
        for units in addition.literals.iter().skip(context.literals.len()) {
            let value = context.intern_value(Str::from_utf16(units)).bits();
            context.literals.push(value);
        }
        for pieces in addition.templates.iter().skip(context.templates.len()) {
            context.templates.push((pieces.clone(), None));
        }
        context.frames.extend(addition.frames);
        context.function_names.extend(addition.function_names);
    });
}

/// The `Function` constructor: the class registration, made able to build one.
///
/// # Why the code address is written here rather than declared by the class
///
/// `#[rtse::class("Function")]` gives a class with no `#[construct]` the
/// default body `|this| this`, which is why `Function("return 1")` answered
/// `undefined` and `new Function("return 1")` answered a plain object that was
/// not callable. The constructor this class needs cannot be written as a member
/// of it: it reaches a capability the HOST installs, and a class member is
/// declared where the class is.
///
/// So the registration is reached rather than repeated — one answer to what
/// `Function` is, with its prototype, its four methods and the
/// `Function.prototype` every callable inherits from — and the one thing this
/// adds is what running it does.
pub(in crate::entry) fn register_function_constructor(context: &mut Context) -> u64 {
    let made = super::function_proto::register_function(context);
    let Some(cell) = Value(made).as_slot() else {
        return made;
    };
    let environment = undefined_of(context);
    // Through the shared spelling of the convention rather than by casting the
    // function item, which is the one place a signature disagreement would be
    // a jump with a corrupt stack instead of a compile error.
    let code: super::modules::Provided = compile_from_text;
    context.mark_callable(cell, code as usize as u64, environment);
    made
}

/// `new Function(p1, …, pn, body)`.
///
/// Every argument is `ToString`ed, which may run user code — so it happens
/// outside any borrow, and a conversion that threw returns before the compiler
/// is reached, per this crate's rule 8.
extern "C" fn compile_from_text(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let given =
        with_current(|context| super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]));
    let mut texts: Vec<String> = Vec::with_capacity(given.len());
    for value in given {
        let primitive = super::primitive::to_primitive(value, crate::coerce::Hint::String);
        if super::throw::in_flight() {
            return with_current(|context| undefined_of(context));
        }
        let text = with_current(|context| {
            super::text::to_text(context, Value(primitive)).and_then(|text| text.to_rust())
        });
        // A string this engine cannot express as Rust text is one no compiler
        // here can parse either, so it is refused rather than lossily replaced
        // — a `U+FFFD` in a body would compile to a program the caller did not
        // write.
        let Some(text) = text else {
            super::throw::syntax_error(
                "a Function argument that is not expressible as text cannot be compiled",
            );
            return with_current(|context| undefined_of(context));
        };
        texts.push(text);
    }
    // The LAST argument is the body and every earlier one names parameters,
    // each of which may itself be a comma-separated list — `new Function("a,b",
    // "return a + b")` is the same function as `new Function("a", "b", …)`.
    let body = texts.pop().unwrap_or_default();
    let parameters: Vec<String> = texts
        .iter()
        .flat_map(|listed| listed.split(','))
        .map(|parameter| parameter.trim().to_owned())
        .filter(|parameter| !parameter.is_empty())
        .collect();

    let compiler = with_current(|context| context.function_compiler);
    let Some(compiler) = compiler else {
        // Not a `TypeError`: the name exists and the call is well formed, and
        // what is missing is a capability of the embedder. Saying so beats
        // answering something uncallable, which is what this did before.
        super::throw::syntax_error(
            "this host installed no compiler, so a function cannot be built from text",
        );
        return with_current(|context| undefined_of(context));
    };
    match compiler(&parameters, &body) {
        Some(made) => made,
        None => {
            super::throw::syntax_error(&format!(
                "the Function body did not compile: {}",
                shortened(&body)
            ));
            with_current(|context| undefined_of(context))
        }
    }
}

/// The body as a diagnostic prints it.
///
/// A whole body can be a page, and a `SyntaxError` whose message is a page is
/// one nobody reads. The first line, cut, is what names the failure.
fn shortened(body: &str) -> String {
    let first = body.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let trimmed = first.trim();
    match trimmed.char_indices().nth(60) {
        Some((at, _)) => format!("{}…", &trimmed[..at]),
        None => trimmed.to_owned(),
    }
}
