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
mod call;
mod capture;
mod choice;
mod class;
mod delegate;
mod destructure;
mod escape;
mod expr;
mod fold;
mod foreach;
mod function;
mod globals;
mod loops;
mod merge;
mod module;
mod object;
mod optional;
mod property;
mod protect;
mod proven;
mod regex;
mod scope;
mod sloppy;
mod stmt;
mod suspends;
mod switch;
mod template;
mod unary;

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
    /// What each function is CALLED, for a stack trace to name it.
    ///
    /// Only the ones that have a name: an arrow assigned to nothing has none,
    /// and inventing one would put a label in a trace that the program cannot
    /// be searched for.
    pub function_names: Vec<(FuncId, String)>,
    /// Which of them is the program's entry.
    pub entry: FuncId,
    /// The text of every string literal, indexed by the number the code holds.
    ///
    /// Travels with the functions because it is half of the program: the code
    /// names a literal by its position here, and placing the code without
    /// seeding this would leave every string reading as absent.
    pub literals: Vec<String>,
    /// The pieces of every tagged-template site, indexed the same way.
    ///
    /// A site is a flat list of literal positions, two per piece: the cooked
    /// text then the raw text, with [`NO_COOKED`] where the escapes were invalid.
    /// Flat rather than a structure because it crosses to a runtime that must
    /// not depend on this crate to name one — the same reason the literals cross
    /// as text and not as a table.
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

/// What emission needs that is not the function being built.
///
/// One struct rather than four parameters threaded through every emitter, and
/// it is `&mut` because declaring a runtime call mutates two of its fields.
/// Grouping them also makes a real property visible: the registry and the
/// declared-calls table outlive one function, because a compilation with two
/// functions calling `__rts_add` must declare it once.
pub struct Ctx<'a> {
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
    /// The name of each function that has one, collected while emitting.
    function_names: Vec<(FuncId, String)>,
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
    literals: Vec<String>,
    /// The pieces of each tagged-template site, in the order the sites were met.
    templates: Vec<Vec<u32>>,
    /// Which locals were proved to hold a number.
    ///
    /// Owned rather than borrowed, and filled by [`emit_program`] rather than by
    /// a caller: it is a fact about the body being emitted, so a caller
    /// supplying it would be supplying an answer about something it has not
    /// looked at.
    numeric: Numeric,
    /// Which locals hold an object that never has to be allocated.
    ///
    /// Owned and scoped exactly as `numeric` is, and for the same reason: it is
    /// a fact about ONE body, so a nested function emitted in the middle of an
    /// outer one has to be read against its own answer.
    flattened: escape::Flattened,
    /// Which names the program creates by assigning to them.
    ///
    /// Answered once for the whole program before anything is emitted, because
    /// the read can come first: `function f() { return n; } n = 0;` emits the
    /// body before reaching the assignment. See [`sloppy`].
    globals: std::collections::BTreeSet<Name>,
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
            model,
            funcs,
            calls,
            keys,
            names,
            types,
            pending: Vec::new(),
            generators: Vec::new(),
            function_names: Vec::new(),
            literals: Vec::new(),
            templates: Vec::new(),
            numeric: Numeric::default(),
            flattened: escape::Flattened::default(),
            globals: std::collections::BTreeSet::new(),
        }
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
        if let Some(found) = self.literals.iter().position(|held| held == text) {
            return found as u32;
        }
        self.literals.push(text.to_owned());
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
    // The script is a function like any other, under the same convention. It
    // was not before: it took no parameters, and every test that ran one called
    // it directly. Making it uniform is what lets a program call itself, and
    // what stops the host having two ways to enter compiled code.
    let sig = ctx.funcs.declare_signature(function::signature());
    let entry = ctx.funcs.declare_function(sig);

    // Nothing encloses a script, so nothing is reachable through a chain that
    // does not exist. An empty scope says exactly that.
    // Which names this program creates by assigning to them, before any of it
    // is emitted — a body that reads one may be emitted before the assignment
    // that creates it is reached.
    ctx.globals = sloppy::created(body);

    let nothing = Scope::new();
    let mut emitted = function::emit_body(
        ctx,
        &nothing,
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
    ctx.globals = sloppy::created(body);
    let nothing = Scope::new();
    let mut emitted = function::emit_body(
        ctx,
        &nothing,
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
        // getter, a spread argument and a HOLE in turn, and each moved on when
        // it landed. The hole was the longest-standing of them: it waited for
        // the runtime to grow a marker for an absent position, because writing
        // it as `undefined` would have made `0 in [,1]` answer true.
        //
        // What is still missing is a spread BESIDE a hole — `[...a, , 1]` — for
        // which the argument-vector path has no way to say "skip this one".
        // The name in the refusal is the point, so the test follows it rather
        // than being deleted with the gap it happened to name.
        let error = emit_source("let a = [...[1], , 2];").expect_err("a spread beside a hole is not emitted");
        assert_eq!(
            error,
            EmitError::Unsupported {
                construct: "a hole beside a spread in an array literal"
            },
            "the name is the deliverable — a gap reported as `Unsupported` with              no word in it is indistinguishable from any other gap"
        );
    }

    #[test]
    fn using_a_name_nothing_introduced_is_the_programs_error_not_a_gap() {
        let error = emit_source("x + 1").expect_err("`x` is not bound");
        assert!(
            matches!(error, EmitError::UnboundName(_)),
            "must not be `Unsupported`: the construct IS emitted, and the \
             program is the thing that is wrong. Got {error:?}"
        );
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
        assert!(
            !instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Call { .. })),
            "nothing here needs the runtime: both operands are proven doubles"
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
        let func = emit_source("let x = 1; x += 1;").expect("emits");
        let adds = instructions(&func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { args, .. } if !args.is_empty()))
            .count();
        assert_eq!(adds, 1);
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
        // `i` is not in scope after the loop, so using it there is the
        // program's error rather than a gap.
        emit_body_of("for (let i = 0; i; i = 1) { }").expect("emits");
        let error =
            emit_body_of("for (let i = 0; i; i = 1) { } return i;").expect_err("`i` is gone");
        assert!(matches!(error, EmitError::UnboundName(_)), "got {error:?}");
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
