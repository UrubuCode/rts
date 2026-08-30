//! The tree, in the machine's representation.
//!
//! # Why this is called `emit` and not `lower`
//!
//! Because `lower` is taken, by the other half of the same pipeline:
//!
//! ```text
//! source ──parse──▶ tree ──emit──▶ IR ──lower──▶ machine code
//!                        (here)        (rts-cranelift::lower)
//! ```
//!
//! Both steps are lowerings in the ordinary sense, and calling both of them
//! "lowering" would make every sentence about the compiler ambiguous in exactly
//! the place it needed to be precise. The machine's module says it is "the only
//! module permitted to construct code-generator instructions", and that claim is
//! only checkable if the two have different names.
//!
//! # The one thing this module must not do
//!
//! Decide a machine question. The [crate rule](../../README.md) is rule 2, and
//! it has teeth here in a way it did not in `syntax/`: this module holds a
//! `FuncBuilder`, so the temptation is live rather than theoretical.
//!
//! Concretely: this never chooses a register, never chooses a calling
//! convention, never decides that an addition can use the integer instruction.
//! It states what the language means and hands the machine operands whose
//! representation it can defend. Where it cannot defend one, it says so — see
//! below.
//!
//! # What is not proven is `Tagged`, and that is the honest state
//!
//! A value nothing established is `Repr::Tagged` — the machine's word for
//! "nothing has been proved about this" — and its operators are calls.
//!
//! That is not a placeholder to be embarrassed about; it is rule 5 working:
//! *what cannot be proven becomes generic, visibly.* A first version that
//! guessed `f64` because most JavaScript numbers are doubles would produce code
//! that is wrong for `2 ** 53` and for `"a" + 1`, and would be wrong *silently*,
//! which is the failure this crate's rules exist to prevent. Generic is slow and
//! correct.
//!
//! Two things narrow it. A literal number is emitted proven, so an operator
//! over literals and over locals the analysis followed is an instruction. And
//! where nothing was proved, an operator a pair of doubles would settle emits a
//! **guard** and takes the instruction when the values turn out to be numbers —
//! which is what a type pass cannot reach, because a guard tests the value it
//! actually got rather than reasoning about where it came from.
//!
//! # What is not here yet
//!
//! Globals, `throw`, classes, and every value that lives on the heap without an
//! entry point to make it — a string, an array. They are
//! refused **by name** through [`EmitError::Unsupported`] rather than
//! mis-emitted, so a program that needs one fails visibly and the gap is a list
//! rather than a rumour. `PLAN.md` §E has the order and why.

mod binding;
mod body_state;
mod call;
mod capture;
mod choice;
mod common_js;
mod class;
mod delegate;
mod destructure;
mod escape;
mod eval;
mod page;
mod dynamic;
mod expr;
mod fold;
mod for_await;
mod foreach;
mod function;
mod globals;
mod inline;
mod loops;
mod merge;
mod module;
mod nonstrict;
mod object;
mod optional;
mod primordial;
mod property;
mod protect;
mod int32;
mod proven;
mod regex;
mod scope;
mod sloppy;
mod stmt;
mod suspends;
mod types;
mod switch;
mod template;
mod omit;
mod settled;
mod unary;
mod with_scope;
mod wrap;

pub use dynamic::{Wanted, dynamic_specifiers, specifiers};
pub use eval::emit_eval_program;
pub use page::emit_page_program;
pub use expr::emit_expr;
pub use loops::Loops;
pub use proven::Numeric;
use proven::analyse;
pub use scope::Scope;
pub use stmt::emit_stmt;

use rts_cranelift::ir::{BuildError, FuncId, FuncRegistry, Function};
use rts_cranelift::repr::Repr;
use rts_cranelift::shape::KeyRegistry;
use rts_cranelift::types::TypeRegistry;

use crate::names::{Name, Names};
use crate::runtime::RuntimeCalls;
use crate::syntax::Stmt;
use crate::values::ValueModel;

/// Why a program could not be emitted.
///
/// Two kinds, and keeping them apart is the point of the type. A
/// [`Self::Build`] is the machine refusing something this module constructed
/// wrongly — a bug here. An [`Self::Unsupported`] is this module admitting a
/// gap. Collapsing them into one would make the second look like the first, and
/// a gap that reads as a bug gets investigated instead of implemented.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EmitError {
    /// The machine refused what was constructed.
    ///
    /// Not a language error. Reaching this means this module built something
    /// the IR does not permit, which is a defect here rather than in the
    /// program being compiled.
    Build(BuildError),

    /// A construct this module does not emit yet, named.
    ///
    /// Named rather than counted: "unsupported expression" tells a reader
    /// nothing, and the whole reason gaps are refused instead of guessed is so
    /// the list of them can be read.
    Unsupported {
        /// What was written, in the language's own words.
        construct: &'static str,
    },

    /// A name was used and nothing introduced it.
    ///
    /// Distinct from a gap: the program is wrong, not this module. It is here
    /// rather than in a checker because emission needs the answer anyway, and a
    /// checker that has not been written cannot be relied on to have run.
    UnboundName(Name),
}

impl From<BuildError> for EmitError {
    fn from(error: BuildError) -> Self {
        EmitError::Build(error)
    }
}

/// Result of emitting.
pub type EmitResult<T> = Result<T, EmitError>;

/// What every value this module produces is, until a type pass says otherwise.
///
/// A single constant rather than the word `Repr::Tagged` written thirty times,
/// so that the day something proves a narrower representation, the sites that
/// were *assuming* are distinguishable from the sites that were *deciding*.
pub const UNPROVEN: Repr = Repr::Tagged;

/// Everything one compilation produced.
///
/// A list rather than a function, because a program is no longer one: a body
/// holding a function expression produces at least two, and every one of them
/// has to be placed for a call to reach it. The entry is named separately
/// because "the first" and "the last" are both wrong — a nested function is
/// finished before the one that defines it, so the script is last, and relying
/// on that would be relying on an emission order rather than on a fact.
pub struct Program {
    /// Each function, with the id it was declared under.
    ///
    /// The id is what a call site holds and what placement resolves, so the two
    /// travel together — a list of bodies alone would need the ids re-derived,
    /// and re-deriving them is how the wrong body gets placed under a name.
    pub functions: Vec<(FuncId, Function)>,
    /// Which of them are generator bodies, to be rewritten before placement.
    ///
    /// The rewrite belongs to whoever holds the type registry — the frame is an
    /// aggregate that does not exist until it runs — and that is the host. This
    /// list is how the host learns which functions to put through it without
    /// re-deriving the answer from a signature flag that async also sets.
    pub generators: Vec<FuncId>,
    /// What each function is CALLED and how many parameters it declares.
    ///
    /// EVERY function, including the ones with no name: `f.length` is a
    /// property the language promises for all of them, and `(function(){}).name`
    /// is the empty string rather than an absence. A trace still prints nothing
    /// for an empty name — that filter belongs to the printer, where it is a
    /// statement about traces instead of about functions.
    pub function_names: Vec<(FuncId, String, u32, bool, bool)>,
    /// Which of them is the program's entry.
    pub entry: FuncId,
    /// Every string literal, as UTF-16 code units, indexed by the number the
    /// code holds.
    ///
    /// Travels with the functions because it is half of the program: the code
    /// names a literal by its position here, and placing the code without
    /// seeding this would leave every string reading as absent.
    ///
    /// Units and not `String` for the reason [`crate::syntax::Text`] states:
    /// `"\uD83D"` is a legal one-unit string that no valid UTF-8 can carry, and
    /// a table of `String`s made it `U+FFFD` on the way to the runtime.
    pub literals: Vec<Vec<u16>>,
    /// The pieces of every tagged-template site, indexed the same way.
    ///
    /// A site is a flat list of literal positions, two per piece: the cooked
    /// text then the raw text, with [`NO_COOKED`] where the escapes were invalid.
    /// Flat rather than a structure because it crosses to a runtime that must
    /// not depend on this crate to name one — the same reason the literals cross
    /// as bare code units and not as a table.
    ///
    /// The strings object is built ONCE per site, on first evaluation, which is
    /// what the specification requires: a tag using it as a map key sees the
    /// same object on every pass.
    pub templates: Vec<Vec<u32>>,
}

/// The cooked position of a piece whose escapes are invalid.
///
/// A sentinel rather than an `Option`, because the list crosses as numbers. It
/// is `u32::MAX`, which is not a literal index any program reaches: the table it
/// indexes is built from the program's own text.
pub const NO_COOKED: u32 = u32::MAX;

/// A captured binding just written, and where the emitter was when it finished.
///
/// Read [`body_state::BodyState::last_captured_write`] for what this is for,
/// why it lives on the body rather than on `Ctx`, and why the window is
/// as narrow as it is.
#[derive(Clone, Copy)]
pub struct CapturedWrite {
    /// Which binding.
    pub name: Name,
    /// How many environments out it lives, as the write resolved it.
    ///
    /// Carried because the SAME spelling can name two bindings at two depths —
    /// a `let` in a loop body shadowing one outside it — and forwarding across
    /// that would answer the inner binding's value for the outer one's read.
    pub hops: u32,
    /// The value stored.
    pub value: rts_cranelift::ir::ValueId,
    /// The block the write's join landed in.
    pub block: rts_cranelift::ir::BlockId,
}

/// What emission needs that is not the function being built.
///
/// One struct rather than four parameters threaded through every emitter, and
/// it is `&mut` because declaring a runtime call mutates two of its fields.
/// Grouping them also makes a real property visible: the registry and the
/// declared-calls table outlive one function, because a compilation with two
/// functions calling `__rts_add` must declare it once.
pub struct Ctx<'a> {
    /// The captured binding written last, and what was written into it.
    ///
    /// # What this is for
    ///
    /// `rngState = (…) % m;` followed by `if (rngState < 0)` reads back, from
    /// the heap, a value the emitter is still holding. Measured 2026-08-21 on
    /// `bench/monte_carlo_pi.ts`: the whole distance between that file and the
    /// same algorithm written with locals is 69,6 ns an iteration, and every
    /// nanosecond of it is the twelve reads and writes of one captured variable
    /// at ~5,4 ns each. A read that does not have to happen is the cheapest one
    /// available.
    ///
    /// # Why it is safe, and the exact condition
    ///
    /// A memo over memory is a wrong ANSWER when its invalidation is
    /// incomplete, not a slow program — so this does not attempt to track what
    /// invalidates it. It carries the block the write's join landed in, and is
    /// spent only while the emitter is still standing at the top of that block
    /// with nothing appended: [`FuncBuilder::nothing_emitted_here`]. Anything
    /// at all having been emitted since, or control having moved to another
    /// block, and the question is simply not asked.
    ///
    /// That is a narrow window and it is the right one. It admits exactly the
    /// shape above — a write, then a read of the same name as the very next
    /// thing — and nothing where an operator, a call or a branch could have run
    /// user code in between.
    ///
    /// # Why only a CAPTURED binding
    ///
    /// Because the environment is an object this compiler created, holding
    /// plain data properties. Storing into one puts the value in the slot and
    /// does nothing else; there is no setter to transform it on the way in and
    /// no getter to answer something different on the way back. That is not
    /// true of an arbitrary object, which is why this is set from `binding.rs`
    /// and never from `property.rs`.
    /// Whether a CLEANUP body is being emitted right now.
    ///
    /// A cleanup block has a shape the machine checks: it ends by handing
    /// control back to whatever is unwinding, never by branching. The throw
    /// check `expr::call` emits after every operation branches, so emitting one
    /// inside a cleanup splits it into blocks that do not end that way — which
    /// the verifier refuses with `CleanupDoesNotEnd`, and did, on the first
    /// `finally` containing an assignment.
    ///
    /// So the check is skipped there, and the gap that leaves is named in
    /// `protect.rs`: a call inside a `finally` that throws does not propagate
    /// out of the cleanup.
    pub in_cleanup: bool,
    /// Whether the method body being emitted is a STATIC one.
    ///
    /// `super.m()` starts its search one link above the enclosing method's
    /// home object, and a static method's home is the CONSTRUCTOR where an
    /// instance method's is the prototype. Without the distinction, static
    /// `super` looked above the prototype and found nothing —
    /// `TypeError: undefined is not a function` on ordinary inheritance.
    ///
    /// A flag rather than a parameter for the reason [`Ctx::in_cleanup`] is
    /// one: what reads it is several frames down, inside another function's
    /// emission, and threading it would touch every step in between.
    pub in_static_method: bool,
    /// Whether what is being emitted is a class FIELD INITIALISER.
    ///
    /// One reader: `new.target`, which the language says is `undefined` there.
    /// The specification enters a field initialiser through `Call` rather than
    /// `Construct`, and this engine cannot inherit that answer from the shape
    /// of the emitted code — `emit/class.rs` writes the initialisers as
    /// statements at the head of the constructor, so the ACTIVATION is the
    /// constructor's and the runtime would rightly answer the class.
    ///
    /// A flag rather than a parameter for [`Ctx::in_static_method`]'s reason,
    /// and scoped by a marker in the tree rather than by the constructor's
    /// emission, because the constructor's own body must still see the real
    /// answer. See `emit/class.rs::FIELD_INITIALISER`.
    pub in_field_initializer: bool,
    /// Whether the code being emitted is NON-STRICT.
    ///
    /// `false` for everything a file compiles to: module code is strict by
    /// definition and a script this host compiles is wrapped in a function it
    /// treats the same way. The only producer of `true` is `rts-host`'s
    /// `live.rs` — the text of `Function(…)` and `eval(…)`, which is a script
    /// body and therefore sloppy unless it says otherwise.
    ///
    /// What reads it is `emit::nonstrict`, which states both consequences.
    /// Cleared for the duration of a body carrying `"use strict"`, and NOT
    /// restored for the functions nested inside it — strictness is inherited
    /// downward, so a sloppy function cannot appear inside a strict one.
    ///
    /// A flag rather than a parameter for [`Ctx::in_static_method`]'s reason.
    pub sloppy: bool,
    /// Se o programa é um `<script>` de PÁGINA.
    ///
    /// O que isso muda é uma coisa só e está em [`globals::resolves`]: um nome
    /// que só o Node tem — `process`, `Buffer`, `setImmediate`, `global` —
    /// deixa de resolver, porque num browser ele não existe. Ver a lista lá
    /// para o que isso custou.
    pub page: bool,
    /// The objects a `with` put on the scope chain, innermost LAST.
    ///
    /// Empty everywhere except inside a `with` body. What reads it is
    /// `emit/binding.rs`, which resolves a name against each of these before
    /// the lexical answer, and `emit/call.rs`, which turns off the two call
    /// fast paths that assume a bare name means what the scope says it means.
    ///
    /// A stack rather than one object because `with (a) with (b) x` asks `b`
    /// first and then `a`, and a single slot would lose the outer one.
    pub with_objects: Vec<rts_cranelift::ir::ValueId>,
    /// The helper bindings of the body being emitted whose CLOSURE is not built.
    ///
    /// Filled once per body by `omit::omittable`, before any of it is emitted,
    /// and read by the declaration rather than by a call site — which is what
    /// makes it deterministic. A name in here has every call to it substituted
    /// by construction, so the closure has no reachable use.
    omitted: std::collections::BTreeSet<Name>,
    /// The candidates this body built for ITSELF, for helpers `omitted` covers.
    ///
    /// `inlinable` is keyed by name over the whole program and must refuse a
    /// spelling two functions use. A helper `omit` approves is called only from
    /// the body that declares it, so the declaration in hand is the one every
    /// call reaches — and this holds it for that body alone.
    local_inlinable: std::collections::BTreeMap<Name, std::rc::Rc<inline::Inlinable>>,
    /// Where a `return` inside a protected span goes instead of returning.
    ///
    /// A `finally` runs on EVERY way out, and a `return` written inside the
    /// `try` is one of them. The machine's cleanup covers the paths it can see —
    /// an unwind — and cannot cover this one: a cleanup hands control back to
    /// whatever is unwinding, and a return is not unwinding.
    ///
    /// So the language routes it. A `return` inside a `try` that has a
    /// `finally` jumps to a block that runs the `finally` and returns there.
    /// Innermost last, and each level's block chains to the one outside it, so
    /// nested `finally` blocks run from the inside out — which is the order the
    /// language states.
    pub finally_returns: Vec<rts_cranelift::ir::BlockId>,
    /// The `finally` bodies a `break` or `continue` has to run on its way out,
    /// with how many loops were enclosing when each `try` was entered.
    ///
    /// A `return` can be routed to ONE block per `try` because its destination
    /// is always the same — out of the function. A `break` cannot: its
    /// destination depends on which loop it names, and that is known at the
    /// JUMP rather than at the `try`. So the body is carried and emitted at the
    /// jump instead, which is the same "two emissions of one tree" this module
    /// already pays for the normal and unwinding copies.
    ///
    /// The count is what decides which ones run: only a `finally` entered
    /// INSIDE the loop being left is on the way out. Leaving an inner loop does
    /// not run a `finally` wrapped around the outer one.
    pub finally_jumps: Vec<(Vec<crate::syntax::Stmt>, usize)>,
    /// What the language's singletons are numbered.
    pub model: &'a ValueModel,
    /// Every function this compilation can name.
    pub funcs: &'a mut FuncRegistry,
    /// Which runtime operations it has asked for so far.
    pub calls: &'a mut RuntimeCalls,
    /// Where property names are numbered.
    ///
    /// The machine's registry, and the host puts the SAME one in the runtime.
    /// A property name is resolved while compiling — that is the point of
    /// numbering it — so what crosses at every access is the number, and the
    /// two sides agreeing about which registry issued it is what makes the
    /// number mean anything.
    pub keys: &'a mut KeyRegistry,
    /// Which name each number stands for, for this compilation.
    pub names: &'a mut Names,
    /// What aggregates this compilation has laid out.
    ///
    /// Held here because a nested function is emitted in the middle of the one
    /// that defines it, and building it needs a `FuncBuilder`, which needs
    /// this. Threading it as a parameter would mean every emitter that can
    /// contain a function expression — which is nearly all of them — carrying
    /// an argument it does not itself use.
    pub types: &'a TypeRegistry,
    /// The functions emitted so far, each with the id it was declared under.
    ///
    /// A compilation is no longer one function. A body containing a function
    /// expression produces at least two, and the one that defines the other is
    /// finished *after* it — so they are accumulated here rather than returned,
    /// which would mean every expression emitter answering a list nearly all of
    /// them would leave empty.
    pending: Vec<(FuncId, Function)>,
    /// The name a binding lends to an anonymous definition on its right.
    ///
    /// `const f = function () {}` gives that function the name `f`, which the
    /// specification calls NamedEvaluation and which nothing here did: an
    /// anonymous function or class kept the empty name, so `f.name` read `""`
    /// and a program printing it saw nothing.
    ///
    /// Set only when the initialiser IS an anonymous definition — not for
    /// `const f = cond ? function () {} : g`, where the language names neither
    /// side — and TAKEN by the first definition that reads it, so a nested
    /// function inside the initialiser does not inherit the outer binding's
    /// name.
    inferred_name: Option<crate::names::Name>,
    /// The name and declared arity of each function, collected while emitting.
    function_names: Vec<(FuncId, String, u32, bool, bool)>,
    /// Which of the emitted functions are generator bodies.
    ///
    /// Collected while emitting rather than derived afterwards: `may_suspend` is
    /// set by an async body too, so a later pass over the signatures could not
    /// tell the two apart.
    generators: Vec<FuncId>,
    /// The text of every string literal this compilation contains, in the order
    /// it first appeared.
    ///
    /// # Why the text travels beside the code rather than inside it
    ///
    /// A string is a heap value, so a literal cannot be an immediate — two
    /// occurrences of `"a"` in a program are *the same string*, and an immediate
    /// would be a number that is not a string and compares wrongly with
    /// everything.
    ///
    /// The obvious alternative is to put the bytes in the compiled image and
    /// hand the runtime a pointer and a length. That needs the machine to lower
    /// `ConstDecl::Text` into a data section, which it does not yet do, and it
    /// would make the text part of the code — so a compilation destined for an
    /// object file would carry it and one destined for memory would too, by two
    /// different mechanisms.
    ///
    /// This is the same shape as the two agreements that already exist. The
    /// compiler numbers something, the host seeds the runtime with the same
    /// numbering, and what crosses at every use is the number. A literal is
    /// referred to by its index here exactly as a property is referred to by its
    /// key.
    literals: Vec<Vec<u16>>,
    /// The pieces of each tagged-template site, in the order the sites were met.
    templates: Vec<Vec<u32>>,
    /// Which locals were proved to hold a number.
    ///
    /// Owned rather than borrowed, and filled by [`emit_program`] rather than by
    /// a caller: it is a fact about the body being emitted, so a caller
    /// supplying it would be supplying an answer about something it has not
    /// looked at.
    numeric: Numeric,
    /// Which of those hold a 32-bit integer rather than an arbitrary double.
    ///
    /// A refinement of `numeric` and never independent of it: `int32::analyse`
    /// admits only names that pass already proved, because an unproven name is
    /// widened at every store and `to_int32` on a tagged value would be a
    /// narrowing without a guard. Owned and scoped exactly as `numeric` is.
    integers: crate::emit::int32::Int32,
    /// Which locals hold an object that never has to be allocated.
    ///
    /// Owned and scoped exactly as `numeric` is, and for the same reason: it is
    /// a fact about ONE body, so a nested function emitted in the middle of an
    /// outer one has to be read against its own answer.
    flattened: escape::Flattened,
    /// What this body's annotations CLAIM, for the sites that spend a claim as
    /// a guard.
    ///
    /// Scoped exactly as `numeric` and `flattened` are, and never consulted by
    /// `expr::stored`: a claim is evidence a program offered rather than
    /// evidence this compiler produced, so it may choose which check to emit and
    /// may never remove one. The `Speculation` type is what makes that a
    /// signature rather than a habit.
    claims: types::Facts,
    /// What the claims amounted to, when anything asked.
    ///
    /// On the context rather than in a static because cargo runs a binary's
    /// tests in threads, and shared statics in test binaries are a measured
    /// flake source in this workspace.
    census: types::Census,
    /// Whether anything asked for the census.
    ///
    /// Read ONCE, here, rather than at each counting site: an environment
    /// lookup on a path that runs per operation is a cost this workspace has
    /// already found and named in `entry::cache`.
    counting_claims: bool,
    /// Which classes the program declares, and what each declares as a FIELD.
    ///
    /// Whole-program and not per-body, unlike `claims`: a claim written in one
    /// function names a class declared in another, which is the ordinary case.
    class_fields: types::Classes,
    /// Which names the program creates by assigning to them.
    ///
    /// Answered once for the whole program before anything is emitted, because
    /// the read can come first: `function f() { return n; } n = 0;` emits the
    /// body before reaching the assignment. See [`sloppy`].
    globals: std::collections::BTreeSet<Name>,
    /// The specifier of the module being emitted, as the host resolved it.
    ///
    /// `None` for a script, which is not a module and has no `import.meta` to
    /// read — the language says so, and answering one anyway would be a name
    /// with nothing behind it.
    ///
    /// On the context rather than threaded as a parameter, for the reason
    /// [`Ctx::in_static_method`] is: `import.meta` and `import()` are ordinary
    /// expressions and may sit inside any nested function, several emissions
    /// below the one place that knows which file is being compiled.
    pub module_specifier: Option<String>,
    /// The file the module came from, and the directory holding it — what
    /// `__filename` and `__dirname` answer.
    ///
    /// Beside the specifier because it is the same fact from the host, and on
    /// the context for the same reason: both names may be read from inside any
    /// nested function. They come DOWN rather than being derived from the
    /// specifier here, because where a file is, is the host's question — this
    /// crate deriving it would be a second path resolver in the language layer,
    /// disagreeing with `graph.rs` the first time a path was spelled oddly.
    ///
    /// `None` for a script and for a caller with nothing to say, which binds the
    /// two names to the empty string rather than refusing: a program reading
    /// `__dirname` where nothing knows one gets an answer it can test.
    pub module_paths: Option<(String, String)>,
    /// Whether `Math` still refers to the primordial the runtime installed.
    ///
    /// Proved over the whole program before anything is emitted — see
    /// `primordial`. False is the safe answer and the default: a program this
    /// has not been computed for gets the call it has always got.
    math_primordial: bool,
    /// Which functions a call site may emit as their own body.
    ///
    /// Whole-program and computed before anything is emitted, like
    /// `math_primordial` and for the same reason: whether a name still refers
    /// to the function it was declared as is a fact about the entire tree, and
    /// nothing smaller than that can answer it without guessing. See `inline`.
    inlinable: std::collections::BTreeMap<Name, std::rc::Rc<inline::Inlinable>>,
    /// The callees whose bodies are being substituted right now, innermost last.
    ///
    /// A cycle among candidates is unbounded substitution at COMPILE time, and
    /// the pass's own recursion check does not see one: it refuses a body that
    /// mentions its OWN name, which stops `f` calling `f` and says nothing about
    /// `f` calling `g` calling `f`. The comment that claimed a mutual pair was
    /// caught "because each is free in the other" was wrong — being free refuses
    /// nothing — and it went unnoticed while every such body was refused for a
    /// different reason, having a `return` in it.
    ///
    /// Admitting guard clauses removed that reason and the pair became
    /// substitutable both ways. `two_functions_can_call_each_other` in
    /// `rts-host/tests/running.rs` overflowed the compiler's stack, which is
    /// what this stack exists to make unrepresentable rather than unlikely.
    substituting: Vec<Name>,
    /// What THIS body knows about throws: where its flag lives, and which
    /// re-raise block each protected region already has.
    ///
    /// Scoped exactly as `numeric` and `flattened` are, and for a harder reason
    /// than theirs: both facts in it are handles into ONE `FuncBuilder`, so a
    /// nested function emitted in the middle of an outer one would otherwise
    /// name something from a function it is not in.
    ///
    /// It was one field — the flag alone — and the re-raise memo was about to
    /// be added beside it with the same four save-and-restore sites to keep in
    /// step by hand. `body_throw` is what stops a fifth site from saving one and
    /// forgetting the other; see the module for what each half did the last time
    /// it leaked across a function boundary.
    pub(super) body: body_state::BodyState,
    /// Whether an `await` in the body being emitted PARKS this frame.
    ///
    /// True inside a plain `async function` — whose body is put through
    /// `frame::resumable_form` and driven by a promise reaction — and false
    /// everywhere else, which is what keeps the two forms of `await` from
    /// meeting. See `expr`'s `ExprKind::Await` for what the other form is and
    /// why an `async function*` and a module's top level still use it: a body
    /// whose suspensions are already stepped by something else (`next()`, the
    /// host) cannot have a second party resuming the same frame, and one
    /// `Suspend` cannot say which of the two parked it.
    ///
    /// Scoped exactly as `thrown_flag` is: saved and restored around every
    /// nested function, because an `await` written inside one parks THAT frame.
    async_parks: bool,
    /// The one `array[index]` pair a desugaring has PROVEN, while it is being
    /// emitted.
    ///
    /// One pair and not a set, because the only producer is a `for-of` and it
    /// proves exactly the pair it minted. See `Ctx::prove_element_read`.
    proven_element: Option<(Name, Name)>,
}

impl<'a> Ctx<'a> {
    /// A context for one compilation.
    pub fn new(
        model: &'a ValueModel,
        funcs: &'a mut FuncRegistry,
        calls: &'a mut RuntimeCalls,
        keys: &'a mut KeyRegistry,
        names: &'a mut Names,
        types: &'a TypeRegistry,
    ) -> Self {
        Ctx {
            in_cleanup: false,
            in_static_method: false,
            in_field_initializer: false,
            sloppy: false,
            page: false,
            with_objects: Vec::new(),
            omitted: std::collections::BTreeSet::new(),
            local_inlinable: std::collections::BTreeMap::new(),
            finally_returns: Vec::new(),
            finally_jumps: Vec::new(),
            model,
            funcs,
            calls,
            keys,
            names,
            types,
            pending: Vec::new(),
            generators: Vec::new(),
            inferred_name: None,
            function_names: Vec::new(),
            literals: Vec::new(),
            templates: Vec::new(),
            numeric: Numeric::default(),
            integers: crate::emit::int32::Int32::default(),
            flattened: escape::Flattened::default(),
            claims: types::Facts::default(),
            census: types::Census::default(),
            counting_claims: types::Census::wanted(),
            class_fields: types::Classes::default(),
            globals: std::collections::BTreeSet::new(),
            module_specifier: None,
            module_paths: None,
            math_primordial: false,
            inlinable: std::collections::BTreeMap::new(),
            substituting: Vec::new(),
            body: body_state::BodyState::default(),
            async_parks: false,
            proven_element: None,
        }
    }

    /// The function a plain call to `name` may be emitted as, if there is one.
    ///
    /// Answers an `Rc` rather than a reference because the body is emitted with
    /// this same context borrowed mutably — the shared handle is what lets the
    /// callee's expression outlive the lookup without copying the tree at every
    /// call site.
    pub(in crate::emit) fn inlinable(&self, name: Name) -> Option<std::rc::Rc<inline::Inlinable>> {
        self.inlinable.get(&name).cloned()
    }

    /// The same, consulting what THIS body built for itself first.
    ///
    /// A helper `omit` approves is declared here and called only from here, so
    /// its own declaration is the one every call reaches — even where the
    /// program-wide map had to refuse the spelling because another function
    /// spends it too. Consulted only through this door, so nothing that does not
    /// ask for it can be answered by a body of a different function.
    pub(in crate::emit) fn inlinable_here(&self, name: Name) -> Option<std::rc::Rc<inline::Inlinable>> {
        self.local_inlinable
            .get(&name)
            .cloned()
            .or_else(|| self.inlinable.get(&name).cloned())
    }

    /// Whether this callee is already being substituted further out.
    ///
    /// Exactly a cycle check: a name on the stack means substituting it again
    /// would re-enter a body that is still being emitted. A legitimate nesting —
    /// `outer` calling `inner` — never repeats a name and is unaffected, which
    /// is why this is a stack rather than a depth counter.
    pub(in crate::emit) fn substituting(&self, name: Name) -> bool {
        self.substituting.contains(&name)
    }

    /// Records that this callee's body is being substituted.
    pub(in crate::emit) fn enter_substitution(&mut self, name: Name) {
        self.substituting.push(name);
    }

    /// Records that it is finished.
    pub(in crate::emit) fn leave_substitution(&mut self) {
        self.substituting.pop();
    }

    /// Whether this helper binding's closure is not built at all.
    ///
    /// See `omit.rs`. True only when every call to the name is certain to be
    /// substituted, so nothing ever reads the value.
    pub(in crate::emit) fn omits(&self, name: Name) -> bool {
        self.omitted.contains(&name)
    }

    /// Whether the body being emitted replaced an object of this name with
    /// plain bindings.
    pub(super) fn flattens(&self, name: Name) -> bool {
        self.flattened.properties(name).is_some()
    }

    /// The number a property name has.
    ///
    /// Minted on first use and remembered by `Names`, so two accesses to `.x`
    /// in one program produce one key — which is what lets two objects built
    /// the same way reach the same layout.
    /// The machine key a property name is.
    ///
    /// The same key `key_of` numbers, as the machine's own type rather than as
    /// a number — which is what `cached_get` takes, because a site remembering
    /// a layout compares keys and not spellings.
    pub fn shape_key(&mut self, name: Name) -> rts_cranelift::shape::Key {
        self.names.key(name, self.keys)
    }

    /// The same key as a number, for a call that carries one.
    pub fn key_of(&mut self, name: Name) -> u32 {
        self.names.key(name, self.keys).index() as u32
    }

    /// The number a string literal has, minting one on first sight.
    ///
    /// Deduplicated by text, which is not a size optimisation: two occurrences
    /// of `"a"` in a program ARE the same string, so `"a" === "a"` has to be
    /// true for the same reason `o === o` is. Two indices would be two heap
    /// values that happen to spell the same thing, and strict equality would
    /// still answer true — because it compares text — but object identity of
    /// interned strings would not survive, and neither would the memory.
    ///
    /// Linear rather than hashed: a compilation has a handful of distinct
    /// literals and the same reasoning the scope layers record applies —
    /// hashing a string costs more than scanning a handful of them.
    pub fn literal(&mut self, text: &str) -> u32 {
        // Through the same table, because a literal the emitter SYNTHESISES —
        // a module specifier, a private name's key, a template's raw text — is
        // the same string as one the program wrote with those characters. Rust
        // text loses nothing on the way in: `encode_utf16` of valid UTF-8 is
        // exactly its code units.
        let units: Vec<u16> = text.encode_utf16().collect();
        self.literal_units(&units)
    }

    /// The same, for text that is already code units.
    ///
    /// This is what a string LITERAL takes, and the reason [`Self::literal`]
    /// delegates here rather than the other way round: `"\uD83D"` is a legal
    /// one-unit string, and there is no `&str` that spells it.
    pub fn literal_units(&mut self, units: &[u16]) -> u32 {
        if let Some(found) = self.literals.iter().position(|held| held == units) {
            return found as u32;
        }
        self.literals.push(units.to_vec());
        (self.literals.len() - 1) as u32
    }

    /// Records a tagged-template site and answers its number.
    ///
    /// NOT deduplicated, where [`Self::literal`] is: two identical templates
    /// written in two places are two sites, and the specification gives each its
    /// own strings object. Sharing one would make `` tag`a` `` in two functions
    /// hand the tag the same object, which a tag using it as a key would see.
    pub fn template(&mut self, pieces: Vec<u32>) -> u32 {
        self.templates.push(pieces);
        (self.templates.len() - 1) as u32
    }

    /// Whether a binding holds a proven number, and so keeps its
    /// representation instead of being widened at every store.
    pub fn holds_number(&self, name: Name) -> bool {
        self.numeric.holds_number(name)
    }

    /// Whether a binding's machine representation is the 32-bit integer one.
    ///
    /// Spent in exactly two places, both in `expr.rs` — `stored` narrows into it
    /// and `binding::read` widens back out — so that no third case had to be
    /// learned anywhere else. `int32.rs` has why that pair is not a no-op.
    pub fn holds_int32(&self, name: Name) -> bool {
        self.integers.holds_int32(name)
    }

    /// Records that a name this emitter MINTED holds a number.
    ///
    /// See `Numeric::prove_minted` for why a desugaring may assert this and a
    /// program may not.
    pub(super) fn prove_minted(&mut self, name: Name) {
        self.numeric.prove_minted(name);
    }

    /// Forgets one, so the assertion does not outlive the construct.
    pub(super) fn forget_minted(&mut self, name: Name) {
        self.numeric.forget_minted(name);
    }

    /// Records that `array[index]` is an element of a PROVEN array at a PROVEN
    /// canonical index, for as long as this is set.
    ///
    /// Set by `foreach.rs` around a desugared `for-of`, which established both
    /// by construction — see `RuntimeOp::ElementAt`. Read by `expr.rs` when it
    /// lowers an `Index`. Answers the previous pair so the caller restores it,
    /// which nested loops need for the reason `prove_minted` needs it.
    pub(super) fn prove_element_read(
        &mut self,
        pair: Option<(Name, Name)>,
    ) -> Option<(Name, Name)> {
        std::mem::replace(&mut self.proven_element, pair)
    }

    /// Whether this read is that pair.
    pub(super) fn is_proven_element(&self, array: Name, index: Name) -> bool {
        self.proven_element == Some((array, index))
    }

    /// Lends a binding's name to the anonymous definition about to be emitted.
    ///
    /// See [`Ctx::inferred_name`]. The caller establishes that the initialiser
    /// IS an anonymous definition; this only carries the name to it.
    pub(super) fn lend_name(&mut self, name: crate::names::Name) {
        self.inferred_name = Some(name);
    }

    /// Takes the lent name, leaving none behind.
    ///
    /// Taken rather than read, so the first definition to ask is the only one
    /// that gets it: a function nested inside the initialiser must not inherit
    /// the outer binding's name.
    pub(super) fn take_lent_name(&mut self) -> Option<crate::names::Name> {
        self.inferred_name.take()
    }

    /// What an annotation claims about a name, where nothing proved it.
    ///
    /// Answers a `Speculation` and never a `bool`, which is the whole of the
    /// separation: a boolean could be read by [`Self::holds_number`]'s callers,
    /// and those narrow with NO guard. This one can only be spent somewhere a
    /// guard is emitted, because nothing else accepts the type.
    #[allow(dead_code)]
    pub(super) fn claimed(&self, name: Name) -> Option<types::Speculation> {
        self.claims.claimed(name)
    }

    /// Whether this body claims anything at all.
    ///
    /// Asked before the per-operand lookups so an unannotated body pays one
    /// comparison instead of two probes at every operator — which is not a
    /// micro-optimisation but the difference between this pass costing 3% of
    /// compile time and costing nothing, measured.
    pub(super) fn claims_empty(&self) -> bool {
        self.claims.len() == 0
    }

    /// Whether a receiver claimed to be an instance of a class reads a member
    /// that class declares as a FIELD.
    ///
    /// The one question that separates `c.cb()` from `c.m()` without running
    /// anything: a field is an own property and a method is on the prototype,
    /// so they want opposite read forms. Answers `false` for everything it
    /// cannot establish, and `false` is the emission that changes nothing.
    pub(super) fn reads_own_field(&self, receiver: Name, member: Name) -> bool {
        match self.claimed(receiver).map(|held| held.kind()) {
            Some(types::Kind::Instance(class)) => {
                self.class_fields.declares_field(class, member)
            }
            _ => false,
        }
    }
}

/// Emits a body of statements as a function taking no parameters.
///
/// # What the signature says, and why it is this one
///
/// Every parameter and the return are `Tagged`, because that is what a
/// JavaScript function is at the boundary: a caller cannot know what it is
/// handing over and a callee cannot know what it will get back. A signature
/// claiming `f64` would be a claim about the program that nothing here proved.
///
/// # Falling off the end
///
/// A JavaScript function that reaches its closing brace returns `undefined`, so
/// this appends that return rather than leaving the block unterminated. The
/// machine's verifier would reject an unterminated block, which means the rule
/// cannot be forgotten — but it would be reported as a malformed function
/// rather than as the language fact it is, so it is done here on purpose.
pub fn emit_program(body: &[Stmt], ctx: &mut Ctx) -> EmitResult<Program> {
    emit_program_with(body, &[], ctx)
}

/// Emits a MODULE: its imports bind, then its statements run.
///
/// The split lives here rather than in the host because what an `import` means
/// for a scope and what an `export` costs are language decisions, and the host
/// is not where a language decision is taken.
pub fn emit_module(
    items: &[crate::syntax::ModuleItem],
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    emit_module_as(items, None, ctx)
}

/// The same, for a module the host resolved to a specifier.
///
/// `None` is a module compiled on its own — the shape every caller had before
/// there was a loader — and it may not export, because there is no specifier to
/// publish under. Refusing there rather than inventing a name is what keeps a
/// program's exports findable by exactly the specifier that imports it.
pub fn emit_module_as(
    items: &[crate::syntax::ModuleItem],
    specifier: Option<&str>,
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    let mut imports = Vec::new();
    let mut body = Vec::new();
    let mut publications = Vec::new();
    for item in items {
        match item {
            crate::syntax::ModuleItem::Import(import) => imports.push(import.clone()),
            crate::syntax::ModuleItem::Stmt(statement) => body.push(statement.clone()),
            crate::syntax::ModuleItem::Export(export) => {
                // Lowered either way, PUBLISHED only when there is a specifier
                // to publish under. This used to refuse outright, and refusing
                // was too strong: an export is a declaration plus a publication,
                // and a module nobody imports still has to run its declarations.
                //
                // Nothing is invented — there is no name, so nothing is
                // published, and a program that tries to import this module
                // fails at ITS import, which is where that failure belongs. That
                // is what the old comment was protecting and it still holds.
                let mut published = Vec::new();
                module::lower_export(export, &mut body, &mut published, ctx)?;
                if specifier.is_some() {
                    publications.extend(published);
                }
            }
        }
    }
    emit_program_with_exports(&body, &imports, specifier, &publications, ctx)
}

/// The same, for a body whose module bound names before it.
///
/// Separate from [`emit_program`] rather than a default argument because the
/// two callers are different things: a script has no imports and never will,
/// and a module always passes its own — including an empty list, which is a
/// module that imports nothing rather than a script.
pub fn emit_program_with(
    body: &[Stmt],
    imports: &[crate::syntax::Import],
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    emit_program_with_exports(body, imports, None, &[], ctx)
}

/// The same, for a module that publishes exports when its body finishes.
///
/// `specifier` is the module's own, resolved by the host. It is what the
/// publications are written under, and the reason a module compiled without one
/// may not export: there would be no name for an importer to find them by.
pub fn emit_program_with_exports(
    body: &[Stmt],
    imports: &[crate::syntax::Import],
    specifier: Option<&str>,
    publications: &[module::Publication],
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    // Nothing encloses a script, so nothing is reachable through a chain that
    // does not exist. An empty scope says exactly that — which is what
    // [`emit_eval_program`] is the one exception to.
    let nothing = Scope::new();
    emit_program_into(body, imports, specifier, publications, &nothing, ctx)
}

/// What both program doors do, with the enclosing scope either empty or the
/// caller's.
pub(super) fn emit_program_into(
    body: &[Stmt],
    imports: &[crate::syntax::Import],
    specifier: Option<&str>,
    publications: &[module::Publication],
    enclosing: &Scope,
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    // The script is a function like any other, under the same convention. It
    // was not before: it took no parameters, and every test that ran one called
    // it directly. Making it uniform is what lets a program call itself, and
    // what stops the host having two ways to enter compiled code.
    // Whole-program, once, before anything is emitted: a claim in one function
    // names a class declared in another, so this cannot be built per body.
    ctx.class_fields = types::declared(body);

    let sig = ctx.funcs.declare_signature(function::signature());
    let entry = ctx.funcs.declare_function(sig);

    // Nothing encloses a script, so nothing is reachable through a chain that
    // does not exist. An empty scope says exactly that.
    // Which names this program creates by assigning to them, before any of it
    // is emitted — a body that reads one may be emitted before the assignment
    // that creates it is reached.
    let global_this = ctx.names.intern("globalThis");
    ctx.globals = sloppy::created(body, global_this);
    // Which file is being compiled, for `import.meta` and `import()`. Recorded
    // once here because both are ordinary expressions that may sit inside any
    // nested function, which is emitted below this point.
    ctx.module_specifier = specifier.map(str::to_owned);
    // The proof that lets `Math.sqrt(x)` be one instruction. Computed once,
    // over the whole program, because that is the only scale at which it is a
    // fact rather than a guess — see `primordial` for why this engine can ask
    // and V8 cannot.
    let math = ctx.names.intern("Math");
    let eval_name = ctx.names.intern("eval");
    ctx.math_primordial = primordial::untouched(body, math, eval_name, global_this);
    // The same shape of proof, one level up: which small functions a call site
    // may emit as their own body rather than calling. See `inline`.
    let length_name = ctx.names.intern("length");
    let arguments_name = ctx.names.intern("arguments");
    ctx.inlinable = inline::candidates(body, eval_name, global_this, length_name, arguments_name);

    let mut emitted = function::emit_body(
        ctx,
        enclosing,
        &[],
        // A module body and a program body have no parameter list, so there
        // is no annotation on one to read.
        &[],
        body,
        false,
        None,
        None,
        // Neither a module body nor a program body is a function expression, so
        // neither has a name of its own to bind.
        None,
        imports,
        specifier,
        publications,
    )?;
    // A program or module entry has no `Function` node of its own to read
    // `is_async` off — unlike every other body, which gets `may_suspend` from
    // `function.rs`. `await` at this level is legal (a module's top level, or a
    // script a host chose to wrap as one), so whether THIS frame may suspend is
    // answered by looking at what it actually contains. Set on the emitted copy
    // and not only on the declared signature, for the same reason `function.rs`
    // does both: the verifier reads the emitted one.
    emitted.signature.may_suspend = suspends::body_suspends(body);
    ctx.pending.push((entry, emitted));
    Ok(finish(entry, ctx))
}

/// Takes everything one compilation accumulated, leaving the `Ctx` empty.
///
/// Split out because a compilation of SEVERAL modules must do this exactly once,
/// at the end. Doing it per module would restart the literal table, and a
/// literal is referred to by its position — so the second module's strings would
/// be numbered over the first's and every one of them would read as the wrong
/// text.
fn finish(entry: FuncId, ctx: &mut Ctx) -> Program {
    // The one place every emission funnels through, which is why the census
    // prints from here: a program with two hundred functions gets one table
    // rather than two hundred, and no caller has to remember to ask.
    if ctx.counting_claims {
        ctx.census.report();
    }
    Program {
        functions: std::mem::take(&mut ctx.pending),
        generators: std::mem::take(&mut ctx.generators),
        function_names: std::mem::take(&mut ctx.function_names),
        entry,
        literals: std::mem::take(&mut ctx.literals),
        templates: std::mem::take(&mut ctx.templates),
    }
}

/// One module of a multi-module compilation: its specifier and its items.
pub struct Unit<'a> {
    /// What an `import` of it names. The host resolved it.
    pub specifier: String,
    /// Its parsed body.
    pub items: &'a [crate::syntax::ModuleItem],
    /// What `__filename` answers, and what `__dirname` does.
    ///
    /// The host's, for the reason [`Ctx::module_paths`] gives. A caller with
    /// nothing to say passes two empty strings, which is what a script gets.
    pub paths: (String, String),
}

/// Every module of one program, emitted into one compilation.
///
/// # Why one compilation and not one per file
///
/// Because a reference belongs to the region that made it. A module compiled and
/// run on its own would hold its exports in its own region, and the importer —
/// in another — could not touch them. That is the same wall `node:vm` and
/// `worker_threads` hit, and the answer here is not to cross it: every module of
/// a program shares one compilation, one literal table, one key registry and one
/// region.
///
/// The order is the caller's. It knows the graph, because it is what read the
/// files.
pub struct Emitted {
    /// The functions, literals and templates of the whole program.
    pub program: Program,
    /// Each module's entry, in the order given — dependencies first.
    pub entries: Vec<FuncId>,
}

/// Emits several modules into one program.
pub fn emit_modules(units: &[Unit<'_>], ctx: &mut Ctx) -> EmitResult<Emitted> {
    let mut entries = Vec::with_capacity(units.len());
    for unit in units {
        let mut imports = Vec::new();
        let mut body = Vec::new();
        let mut publications = Vec::new();
        for item in unit.items {
            match item {
                crate::syntax::ModuleItem::Import(import) => imports.push(import.clone()),
                crate::syntax::ModuleItem::Stmt(statement) => body.push(statement.clone()),
                crate::syntax::ModuleItem::Export(export) => {
                    module::lower_export(export, &mut body, &mut publications, ctx)?;
                }
            }
        }
        ctx.module_paths = Some(unit.paths.clone());
        entries.push(emit_unit(
            &body,
            &imports,
            Some(&unit.specifier),
            &publications,
            ctx,
        )?);
    }
    let last = *entries.last().ok_or(EmitError::Unsupported {
        construct: "a program with no modules",
    })?;
    Ok(Emitted {
        program: finish(last, ctx),
        entries,
    })
}

/// One module's body as a function, without taking the compilation apart.
fn emit_unit(
    body: &[Stmt],
    imports: &[crate::syntax::Import],
    specifier: Option<&str>,
    publications: &[module::Publication],
    ctx: &mut Ctx,
) -> EmitResult<FuncId> {
    let sig = ctx.funcs.declare_signature(function::signature());
    let entry = ctx.funcs.declare_function(sig);
    let global_this = ctx.names.intern("globalThis");
    ctx.globals = sloppy::created(body, global_this);
    // Which file is being compiled, for `import.meta` and `import()`. Recorded
    // once here because both are ordinary expressions that may sit inside any
    // nested function, which is emitted below this point.
    ctx.module_specifier = specifier.map(str::to_owned);
    // The proof that lets `Math.sqrt(x)` be one instruction. Computed once,
    // over the whole program, because that is the only scale at which it is a
    // fact rather than a guess — see `primordial` for why this engine can ask
    // and V8 cannot.
    let math = ctx.names.intern("Math");
    let eval_name = ctx.names.intern("eval");
    ctx.math_primordial = primordial::untouched(body, math, eval_name, global_this);
    // The same shape of proof, one level up: which small functions a call site
    // may emit as their own body rather than calling. See `inline`.
    let length_name = ctx.names.intern("length");
    let arguments_name = ctx.names.intern("arguments");
    ctx.inlinable = inline::candidates(body, eval_name, global_this, length_name, arguments_name);
    let nothing = Scope::new();
    let mut emitted = function::emit_body(
        ctx,
        &nothing,
        &[],
        // A module body and a program body have no parameter list, so there
        // is no annotation on one to read.
        &[],
        body,
        false,
        None,
        None,
        // Neither a module body nor a program body is a function expression, so
        // neither has a name of its own to bind.
        None,
        imports,
        specifier,
        publications,
    )?;
    // See the sibling comment in `emit_program_with_exports`: this is the other
    // caller with no `Function` node of its own, and a graph of modules reaches
    // its entries only through here.
    emitted.signature.may_suspend = suspends::body_suspends(body);
    ctx.pending.push((entry, emitted));
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::parse::parse_script;
    use crate::syntax::{FunctionBody, ModuleItem, StmtKind};
    use crate::values::ValueModel;
    use rts_cranelift::ir::FuncRegistry;
    use rts_cranelift::ir::inst::{Inst, Terminator};
    use rts_cranelift::tags::TagRegistry;

    /// The program.s own entry, for a test that asserts about one function.
    ///
    /// Named rather than indexed: a nested function is finished before the one
    /// that defines it, so the entry is last today — and a test relying on that
    /// would be relying on an emission order rather than on a fact.
    fn entry_of(program: Program) -> Function {
        program
            .functions
            .into_iter()
            .find(|(id, _)| *id == program.entry)
            .expect("the entry was emitted")
            .1
    }

    /// Hands back what was emitted, having asked the machine whether it is well
    /// formed.
    ///
    /// Every helper here goes through this, so no test in this module can pass
    /// on a function the verifier would reject. Emission that produced a
    /// malformed function has not succeeded, and a test asserting something
    /// about one is asserting something about nothing.
    fn verified(
        emitted: EmitResult<Function>,
        types: &TypeRegistry,
        funcs: &FuncRegistry,
    ) -> EmitResult<Function> {
        let func = emitted?;
        let errors = rts_cranelift::verify::verify(&func, types, funcs);
        assert!(
            errors.is_empty(),
            "the machine rejected what was emitted: {errors:?}"
        );
        Ok(func)
    }

    /// Emits a script's top-level statements, for tests that want to write
    /// source rather than build a tree by hand.
    fn emit_source(source: &str) -> EmitResult<Function> {
        let mut names = Names::default();
        let mut tags = TagRegistry::new();
        let model = ValueModel::declare(&mut tags);
        let types = TypeRegistry::default();
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        let program = parse_script(source, &mut names).expect("the test's source must parse");
        // Imports and exports are not statements and this helper compiles a
        // body, so anything that is not one is dropped rather than silently
        // treated as one. No test here writes one.
        let body: Vec<_> = program
            .body
            .into_iter()
            .filter_map(|item| match item {
                ModuleItem::Stmt(statement) => Some(statement),
                _ => None,
            })
            .collect();
        let mut keys = rts_cranelift::shape::KeyRegistry::new();
        let func = {
            let mut ctx = Ctx::new(
                &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
            );
            emit_program(&body, &mut ctx).map(entry_of)
        };
        verified(func, &types, &funcs)
    }

    /// The same, keeping the runtime operations the body declared.
    ///
    /// A test about which operation was reached cannot read that from the
    /// function alone: a call names a `FuncId`, and what that id means lives in
    /// the registry the emission was given. Returning it is the only way to ask
    /// "did this reach `ToBoolean`" rather than "did this make a call".
    fn emit_source_with_calls(source: &str) -> EmitResult<(Function, RuntimeCalls)> {
        let mut names = Names::default();
        let mut tags = TagRegistry::new();
        let model = ValueModel::declare(&mut tags);
        let types = TypeRegistry::default();
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        let program = parse_script(source, &mut names).expect("the test's source must parse");
        let body: Vec<_> = program
            .body
            .into_iter()
            .filter_map(|item| match item {
                ModuleItem::Stmt(statement) => Some(statement),
                _ => None,
            })
            .collect();
        let mut keys = rts_cranelift::shape::KeyRegistry::new();
        let func = {
            let mut ctx = Ctx::new(
                &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
            );
            emit_program(&body, &mut ctx).map(entry_of)
        };
        verified(func, &types, &funcs).map(|func| (func, calls))
    }

    /// Emits a FUNCTION body, where `return` is legal.
    ///
    /// Wrapped in a declaration and unwrapped again rather than parsed as a
    /// script: `return` at the top level of a script is a syntax error, so a
    /// test about returning cannot be written as one.
    fn emit_body_of(source: &str) -> EmitResult<Function> {
        let mut names = Names::default();
        let mut tags = TagRegistry::new();
        let model = ValueModel::declare(&mut tags);
        let types = TypeRegistry::default();
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        let program = parse_script(&format!("function __test() {{ {source} }}"), &mut names)
            .expect("the test's source must parse");
        let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
            panic!("expected exactly one statement");
        };
        let StmtKind::Function(function) = &statement.kind else {
            panic!("expected a function declaration");
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!("a declaration always has a block body");
        };
        let mut keys = rts_cranelift::shape::KeyRegistry::new();
        let func = {
            let mut ctx = Ctx::new(
                &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
            );
            emit_program(body, &mut ctx).map(entry_of)
        };
        verified(func, &types, &funcs)
    }

    #[test]
    fn an_empty_body_still_returns_undefined() {
        let func = emit_source("").expect("an empty script emits");
        // Not "it has a terminator": the language fact is which value comes
        // back, and a function falling off its end returning nothing at all is
        // a different language.
        assert_eq!(func.signature.returns, vec![UNPROVEN]);
    }

    #[test]
    fn a_gap_is_named_rather_than_counted() {
        // This has named `f()`, an array literal, `new`, a class, a class
        // getter, a spread argument, a HOLE and then a spread BESIDE a hole in
        // turn, and each moved on when it landed. The last of those is now
        // emitted — the appending path says "skip this one" by appending the
        // absence marker `ArrayNew` already fills an unwritten position with —
        // so the test follows the refusal to the next construct rather than
        // being deleted with the gap it happened to name.
        //
        // `using` is that construct: it needs `Symbol.dispose`, which the
        // runtime does not have. Written inside a function because the checker
        // refuses one at a script's top level before emission is reached — a
        // different refusal, and pinning it here would be testing the checker.
        let error = emit_source("function f() { using r = {}; }")
            .expect_err("`using` is not emitted");
        assert_eq!(
            error,
            EmitError::Unsupported {
                construct: "`using`, which needs `Symbol.dispose`"
            },
            "the name is the deliverable — a gap reported as `Unsupported` with              no word in it is indistinguishable from any other gap"
        );
    }

    /// `[...a, , 1]` is elision beside a spread, and elision is not `undefined`.
    ///
    /// It was refused by name until the appending path learned to append the
    /// absence marker. Pinned as EMITTING rather than as the array it builds,
    /// because nothing here runs — what the marker means is
    /// `tests/cross-runtime/syntax/claude2-array-literal-holes-elision.ts`,
    /// which compares `1 in [1, , ...[]]` against Node and Bun.
    #[test]
    fn a_hole_beside_a_spread_is_emitted() {
        emit_source("let a = [...[1], , 2];").expect("a hole beside a spread emits");
    }

    #[test]
    fn using_a_name_nothing_introduced_emits_a_runtime_reference_error() {
        // `x` is bound nowhere the scope walk, `predefined` or `globals::
        // resolves` can see, which used to be `EmitError::UnboundName` —
        // refused before the program ran at all. The language's own answer is
        // a catchable `ReferenceError`, raised only when this read actually
        // executes, so this now emits: see `emit::globals::unbound_read`.
        emit_source("x + 1").expect("emits a call that raises at run time");
    }

    /// How many values are merged through block parameters.
    ///
    /// The ENTRY block is excluded, and that is the whole reason this is a
    /// function rather than a sum written at each site: a function.s entry
    /// parameters are its calling convention, not a merge, and counting them
    /// would make every one of these tests report six more than it means.
    fn merged_values(func: &Function) -> usize {
        func.blocks()
            .filter(|(id, _)| *id != func.entry)
            .map(|(_, block)| block.params.len())
            .sum()
    }

    /// Every instruction emitted, in no particular order.
    fn instructions(func: &Function) -> Vec<Inst> {
        func.blocks()
            .flat_map(|(_, block)| block.insts.iter())
            .filter_map(|id| func.inst(*id).map(|data| data.inst.clone()))
            .collect()
    }

    #[test]
    fn a_local_is_a_name_for_a_value_and_not_a_cell() {
        let func = emit_source("let x = 1; x + x;").expect("emits");
        // The claim, stated as the thing that would be false if it were wrong:
        // a binding that were a slot would allocate, store and load. Pinned
        // because the slot-per-local implementation is the obvious one, and
        // undoing it later is a rewrite rather than an optimisation — every
        // read would have become a memory operation for a pass to prove away.
        assert!(
            !instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Alloc { .. } | Inst::FieldStore { .. })),
            "declaring a local must not touch memory"
        );
    }

    #[test]
    fn an_operator_on_proven_numbers_is_an_instruction() {
        let func = emit_source("let x = 1; x + x;").expect("emits");
        // This test asserted the opposite until the type pass existed, and the
        // assertion it made was correct then: `+` is a call BECAUSE it might
        // concatenate. Proving both operands numeric is exactly the evidence
        // that it cannot, so the call goes.
        // ONE call, and it is not the operator's.
        //
        // This asserted zero until every body started asking for the address of
        // the throw flag at its entry — see `RuntimeOp::ThrownAddress`, which
        // made that one call the price of turning every later check into a
        // load. The claim being pinned is unchanged and is about the OPERATOR:
        // `+` on two proven doubles is an instruction, so a second call here
        // would be it going back to the runtime.
        let calls = instructions(&func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { .. }))
            .count();
        assert_eq!(
            calls, 1,
            "the only call in this body is the entry's throw-flag address; a \
             second one would be `+` reaching the runtime, and both operands \
             are proven doubles"
        );
        assert!(
            instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::FloatArith(..))),
            "the addition must be an instruction"
        );
    }

    #[test]
    fn an_operator_reaches_the_runtime_when_nothing_was_proved() {
        // `s` comes from a call, so nothing here knows what it is — and `1 + s`
        // may concatenate. The call is the correct emission, and this is what
        // rule 5 means by generic being visible rather than a fallback.
        // `s` comes from a call, so nothing here knows what it is. The call is
        // emitted now, so what this pins is that the ADDITION reached the
        // runtime — which is the claim the test was always making.
        let func = emit_source("function f() { return 1; } let s = f(); 1 + s;").expect("emits");
        assert!(
            instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Call { .. })),
            "nothing proved `s`, so `1 + s` may concatenate and must be a call"
        );
    }

    /// How many times the body asks the runtime to convert a value to a boolean.
    ///
    /// Counted by callee rather than by call count, because every body makes
    /// calls: the throw-flag address at the entry, the property reads, the
    /// operator that could not be proven. A total would move for reasons that
    /// have nothing to do with the claim.
    fn to_boolean_calls(func: &Function, calls: &RuntimeCalls) -> usize {
        let Some((_, wanted)) = calls
            .declared()
            .find(|(op, _)| *op == crate::runtime::RuntimeOp::ToBoolean)
        else {
            // Never declared, so it was never called: the operation is declared
            // lazily, at the first site that asks for it.
            return 0;
        };
        instructions(func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { callee, .. } if *callee == wanted))
            .count()
    }

    #[test]
    fn a_condition_does_not_pay_to_undo_a_widening_it_just_paid_for() {
        // `typeof o.x === "string"` cannot take the guarded form's instruction —
        // a string is never a double — so the comparison is the runtime's, which
        // answers a PROVEN boolean. The emitter widens it because a comparison
        // written in an expression is a value; `if` then wants the proof back.
        // Asking the runtime for it undoes an instruction emitted three lines
        // earlier.
        let (func, calls) =
            emit_source_with_calls("let o = {}; if (typeof o.x === \"string\") { o.y = 1; }")
                .expect("emits");
        assert_eq!(
            to_boolean_calls(&func, &calls),
            0,
            "the condition already holds the proof; converting it back is a call \
             that buys nothing"
        );
    }

    #[test]
    fn a_condition_over_a_value_that_is_not_a_boolean_still_converts() {
        // The twin. `if (o.x)` has no proof to recover — the truthiness of an
        // arbitrary value is the runtime's question, and this is the case that
        // stops the assertion above from being satisfied by never converting.
        let (func, calls) =
            emit_source_with_calls("let o = {}; if (o.x) { o.y = 1; }").expect("emits");
        assert!(
            to_boolean_calls(&func, &calls) >= 1,
            "an arbitrary value's truthiness is not something the emitter knows"
        );
    }

    #[test]
    fn a_switch_over_proven_numbers_tests_with_instructions() {
        let func = emit_source("let x = 3; let hit = 0; switch (x) { case 1: hit = 1; break; case 2: hit = 2; break; case 3: hit = 3; break; }").expect("emits");
        // One call, and it is the entry's throw-flag address — the same floor
        // `an_operator_on_proven_numbers_is_an_instruction` documents. A switch
        // used to add one call PER LABEL on top of it, each with the throw
        // check a call implies, while every operand at every label was a proven
        // double. Three labels here, so this asserted 4 before the change.
        let calls = instructions(&func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { .. }))
            .count();
        assert_eq!(
            calls, 1,
            "a switch over proven numbers must reach the runtime no more often \
             than an empty body does"
        );
        assert!(
            instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Compare(..))),
            "each label is a comparison instruction"
        );
    }

    #[test]
    fn a_switch_over_something_unproven_still_reaches_the_runtime() {
        // `s` comes from a call, so nothing knows what it is — and `===`
        // between two strings compares their TEXT, which reads the heap. The
        // call is the correct emission, and this is the twin that stops the
        // fold above from being applied where it would be wrong.
        let func = emit_source(
            "function f() { return 1; } let s = f(); switch (s) { case 1: break; }",
        )
        .expect("emits");
        let calls = instructions(&func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { .. }))
            .count();
        assert!(
            calls > 1,
            "nothing proved the subject, so the label test must ask the runtime"
        );
    }

    #[test]
    fn a_value_crossing_a_boundary_is_widened_back() {
        let func = emit_body_of("let x = 1; return x;").expect("emits");
        // `1 + 1` is not integer addition until something proves both sides are
        // numbers, and nothing has. Emitting `arith` would be fast and wrong for
        // `"a" + 1`, which is the failure rule 5 exists to prevent.
        //
        // The first version of this test asserted `Inst::Generic` instead, and
        // it was pinning the wrong thing: the machine refuses to lower a generic
        // operation at all, so what it pinned was IR that passes the verifier
        // and can never become machine code. Which symbol a generic addition
        // dials is a fact about JavaScript, so the language emits the call.
        assert!(
            instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Widen(_))),
            "a proof stops at the boundary: a caller cannot know what it gets \
             back, so the signature says tagged and the value is widened to match"
        );
    }

    #[test]
    fn a_compound_assignment_reads_its_target_once() {
        // `x += 1` is not `x = x + 1`: the target is evaluated once. With a
        // plain local the difference is invisible, and it is pinned here anyway
        // because the rewrite that loses it is the tempting one, and the day
        // the target is `a[i++]` the test that catches it will already exist.
        //
        // Calls that TAKE something are counted, not calls. Every operation now
        // carries a throw check behind it — `__rts_thrown`, and `__rts_take_thrown`
        // on the unwinding edge — and both take nothing, so counting every call
        // stopped meaning "how many times was the target operated on". The
        // distinction is real rather than a way to make the number come out: an
        // operation on values has values, and the check has none.
        // CONTAR CHAMADAS DEIXOU DE MEDIR ISTO, e a troca é registrada em vez de
        // silenciosa. O proxy era "uma chamada com argumentos = uma operação
        // sobre o alvo", e ele valia enquanto `+=` sempre chamava `__rts_add`.
        // Desde que `arithmetic()` aceita `+`, um alvo provado numérico vira uma
        // instrução de máquina e o número de chamadas cai para ZERO — o alvo
        // continua sendo lido uma vez, e o teste passaria a falhar por um acerto.
        //
        // O que se conta agora é a OPERAÇÃO, seja qual for a forma dela: uma
        // aritmética de máquina ou uma chamada que leva argumentos. Isso pina o
        // que a frase acima diz — o alvo é operado UMA vez — e continua valendo
        // se um dos dois caminhos deixar de existir.
        let func = emit_source("let x = 1; x += 1;").expect("emits");
        let operations = instructions(&func)
            .iter()
            .filter(|inst| {
                matches!(inst, Inst::FloatArith(..))
                    || matches!(inst, Inst::Call { args, .. } if !args.is_empty())
            })
            .count();
        assert_eq!(operations, 1);

        // E o caminho provado É o que roda neste caso: sem esta linha, um dia em
        // que `x += 1` voltasse a chamar o runtime o teste acima continuaria
        // passando, medindo a regressão como se fosse o mesmo comportamento.
        let calls = instructions(&func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { args, .. } if !args.is_empty()))
            .count();
        assert_eq!(calls, 0, "um alvo provado numerico nao chama o runtime");
    }

    #[test]
    fn an_unreachable_statement_after_a_return_is_not_emitted() {
        // Legal JavaScript, and the machine's verifier rejects a block with two
        // terminators — so this is not a nicety. It is why every statement
        // emitter answers whether control can still reach the next one.
        let func = emit_body_of("return 1; return 2;").expect("emits");
        let returns = func
            .blocks()
            .filter(|(_, block)| matches!(block.terminator, Some(Terminator::Return(_))))
            .count();
        assert_eq!(returns, 1);
    }

    #[test]
    fn a_name_the_two_arms_disagree_about_becomes_a_block_parameter() {
        // The mechanism, stated as what would be false without it: after
        // `if (c) { x = 1 } else { x = 2 }` there is no single definition of
        // `x`, and the IR is in SSA form, so there is nothing to write twice.
        // The join takes a parameter and each arm passes its own value.
        let func =
            emit_body_of("let x = 0; if (x) { x = 1; } else { x = 2; } return x;").expect("emits");
        assert!(
            func.blocks().any(|(_, block)| !block.params.is_empty()),
            "no block took a parameter, so two definitions were merged by \
             something that cannot be correct"
        );
    }

    #[test]
    fn a_name_neither_arm_touches_does_not_get_a_parameter() {
        // A correct program that moves a value through a register for no reason
        // is still worse than one that does not, and comparing the two paths is
        // what prevents it — so it is pinned rather than trusted.
        let func =
            emit_body_of("let x = 0; let y = 1; if (x) { x = 1; } return y;").expect("emits");
        assert_eq!(
            merged_values(&func),
            1,
            "only `x` differs between the two paths"
        );
    }

    #[test]
    fn both_arms_returning_emits_no_join_block() {
        // Ordinary JavaScript, and a join here would be a block nothing jumps
        // to — malformed, and rejected by the verifier every helper runs.
        let func = emit_body_of("if (1) { return 1; } else { return 2; }").expect("emits");
        let returns = func
            .blocks()
            .filter(|(_, block)| matches!(block.terminator, Some(Terminator::Return(_))))
            .count();
        assert_eq!(returns, 2);
    }

    #[test]
    fn the_second_arm_starts_from_the_environment_the_first_one_did() {
        // Without restoring, the `else` arm would read what `then` produced.
        // Written as source rather than as an assertion about ids because the
        // verifier is what catches it: a value used where it does not dominate.
        emit_body_of("let x = 0; let y = 0; if (x) { x = 1; } else { y = x; } return y;")
            .expect("emits");
    }

    #[test]
    fn a_nested_if_jumps_from_where_its_arm_ended_not_where_it_began() {
        // The bug this caught: jumping from the arm's FIRST block appends a
        // second terminator to a block whose branch is already there. It only
        // appears once something nests, which is why the machine's builder was
        // given a way to say where it currently is.
        emit_body_of("let x = 0; if (x) { if (x) { x = 1; } else { x = 2; } } return x;")
            .expect("emits");
    }

    #[test]
    fn a_condition_reaches_the_runtime_because_the_empty_string_is_falsy() {
        // Six of the seven falsy values a comparison settles. The seventh reads
        // a string's length from the heap, so truthiness is a call — which is
        // why control flow could not be emitted before calls existed.
        let func = emit_body_of("if (1) { return 1; } return 2;").expect("emits");
        assert!(
            instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Call { .. })),
            "the condition must reach the runtime, not a bare narrowing"
        );
    }

    #[test]
    fn a_loop_carries_what_it_writes_and_nothing_else() {
        // The whole content of the phase, as a count. `i` is written and `n` is
        // only read, so exactly one binding travels — and the alternative
        // implementation, a parameter for every live local, would give two.
        let func =
            emit_body_of("let i = 0; let n = 1; while (i) { i = n; } return i;").expect("emits");
        // Header and exit each carry `i`.
        assert_eq!(merged_values(&func), 2, "only `i` differs between passes");
    }

    #[test]
    fn a_body_local_gets_no_parameter_because_it_is_a_new_binding_each_pass() {
        // `x` is written every pass and is a different binding every pass, so
        // nothing outside the body can name it and nothing needs to carry it.
        let func =
            emit_body_of("let i = 0; while (i) { let x = i; x = 1; } return i;").expect("emits");
        assert_eq!(
            merged_values(&func),
            0,
            "the loop writes nothing that outlives a pass"
        );
    }

    #[test]
    fn break_and_continue_reach_the_blocks_the_loop_recorded() {
        // Both merge through the same mechanism as the back edge, so the thing
        // worth pinning is that they are emitted at all and that what they
        // produce is well formed — which is the verifier's answer, not mine.
        emit_body_of("let i = 0; while (i) { if (i) { break; } i = 1; } return i;").expect("emits");
        emit_body_of("let i = 0; while (i) { if (i) { continue; } i = 1; } return i;")
            .expect("emits");
    }

    #[test]
    fn a_for_header_owns_a_scope_that_does_not_survive_the_loop() {
        // `i` is not in scope after the loop. JavaScript itself does not
        // refuse that while parsing — a bare read of a name nothing declares
        // is a `ReferenceError` raised when the read runs, and this is one:
        // real Node agrees, since nothing here is a `let` shadowing anything.
        emit_body_of("for (let i = 0; i; i = 1) { }").expect("emits");
        emit_body_of("for (let i = 0; i; i = 1) { } return i;")
            .expect("emits a call that raises `i is not defined` at run time");
    }

    #[test]
    fn a_for_runs_its_update_on_the_continue_path() {
        // The reason `continue` targets a block of its own rather than the
        // header: jumping straight to the header would skip `i = 1`, and the
        // loop would spin. Well-formedness is what is checkable here; the
        // stepping block existing at all is what the count shows.
        let func = emit_body_of("for (let i = 0; i; i = 1) { continue; }").expect("emits");
        assert!(
            func.blocks().count() >= 5,
            "header, body, stepping, exit and the entry are all distinct"
        );
    }

    #[test]
    fn a_do_while_sends_continue_to_the_condition_not_the_top() {
        // Stated where the node is declared, and getting it wrong produces a
        // loop that runs its body twice per pass for programs using `continue`.
        emit_body_of("let i = 0; do { continue; } while (i);").expect("emits");
    }

    #[test]
    fn nested_loops_break_out_of_the_innermost_one() {
        emit_body_of("let i = 0; while (i) { while (i) { break; } i = 1; } return i;")
            .expect("emits");
    }
}
