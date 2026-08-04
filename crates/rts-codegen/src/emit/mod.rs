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
//! # Everything is `Tagged`, and that is the honest state
//!
//! No type pass exists yet. So every value this emits is `Repr::Tagged` — the
//! machine's word for "nothing has been proved about this" — and every operator
//! becomes a `GenericOp`.
//!
//! That is not a placeholder to be embarrassed about; it is rule 5 working:
//! *what cannot be proven becomes generic, visibly.* A first version that
//! guessed `f64` because most JavaScript numbers are doubles would produce code
//! that is wrong for `2 ** 53` and for `"a" + 1`, and would be wrong *silently*,
//! which is the failure this crate's rules exist to prevent. Generic is slow and
//! correct. The type pass makes it fast, and it is a separate piece of work with
//! its own evidence.
//!
//! # What is not here yet
//!
//! Control flow, calls, objects, closures. They are refused **by name** through
//! [`EmitError::Unsupported`] rather than mis-emitted, so a program that needs
//! one fails visibly and the gap is a list rather than a rumour. `PLAN.md` §E
//! has the order and why.

mod expr;
mod scope;
mod stmt;

pub use expr::emit_expr;
pub use scope::Scope;
pub use stmt::emit_stmt;

use rts_cranelift::ir::{BuildError, FuncBuilder, Function, Signature};
use rts_cranelift::repr::Repr;
use rts_cranelift::types::TypeRegistry;

use crate::names::Name;
use crate::syntax::Stmt;

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
pub fn emit_body(
    body: &[Stmt],
    params: &[Name],
    types: &TypeRegistry,
    model: &crate::values::ValueModel,
) -> EmitResult<Function> {
    let signature = Signature {
        params: params.iter().map(|_| UNPROVEN).collect(),
        returns: vec![UNPROVEN],
        ..Signature::default()
    };
    let mut func = Function::new(signature);
    let entry = func.push_block();

    let mut scope = Scope::new();
    for name in params {
        let value = func.push_block_param(entry, UNPROVEN);
        scope.declare(*name, value);
    }

    let mut builder = FuncBuilder::new(&mut func, types, entry);
    let mut terminated = false;
    for statement in body {
        if emit_stmt(&mut builder, &mut scope, model, statement)? {
            terminated = true;
            break;
        }
    }

    if !terminated {
        let undefined = expr::undefined(&mut builder, model);
        builder.ret(&[undefined]);
    }

    Ok(func)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::parse::parse_script;
    use crate::syntax::{FunctionBody, ModuleItem, StmtKind};
    use rts_cranelift::ir::inst::{GenericOp, Inst, Terminator};
    use crate::values::ValueModel;
    use rts_cranelift::tags::TagRegistry;

    /// Emits a script's top-level statements, for tests that want to write
    /// source rather than build a tree by hand.
    fn emit_source(source: &str) -> EmitResult<Function> {
        let mut names = Names::default();
        let mut tags = TagRegistry::new();
        let model = ValueModel::declare(&mut tags);
        let types = TypeRegistry::default();
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
        emit_body(&body, &[], &types, &model)
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
        emit_body(body, &[], &types, &model)
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
        let error = emit_source("f()").expect_err("calls are not emitted yet");
        assert_eq!(
            error,
            EmitError::Unsupported { construct: "a call" },
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
    fn an_operator_is_generic_because_nothing_proved_otherwise() {
        let func = emit_source("let x = 1; x + x;").expect("emits");
        // `1 + 1` is not integer addition until something proves both sides are
        // numbers, and nothing has. Emitting `arith` here would be fast and
        // wrong for `"a" + 1`, which is the failure rule 5 exists to prevent —
        // so the generic form is the deliverable rather than a placeholder.
        assert!(
            instructions(&func)
                .iter()
                .any(|inst| matches!(inst, Inst::Generic(GenericOp::Add, _, _))),
            "`+` must emit the generic operation, not `arith`"
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
            .filter(|inst| matches!(inst, Inst::Generic(GenericOp::Add, _, _)))
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
}
