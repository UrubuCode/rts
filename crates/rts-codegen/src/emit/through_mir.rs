//! A function of the running program, compiled through the MIR stage instead of here.
//!
//! # What this is, and why it is a door and not a switch
//!
//! `docs/engine/four-stages.md` measures how many functions of the corpus the new
//! stage turns into code the machine accepts. None of them RAN: the number said what
//! compiled, never what answered. This is where one runs -- `emit_function` asks here
//! first, and a function this door takes is the one the program executes.
//!
//! It is per function because the two stages have to share one program: the running
//! emitter still makes every function this door declines, and the two meet at the
//! calling convention, which both now keep (`signature()`, adopted by the MIR boundary
//! for exactly this). Where they would have to agree about more than that, this door
//! declines -- and each reason is listed below, because a function taken where the two
//! disagree is a wrong answer that compiles.
//!
//! # What is declined, and the disagreement each one avoids
//!
//! - **An environment built here.** The running emitter lays environments out per block
//!   and per loop pass (`emit/binding.rs`); the MIR stage lays out one per activation. A
//!   closure made by one and read by the other would walk a different number of links.
//!   A closure made here IS taken, over a body the running emitter emits -- see
//!   `Nested` -- because a function that builds no environment hands its closures the
//!   one it was made in, which is what the running emitter would have handed them. A read or write of a name the ENCLOSING function
//!   holds is taken, because its links and its key come from the running emitter's own
//!   scope -- `lower::lower_within`.
//! - **A call to a function of the module by number** -- the MIR boundary refuses it
//!   anyway.
//! - **A suspension**, a generator or an `async` body: the running emitter wraps those
//!   and puts them through `resumable_form`, which this stage does not do yet.
//! - **A global the running emitter would not read as a global.** It answers a name no
//!   program declares with `UnboundGlobalGet` -- the `ReferenceError` -- and a name its
//!   own scope binds without a declaration in the tree (CommonJS's `require`, a page
//!   script's window) through that binding. The MIR stage reads every undeclared name
//!   through `GlobalGet`. So a function is taken only where each global it reads is one
//!   `globals::resolves` or `predefined` places AND the enclosing scope does not bind.
//! - **Sloppy code** (`eval`, `Function` text), where `this` and writes differ, and a
//!   body inside a `with`.
//!
//! What remains is what the two stages were built to agree on: a function's own
//! locals, its parameters, operators, property access, calls through values, `try`,
//! and globals. The machine's verifier has the last word, as it does in the
//! instrument.

use rts_cranelift::ir::Function as MachineFunction;

use super::{Ctx, Scope};
use crate::domain::{JsConst, JsPrim};
use crate::syntax::Function;

/// The environment variable that turns this door off, for measuring the running
/// emitter alone against the same binary. Anything but `0` leaves it on.
const SWITCH: &str = "RTS_MIR";

/// Whether this door is open for this compilation.
pub(super) fn open() -> bool {
    std::env::var(SWITCH).map_or(true, |held| held != "0")
}

/// The function compiled through the MIR stage, or `None` where it declines.
pub(super) fn try_emit(
    ctx: &mut Ctx,
    enclosing: &Scope,
    function: &Function,
) -> Option<MachineFunction> {
    let traced = std::env::var(TRACE).ok();
    let answer = attempt(ctx, enclosing, function);
    let named = || {
        let spelled = function
            .name
            .map_or("<anonymous>", |name| ctx.names.text(name));
        format!("{spelled} @{}", function.at.0)
    };
    match answer {
        // WHICH FUNCTIONS RAN THROUGH HERE, on request: a count of what compiled says
        // nothing about what executed, and this is the one place that knows both.
        Ok(machine) => {
            if traced.is_some() {
                eprintln!("[mir] {}", named());
            }
            Some(machine)
        }
        // AND WHY THE REST DID NOT, with `why`: the work list, measured on what runs --
        // the reason first, so a count by reason is a sort of the lines.
        Err(why) => {
            if traced.as_deref() == Some("why") {
                eprintln!("[mir-declined] {why} -- {}", named());
            }
            None
        }
    }
}

fn attempt(
    ctx: &mut Ctx,
    enclosing: &Scope,
    function: &Function,
) -> Result<MachineFunction, String> {
    let refused = [
        (ctx.sloppy, "sloppy code"),
        (function.is_async, "an async function"),
        (function.is_generator, "a generator"),
        (!ctx.with_objects.is_empty(), "inside `with`"),
        (ctx.in_field_initializer, "a field initialiser"),
    ];
    if let Some((_, why)) = refused.iter().find(|(held, _)| *held) {
        return Err((*why).to_owned());
    }
    let resolution = ctx
        .mir_resolution
        .clone()
        .ok_or("no scope tree for this program")?;
    // THE FUNCTIONS WRITTEN DIRECTLY INSIDE, whose closures this function makes. Their
    // BODIES are emitted by the running emitter -- in the scope this function sits in,
    // which is what they reach: a function this door takes builds no environment, so
    // nothing of its own is captured by them -- and the closure is made here from the
    // id that answers. A class is declined: its methods are installed with a home object
    // the MIR stage's class lowering and the running emitter's do not agree about.
    let mut nested = Nested::default();
    match &function.body {
        crate::syntax::FunctionBody::Block(statements) => {
            statements.iter().for_each(|held| nested.statement(held))
        }
        crate::syntax::FunctionBody::Expression(value) => nested.expression(value),
    }
    if nested.class {
        return Err("a class written inside".to_owned());
    }
    // AN ARROW INSIDE READS `this` -- and `arguments`, `super`, `new.target` -- FROM
    // THIS FUNCTION, which the running emitter hands it through an environment slot
    // this function would have to build. Declined rather than built.
    if nested
        .found
        .iter()
        .any(|(inner, _)| inner.captures_this && reads_enclosing_this(inner, ctx.names))
    {
        return Err("an arrow inside reads this function's `this`".to_owned());
    }
    let positions: Vec<rts_cranelift::fault::Position> =
        nested.found.iter().map(|(inner, _)| inner.at).collect();
    let callees = crate::lower::Callees::of_positions(&positions);
    let mut domain = crate::domain::Js::new();
    // THE RUNNING EMITTER'S LAYOUT for every name this function does not own: it makes
    // the closure, from `enclosing`'s environment, so its scope is what says how many
    // links out a name is and under which key.
    let layout = |name: crate::names::Name| match enclosing.lookup(name) {
        Some(super::scope::Binding::InEnvironment { hops, name }) => Some((hops, name)),
        _ => None,
    };
    let graph = crate::lower::lower_within(
        function,
        &resolution,
        &callees,
        &mut domain,
        ctx.names,
        rts_mir::guard::Tier::Generic,
        Some(&layout),
    )
    .map_err(|held| format!("lowering: {held:?}"))?;
    let unbound = agrees(ctx, enclosing, &graph, &domain)?;
    let written = graph.block(graph.entry()).params.len();
    if written > crate::runtime::ARGUMENT_SLOTS {
        return Err("more parameters than the convention has slots".to_owned());
    }

    let inferred = rts_mir::infer::infer(&graph, &domain);
    let mut machine = MachineFunction::new(super::function::signature());
    let entry = machine.entry;
    let start: Vec<_> = machine.block(entry).ok_or("no entry block")?.params.clone();
    // THE NESTED BODIES, now that this function is known to lower -- emitted before the
    // machine lowering that takes their addresses. A machine refusal after this point
    // leaves them emitted and unused, and the running emitter emits them again with this
    // function: a waste the verifier-accepted path does not pay.
    //
    // The name THIS function was lent is put aside while they are emitted and put back
    // after: it is taken when this function's own id is recorded, which is after its
    // body, and the first nested definition to ask would otherwise take it.
    let built = inner_scope(enclosing, &resolution, function);
    let inside = built.as_ref().unwrap_or(enclosing);
    let outer_name = ctx.take_lent_name();
    let mut module = Vec::with_capacity(nested.found.len());
    for (inner, declared) in &nested.found {
        if let Some(name) = nested.lent.get(&inner.at)
            && !ctx.names.text(*name).starts_with("__rts_")
        {
            ctx.lend_name(*name);
        }
        ctx.mir_candidate = true;
        let made = super::function::nested_code(ctx, inside, inner, *declared);
        let _ = ctx.take_lent_name();
        match made {
            Ok(id) => module.push(id),
            Err(held) => {
                ctx.restore_lent_name(outer_name);
                return Err(format!("a nested function: {held:?}"));
            }
        }
    }
    ctx.restore_lent_name(outer_name);
    let lowered = {
        let mut into = rts_cranelift::ir::FuncBuilder::new(&mut machine, ctx.types, entry);
        let parts = crate::machine::Parts {
            funcs: &mut *ctx.funcs,
            calls: &mut *ctx.calls,
            literals: &mut ctx.literals,
            keys: &mut *ctx.keys,
            model: ctx.model,
            module: &module,
        };
        let mut ops = crate::machine::JsMachine::new(&domain, inferred)
            .tail_calls_of(&graph)
            .unbound_reads(unbound)
            .declaring_into(parts)
            .naming_with(&mut *ctx.names)
            .with_incoming(&start);
        rts_mir::lower::lower(&graph, &mut into, &mut ops, &start[2..2 + written])
    };
    lowered.map_err(|held| format!("machine: {held:?}"))?;
    let refused = rts_cranelift::verify(&machine, ctx.types, ctx.funcs);
    match refused.first() {
        None => Ok(machine),
        Some(first) => Err(format!("verifier: {first:?}")),
    }
}

/// Set to anything to have every function this door takes named on standard error, and
/// to `why` to have every one it declines give its reason there as well.
const TRACE: &str = "RTS_MIR_TRACE";

/// Whether every operation in the graph is one the two stages agree about.
fn agrees(
    ctx: &mut Ctx,
    enclosing: &Scope,
    graph: &rts_mir::cfg::Func,
    domain: &crate::domain::Js,
) -> Result<std::collections::BTreeSet<rts_mir::ValueId>, String> {
    let mut unbound = std::collections::BTreeSet::new();
    let window = super::page::page_window_name(ctx);
    let in_page = enclosing.lookup(window).is_some();
    for inst in &graph.insts {
        match &inst.op {
            rts_mir::Op::Suspend { .. } => return Err("a suspension".to_owned()),
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Func(_),
                ..
            } => return Err("a call by number".to_owned()),
            rts_mir::Op::Prim { prim, args } => match domain.meaning(*prim) {
                // AN ENVIRONMENT BUILT HERE is laid out by this stage and read by what
                // the running emitter makes inside it, through the layer `attempt`
                // hands the nested emission -- `lower/environment.rs` says why the two
                // layouts are one.
                Some(JsPrim::GlobalRead) => {
                    let Some(name) = args.first().and_then(|key| key_name(graph, domain, *key))
                    else {
                        return Err("a global read with no fixed key".to_owned());
                    };
                    let text = ctx.names.text(name);
                    if enclosing.lookup(name).is_some() {
                        return Err(format!("{text}, which the enclosing scope binds"));
                    }
                    let placed = super::globals::resolves(ctx, name)
                        || matches!(text, "undefined" | "NaN" | "Infinity");
                    if placed {
                        continue;
                    }
                    // A NAME NOTHING PLACED is the running emitter's `unbound_read`: the
                    // global object is asked when the read RUNS, and its absence is a
                    // `ReferenceError` then -- `dom` and `DomTimers` are installed by the
                    // host, which no compile-time list sees. Two cases are that emitter's
                    // alone: a page script, whose sibling scripts write its window, and
                    // `typeof`, which the language exempts from the error.
                    if in_page {
                        return Err(format!("the unplaced global {text}, in a page script"));
                    }
                    if typeof_reads(graph, domain, inst.result) {
                        return Err(format!("`typeof` of the unplaced global {text}"));
                    }
                    unbound.insert(inst.result);
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(unbound)
}

/// The scope a function written inside this one is emitted in, when this one builds an
/// environment: the enclosing scope one link further out, under a layer holding what
/// this function's environment holds. `None` when it builds none -- the closure is then
/// handed the enclosing environment, and the enclosing scope is the answer as it stands.
fn inner_scope(
    enclosing: &Scope,
    resolution: &crate::names::resolve::Resolution,
    function: &Function,
) -> Option<Scope> {
    let owned: std::collections::BTreeSet<crate::names::Name> = resolution
        .function_scope(function.at)
        .map(|scope| resolution.environment_of(scope))
        .unwrap_or_default()
        .into_iter()
        .map(|binding| resolution.binding(binding).name)
        .collect();
    if owned.is_empty() {
        return None;
    }
    let shifted: Vec<_> = enclosing
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + 1))
        .collect();
    Some(Scope::for_function(None, owned.clone(), &owned, &shifted))
}

/// Whether a `typeof` reads this value.
fn typeof_reads(
    graph: &rts_mir::cfg::Func,
    domain: &crate::domain::Js,
    value: rts_mir::ValueId,
) -> bool {
    graph.insts.iter().any(|inst| match &inst.op {
        rts_mir::Op::Prim { prim, args } => {
            domain.meaning(*prim) == Some(JsPrim::TypeOf) && args.contains(&value)
        }
        _ => false,
    })
}

/// The name a declared key constant spells, if that is what defined `value`.
fn key_name(
    graph: &rts_mir::cfg::Func,
    domain: &crate::domain::Js,
    value: rts_mir::ValueId,
) -> Option<crate::names::Name> {
    let defined = graph.insts.iter().find(|inst| inst.result == value)?;
    let rts_mir::Op::Const(rts_mir::Const::Declared(index)) = defined.op else {
        return None;
    };
    match domain.declared(index) {
        Some(JsConst::Key(name)) => Some(*name),
        _ => None,
    }
}

/// The functions written directly inside a body -- not inside those -- and whether a
/// class is written there.
#[derive(Default)]
struct Nested<'a> {
    found: Vec<(&'a Function, bool)>,
    class: bool,
    /// The name each anonymous definition is given by where it is written --
    /// NamedEvaluation, which the running emitter carries in `Ctx::lend_name` from the
    /// site that writes it. Keyed by position because those sites are here the PARENT of
    /// the function and the walk meets the function as a child.
    ///
    /// Three sites, the ones a function this door takes can hold: an initialiser of a
    /// name, an assignment to one, and a property written under a name. A pattern's
    /// default is the fourth, and its functions are never reached by this walk -- so
    /// they have no number, and the lowering declines the function for it.
    lent: std::collections::BTreeMap<rts_cranelift::fault::Position, crate::names::Name>,
}

impl Nested<'_> {
    fn lend(&mut self, value: &crate::syntax::Expr, name: crate::names::Name) {
        if let crate::syntax::ExprKind::Function(inner) = &value.kind
            && inner.name.is_none()
        {
            self.lent.insert(inner.at, name);
        }
    }
}

impl<'a> Nested<'a> {
    fn statement(&mut self, statement: &'a crate::syntax::Stmt) {
        use crate::emit::capture::StmtChild;
        crate::emit::capture::walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.statement(inner),
            StmtChild::Expr(value) => self.expression(value),
            StmtChild::Binding(binding) => {
                if let Some(value) = &binding.value {
                    if let crate::syntax::Pattern::Name(name) = binding.target {
                        self.lend(value, name);
                    }
                    self.expression(value);
                }
            }
            StmtChild::Catch(clause) => clause.body.iter().for_each(|held| self.statement(held)),
            StmtChild::Function(inner) => self.found.push((inner, true)),
            StmtChild::Class(_) => self.class = true,
        });
    }

    fn expression(&mut self, value: &'a crate::syntax::Expr) {
        use crate::emit::capture::Child;
        use crate::syntax::{AssignOp, AssignTarget, ExprKind, Property, PropertyKey};
        match &value.kind {
            ExprKind::Function(inner) => return self.found.push((inner, false)),
            ExprKind::Class(_) => {
                self.class = true;
                return;
            }
            // `f = () => {}` and `f ??= () => {}` name the arrow; `f += ...` names
            // nothing, and neither does `o.f = ...` -- the rule is attached to an
            // identifier reference on the left.
            ExprKind::Assign {
                target: AssignTarget::Place(place),
                value: assigned,
                op,
            } if !matches!(op, AssignOp::Compound(_)) => {
                if let ExprKind::Ident(name) = place.kind {
                    self.lend(assigned, name);
                }
            }
            ExprKind::Object { properties } => {
                for property in properties {
                    if let Property::Value {
                        key: PropertyKey::Named(name),
                        value: held,
                        ..
                    } = property
                    {
                        self.lend(held, *name);
                    }
                }
            }
            _ => {}
        }
        crate::emit::capture::walk_expr(value, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner),
            Child::Function(inner) => self.found.push((inner, false)),
            Child::Class(_) => self.class = true,
        });
    }
}

/// Whether an arrow reads what an arrow takes from the function it is written in --
/// `this`, `arguments`, `super`, `new.target` -- itself or through an arrow inside it.
fn reads_enclosing_this(arrow: &Function, names: &crate::names::Names) -> bool {
    fn statement(held: &crate::syntax::Stmt, names: &crate::names::Names) -> bool {
        use crate::emit::capture::StmtChild;
        let mut found = false;
        crate::emit::capture::walk_stmt(held, &mut |child| {
            found |= match child {
                StmtChild::Stmt(inner) => statement(inner, names),
                StmtChild::Expr(value) => expression(value, names),
                StmtChild::Binding(binding) => {
                    binding.value.as_ref().is_some_and(|value| expression(value, names))
                }
                StmtChild::Catch(clause) => clause.body.iter().any(|held| statement(held, names)),
                StmtChild::Function(inner) => inner.captures_this && reads_enclosing_this(inner, names),
                // A class body has its own `this`; its computed keys and heritage do not,
                // and are rare enough to count as reading it.
                StmtChild::Class(_) => true,
            };
        });
        found
    }
    fn expression(value: &crate::syntax::Expr, names: &crate::names::Names) -> bool {
        use crate::emit::capture::Child;
        use crate::syntax::ExprKind;
        match &value.kind {
            ExprKind::This
            | ExprKind::NewTarget
            | ExprKind::SuperMember { .. }
            | ExprKind::SuperCall { .. } => return true,
            ExprKind::Ident(name) if names.spelled(*name) == Some("arguments") => return true,
            _ => {}
        }
        let mut found = false;
        crate::emit::capture::walk_expr(value, &mut |child| {
            found |= match child {
                Child::Expr(inner) => expression(inner, names),
                Child::Function(inner) => inner.captures_this && reads_enclosing_this(inner, names),
                Child::Class(_) => true,
            };
        });
        found
    }
    match &arrow.body {
        crate::syntax::FunctionBody::Block(statements) => {
            statements.iter().any(|held| statement(held, names))
        }
        crate::syntax::FunctionBody::Expression(value) => expression(value, names),
    }
}
