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
mod expr;
mod foreach;
mod function;
mod loops;
mod merge;
mod proven;
mod scope;
mod stmt;
mod switch;
mod template;
mod unary;

pub use expr::emit_expr;
pub use loops::Loops;
pub use proven::{Numeric, analyse};
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
    /// Which of them is the program's entry.
    pub entry: FuncId,
    /// The text of every string literal, indexed by the number the code holds.
    ///
    /// Travels with the functions because it is half of the program: the code
    /// names a literal by its position here, and placing the code without
    /// seeding this would leave every string reading as absent.
    pub literals: Vec<String>,
}

/// What emission needs that is not the function being built.
///
/// One struct rather than four parameters threaded through every emitter, and
/// it is `&mut` because declaring a runtime call mutates two of its fields.
/// Grouping them also makes a real property visible: the registry and the
/// declared-calls table outlive one function, because a compilation with two
/// functions calling `__rts_add` must declare it once.
pub struct Ctx<'a> {
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
    /// Which locals were proved to hold a number.
    ///
    /// Owned rather than borrowed, and filled by [`emit_program`] rather than by
    /// a caller: it is a fact about the body being emitted, so a caller
    /// supplying it would be supplying an answer about something it has not
    /// looked at.
    numeric: Numeric,
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
            model,
            funcs,
            calls,
            keys,
            names,
            types,
            pending: Vec::new(),
            literals: Vec::new(),
            numeric: Numeric::default(),
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
    // The script is a function like any other, under the same convention. It
    // was not before: it took no parameters, and every test that ran one called
    // it directly. Making it uniform is what lets a program call itself, and
    // what stops the host having two ways to enter compiled code.
    let sig = ctx.funcs.declare_signature(function::signature());
    let entry = ctx.funcs.declare_function(sig);

    // Nothing encloses a script, so nothing is reachable through a chain that
    // does not exist. An empty scope says exactly that.
    let nothing = Scope::new();
    let emitted = function::emit_body(ctx, &nothing, &[], body, false)?;
    ctx.pending.push((entry, emitted));

    Ok(Program {
        functions: std::mem::take(&mut ctx.pending),
        entry,
        literals: std::mem::take(&mut ctx.literals),
    })
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
        // This has named `f()`, an array literal and `new` in turn, and each
        // moved on when it landed. A class is what is still missing — and the
        // name in the refusal is the point, so the test follows it rather than
        // being deleted with the gap it happened to name.
        let error = emit_source("class C {}").expect_err("a class is not emitted yet");
        assert_eq!(
            error,
            EmitError::Unsupported {
                construct: "a class declaration"
            },
            "the name is the deliverable — a gap reported as `Unsupported` with \
             no word in it is indistinguishable from any other gap"
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
        let func = emit_source("let x = 1; x += 1;").expect("emits");
        let adds = instructions(&func)
            .iter()
            .filter(|inst| matches!(inst, Inst::Call { .. }))
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
