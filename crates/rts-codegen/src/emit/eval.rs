//! `eval` source, as a program that resolves its free names in a frame that
//! already exists.
//!
//! # Why this is its own door and not a flag on [`super::emit_program`]
//!
//! Because the difference is a SCOPE, and a scope is what every other program
//! door deliberately has none of: `emit_program_with_exports` starts from
//! `Scope::new()` and says so — *"nothing encloses a script"*. An `eval`
//! fragment is the one program for which that sentence is false, and putting
//! the exception beside the rule is what makes the rule readable.

use super::{Ctx, EmitResult, Program, Scope, emit_program_into};
use crate::syntax::Stmt;


/// Emits an `eval` fragment: a script that RESOLVES FREE NAMES against a scope
/// the caller is going to hand it, and answers what it completed with.
///
/// # What `enclosing` is
///
/// Every name reachable through the environment chain the caller will pass as
/// the script's environment parameter, with how many `__rts_outer` links out it
/// lives. The host reads it off the running program —
/// `rts_core::entry::environment_names` walks the chain — because the bindings
/// exist only at run time: a direct `eval` is compiled after the frame it
/// belongs to already exists.
///
/// Nearest wins, and the list is expected to arrive that way: a name at one hop
/// is the caller's answer for it, and a second entry further out would be
/// shadowed. That rule is the caller's rather than restated here, because it is
/// the same rule `Scope::lookup` already applies to everything else.
///
/// # The completion value, and the part of it that is not answered
///
/// `eval("1 + 2 * 3")` is `7`, so the last statement's value has to come back.
/// A trailing EXPRESSION statement becomes a `return`, which is what makes that
/// work — and it is the whole of what is implemented. The specification's
/// completion value also flows out of an `if`, a block and a loop
/// (`eval("if (1) 2")` is `2`), and those answer `undefined` here.
///
/// Named rather than hidden: it is a wrong answer for source of that shape, and
/// the honest fix is a completion-value lowering over every statement form
/// rather than a second special case per form. What makes this shippable
/// meanwhile is that the wrong answer is `undefined` — the value of a statement
/// that produced nothing — rather than another program's variable, which is the
/// failure the refusal this replaces was protecting against.
///
/// # `hide_node_globals`
///
/// A direct `eval` has no scope of its own to have decided this — it is
/// compiled as a fresh, separate program (`rts-host`'s `Scoped::Eval`) with no
/// `Ctx` surviving from whatever called it. So the caller reads the fact back
/// off the running environment instead:
/// `rts_core::entry::hides_node_globals(environment)` walks the SAME
/// `__rts_outer` chain `environment_names` just walked, checking the mark
/// `rts_core::entry::mark_hides_node_globals` left when that environment's
/// OWN scope (a page script, a `vm` sandbox) was compiled. Without this,
/// `eval("process")` called from inside a page would answer a real value even
/// after the page's own bare read stopped — the second leak the 2026-09-04
/// audit named, and the reason `NODE_ONLY`'s own doc says this list alone was
/// never enough.
pub fn emit_eval_program(
    body: &[Stmt],
    enclosing: &[(crate::names::Name, u32)],
    hide_node_globals: bool,
    ctx: &mut Ctx,
) -> EmitResult<Program> {
    let mut body = body.to_vec();
    if let Some(last) = body.pop() {
        let last = match last.kind {
            crate::syntax::StmtKind::Expr(value) => {
                Stmt::new(crate::syntax::StmtKind::Return(Some(value)), last.at)
            }
            other => Stmt::new(other, last.at),
        };
        body.push(last);
    }
    // Same reasoning as `page::emit_page_program`: a name found in the chain
    // resolves at zero hops, before `globals::resolves` — and so before
    // `NODE_ONLY` — is ever consulted, so the filter has to run here rather
    // than trusting that check alone.
    let enclosing: Vec<(crate::names::Name, u32)> = match hide_node_globals {
        true => super::globals::without_node_only(ctx, enclosing),
        false => enclosing.to_vec(),
    };
    ctx.hide_node_globals = hide_node_globals;
    let scope = Scope::for_function(
        None,
        std::collections::BTreeSet::new(),
        // Nothing is captured, so nothing is bound at zero hops either.
        &std::collections::BTreeSet::new(),
        &enclosing,
    );
    emit_program_into(&body, &[], None, &[], &scope, ctx)
}
