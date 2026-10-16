//! A call to a function declaration, as a machine call and not a trip through the door.
//!
//! # What this removes, and what it must not change
//!
//! `f(x)` through `RuntimeOp::Call` pays a crossing into the runtime: the callee is read
//! out of an environment, resolved to a code address, checked for being a class
//! constructor, and only then jumped to. When the program proves WHICH function a name is,
//! and that the function needs none of what the door supplies, the call is a machine call
//! to that function's id: `fn(env, this, a0..a3) -> value`, the convention both emitters
//! keep (`function::signature`).
//!
//! # The proof, clause by clause (all must hold, else the door is used)
//!
//! - **One declaration, never written.** A top-level `function` declaration whose name the
//!   whole body declares exactly once, and which `primordial::disturbed` never sees
//!   assigned (`g = 5`, `g++`, a destructuring target, `eval`, `globalThis`). So every
//!   call spelled `g(...)` whose scope can see it reaches this function.
//! - **Captures nothing.** The environment is `undefined`, so no free name of the body may
//!   resolve to an enclosing binding. `capture::names_in_function` is the engine's own
//!   over-approximating answer to "what does this function mention that it does not own";
//!   each such name must then be a global (declared nowhere, and unbound at the site) or
//!   declared ONLY inside the function, or be a top-level `let`/`const`/`var` of its body.
//!   Its own name is a capture (recursion) and refuses.
//! - **Light.** `light_call::is_light`, decided where the body is emitted: no `arguments`,
//!   rest, fifth parameter, `new.target`, `eval`, and strict. The door's bookkeeping for
//!   those is exactly what a direct call skips.
//! - **A leaf.** No call, `new`, tagged template, `await`, `yield`, `super`, class,
//!   nested function, loop over an iterator, `using` or `with`. The door records the callee
//!   on a stack that error traces read, and settles a pending tail call; with no call in
//!   the body there is no tail call to settle. A throw from an OPERATION inside the body
//!   (a property read of `undefined`) therefore carries a trace without this frame -- the
//!   one observable difference, and only in the text of `stack`.
//! - **Plain parameters.** No default and no destructuring, so the free-name rule above
//!   cannot be confused by a default evaluated in the parameter scope.
//!
//! The target must already be emitted when the call site is: its id exists only then. A
//! call written above the declaration in source order keeps the door.

use std::collections::{BTreeMap, BTreeSet};

use rts_cranelift::fault::Position;

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::inline::Declarations;
use crate::names::Name;
use crate::runtime::ARGUMENT_SLOTS;
use crate::syntax::{
    Expr, ExprKind, Function, FunctionBody, Pattern, Stmt, StmtKind,
};

/// A declaration this pass may call directly, once its id is known.
pub(super) struct Direct {
    /// Where it was written, which is what ties the emitted function to this record.
    pub at: Position,
    /// The names it reads that nothing declares: each must also be unbound at a call site.
    pub globals: Vec<Name>,
}

/// `f(a, b)` written in code the RUNNING emitter emits (the program's entry, a module body, a
/// function it declines to hand the MIR stage), as a machine call to `f`'s id.
///
/// The same proof as the MIR path, asked at the site: `candidates` proved the declaration,
/// `Ctx::direct_targets` holds it only once its id exists (a call emitted above the
/// declaration keeps the door), the name must be bound here -- a name resolving to a global
/// is another thing -- and so must no global the callee reads. Plain `Call` only, no spread,
/// at most [`ARGUMENT_SLOTS`] arguments.
pub(super) fn emit(
    builder: &mut rts_cranelift::ir::FuncBuilder,
    scope: &mut super::Scope,
    ctx: &mut super::Ctx,
    callee: &Expr,
    arguments: &[crate::syntax::Spreadable],
) -> super::EmitResult<Option<rts_cranelift::ir::ValueId>> {
    let ExprKind::Ident(name) = &callee.kind else {
        return Ok(None);
    };
    let Some(target) = ctx.direct_targets.get(name).copied() else {
        return Ok(None);
    };
    let Some(candidate) = ctx.direct_candidates.get(name) else {
        return Ok(None);
    };
    if arguments.len() > ARGUMENT_SLOTS
        || scope.lookup(*name).is_none()
        || candidate.globals.iter().any(|held| scope.lookup(*held).is_some())
    {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(ARGUMENT_SLOTS);
    for argument in arguments {
        let crate::syntax::Spreadable::Single(value) = argument else {
            return Ok(None);
        };
        values.push(value);
    }
    let undefined = super::expr::undefined(builder, ctx);
    let mut passed = vec![undefined, undefined];
    for value in values {
        let held = super::expr::emit_expr(builder, scope, ctx, value)?;
        passed.push(super::expr::tagged(builder, held));
    }
    while passed.len() < 2 + ARGUMENT_SLOTS {
        passed.push(undefined);
    }
    let produced = builder.call(ctx.funcs, target, &passed)?;
    super::expr::raise_if_thrown(builder, ctx)?;
    Ok(Some(produced[0]))
}

/// The declarations of `body` that pass every clause that can be decided from the tree.
pub(super) fn candidates(
    body: &[Stmt],
    eval: Name,
    global_this: Name,
) -> BTreeMap<Name, Direct> {
    let mut found = BTreeMap::new();
    if super::capture::has_with(body) {
        return found;
    }
    let declared = Declarations::of(body);
    let writes = super::primordial::disturbed(body, eval, global_this);
    for statement in body {
        let StmtKind::Function(function) = &statement.kind else {
            continue;
        };
        let Some(name) = function.name else {
            continue;
        };
        if function.is_async
            || function.is_generator
            || function.captures_this
            || function.rest_parameter.is_some()
            || function.parameters.len() > ARGUMENT_SLOTS
            || function
                .parameters
                .iter()
                .any(|held| held.default.is_some() || !matches!(held.target, Pattern::Name(_)))
        {
            continue;
        }
        if declared.count(name) != 1 || !writes.untouched(name) {
            continue;
        }
        if !is_leaf(function) {
            continue;
        }
        let Some(globals) = free_globals(statement, function, name, &declared) else {
            continue;
        };
        found.insert(
            name,
            Direct {
                at: function.at,
                globals,
            },
        );
    }
    found
}

/// The names the function reads that no declaration of the body spells, or `None` where
/// some name may resolve to a binding outside it.
fn free_globals(
    statement: &Stmt,
    function: &Function,
    name: Name,
    declared: &Declarations,
) -> Option<Vec<Name>> {
    let mut free = BTreeSet::new();
    super::capture::names_in_function(function, &mut free);
    let inside = Declarations::of(std::slice::from_ref(statement));
    let mut own_top = BTreeSet::new();
    if let FunctionBody::Block(statements) = &function.body {
        for held in statements {
            match &held.kind {
                StmtKind::Declare { bindings, .. } => {
                    for binding in bindings {
                        let mut bound = Vec::new();
                        binding.target.bound_names(&mut bound);
                        own_top.extend(bound);
                    }
                }
                StmtKind::Function(inner) => own_top.extend(inner.name),
                _ => {}
            }
        }
    }
    let mut globals = Vec::new();
    for held in free {
        if held == name {
            return None;
        }
        if own_top.contains(&held) {
            continue;
        }
        let all = declared.count(held);
        if all == 0 {
            globals.push(held);
        } else if all != inside.count(held) {
            return None;
        }
    }
    Some(globals)
}

/// Whether the body holds nothing the door would have had to account for.
fn is_leaf(function: &Function) -> bool {
    let mut probe = Leaf { refused: false };
    match &function.body {
        FunctionBody::Block(body) => body.iter().for_each(|held| probe.statement(held)),
        FunctionBody::Expression(value) => probe.expression(value),
    }
    !probe.refused
}

struct Leaf {
    refused: bool,
}

impl Leaf {
    fn statement(&mut self, statement: &Stmt) {
        if self.refused {
            return;
        }
        if matches!(
            statement.kind,
            StmtKind::ForEach { .. } | StmtKind::Using { .. } | StmtKind::With { .. }
        ) {
            self.refused = true;
            return;
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.statement(inner),
            StmtChild::Expr(value) => self.expression(value),
            StmtChild::Binding(binding) => {
                if let Some(value) = &binding.value {
                    self.expression(value);
                }
            }
            StmtChild::Catch(clause) => clause.body.iter().for_each(|inner| self.statement(inner)),
            StmtChild::Function(_) | StmtChild::Class(_) => self.refused = true,
        });
    }

    fn expression(&mut self, value: &Expr) {
        if self.refused {
            return;
        }
        if matches!(
            value.kind,
            ExprKind::Call { .. }
                | ExprKind::New { .. }
                | ExprKind::TaggedTemplate { .. }
                | ExprKind::Await(_)
                | ExprKind::Yield { .. }
                | ExprKind::SuperCall { .. }
                | ExprKind::SuperMember { .. }
                | ExprKind::NewTarget
                | ExprKind::Class(_)
                | ExprKind::Function(_)
        ) {
            self.refused = true;
            return;
        }
        walk_expr(value, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner),
            Child::Function(_) | Child::Class(_) => self.refused = true,
        });
    }
}
