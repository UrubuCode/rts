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
        (
            function.is_async && function.is_generator,
            "an async generator, whose await drains where its yield parks",
        ),
        (!ctx.with_objects.is_empty(), "inside `with`"),
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
    // id that answers. A class is compiled the same way, inside a helper -- `helper_of`.
    let mut nested = Nested::default();
    match &function.body {
        crate::syntax::FunctionBody::Block(statements) => {
            statements.iter().for_each(|held| nested.statement(held))
        }
        crate::syntax::FunctionBody::Expression(value) => nested.expression(value),
    }
    // A CLASS INSIDE AN ARROW would need the arrow's `this` handed to its helper, which
    // is the enclosing function's and not carried here.
    if function.captures_this
        && nested
            .classes
            .iter()
            .any(|class| class_reads_enclosing_this(class, ctx.names))
    {
        return Err("a class inside an arrow, whose helper would need the enclosing `this`".to_owned());
    }
    // AN OBJECT LITERAL'S HELPER receives this function's `this` and nothing else, so
    // what its values read of `arguments`, `super` or `new.target` would be the
    // helper's; and a suspension cannot move into it at all.
    for object in &nested.objects {
        let written = [crate::syntax::Stmt {
            kind: crate::syntax::StmtKind::Expr((*object).clone()),
            at: object.at,
        }];
        if super::suspends::body_suspends(&written) {
            return Err("an object literal that suspends, which a helper cannot".to_owned());
        }
        let reads = Reads {
            this: function.captures_this,
            arguments: true,
        };
        if expression_reads(object, ctx.names, reads) {
            return Err("an object literal reading what its helper would not have".to_owned());
        }
    }
    let helpers: Vec<Function> = nested
        .classes
        .iter()
        .map(|class| {
            helper_of(crate::syntax::Expr {
                kind: crate::syntax::ExprKind::Class(Box::new((*class).clone())),
                at: class.at,
            })
        })
        .chain(nested.objects.iter().map(|object| helper_of((*object).clone())))
        .collect();
    // AN ARROW INSIDE READS `this` OR `arguments` FROM THIS FUNCTION, which the running
    // emitter hands it through environment slots under `__rts_this` and `arguments` --
    // so the environment built here holds both, and the layer the arrow is emitted in
    // shows them. `super` and `new.target` have no slot, and still decline.
    let arrows: Vec<&Function> = nested
        .found
        .iter()
        .map(|(inner, _)| *inner)
        .filter(|inner| inner.captures_this)
        .collect();
    if arrows.iter().any(|inner| arrow_reads(inner, ctx.names, Reads::FRAME)) {
        return Err("an arrow inside reads this function's `super` or `new.target`".to_owned());
    }
    let lexical: Vec<crate::names::Name> =
        match arrows.iter().any(|inner| reads_enclosing_this(inner, ctx.names)) {
            true => vec![ctx.names.intern("__rts_this"), ctx.names.intern("arguments")],
            false => Vec::new(),
        };
    // The functions, then the class helpers, numbered in that order -- the order the
    // module list below is filled in.
    let positions: Vec<rts_cranelift::fault::Position> = nested
        .found
        .iter()
        .map(|(inner, _)| inner.at)
        .chain(helpers.iter().map(|helper| helper.at))
        .collect();
    // A SITE PER TAGGED TEMPLATE, minted as `emit/template.rs` mints one: the cooked
    // text then the raw, per piece. Minted before the lowering, so a function this door
    // then declines leaves rows nothing reads -- a cost in table size, never in meaning,
    // since the running emitter mints its own for what it emits.
    let mut sites = std::collections::BTreeMap::new();
    for template in &nested.templates {
        let crate::syntax::ExprKind::TaggedTemplate { parts, .. } = &template.kind else {
            continue;
        };
        let mut pieces = Vec::with_capacity(parts.len() * 2);
        for part in parts {
            pieces.push(match &part.cooked {
                Some(text) => ctx.literal_units(text.units()),
                None => super::NO_COOKED,
            });
            pieces.push(ctx.literal(&part.raw));
        }
        sites.insert(template.at, ctx.template(pieces));
    }
    let callees = crate::lower::Callees::of_positions(&positions).with_templates(sites);
    let mut domain = crate::domain::Js::new();
    // THE RUNNING EMITTER'S LAYOUT for every name this function does not own: it makes
    // the closure, from `enclosing`'s environment, so its scope is what says how many
    // links out a name is and under which key.
    let layout = |name: crate::names::Name| match enclosing.lookup(name) {
        Some(super::scope::Binding::InEnvironment { hops, name }) => Some((hops, name)),
        _ => None,
    };
    // `callee` is the key a non-strict `arguments` object defines, and the lowering
    // reads the interner rather than growing it.
    if ctx.sloppy {
        ctx.names.intern("callee");
    }
    let (graph, placement) = crate::lower::lower_within(
        function,
        &resolution,
        &callees,
        &mut domain,
        ctx.names,
        rts_mir::guard::Tier::Generic,
        Some(&layout),
        &lexical,
        ctx.sloppy,
    )
    .map_err(|held| format!("lowering: {held:?}"))?;
    let unbound = agrees(ctx, enclosing, &graph, &domain)?;
    let written = graph.block(graph.entry()).params.len();
    if written > crate::runtime::ARGUMENT_SLOTS {
        return Err("more parameters than the convention has slots".to_owned());
    }

    let inferred = rts_mir::infer::infer(&graph, &domain);
    // A FUNCTION THAT PARKS says so on its signature, which the machine's verifier reads
    // before it accepts a suspension -- the same flag `emit_function` sets on the body
    // the running emitter builds.
    let suspends = function.is_async || function.is_generator;
    let mut signature = super::function::signature();
    signature.may_suspend = suspends;
    let mut machine = MachineFunction::new(signature);
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
    let outer_name = ctx.take_lent_name();
    let mut module = Vec::with_capacity(nested.found.len());
    let written_inside = nested
        .found
        .iter()
        .map(|(inner, declared)| (*inner, *declared, true))
        .chain(helpers.iter().map(|helper| (helper, false, false)));
    for (index, (inner, declared, candidate)) in written_inside.enumerate() {
        // THE SCOPE IT IS EMITTED IN is the chain of environments in force where its
        // closure was made: the pass environments there, innermost first, then this
        // function's, then the enclosing layout -- `lower/environment.rs`'s layout, said
        // in the running emitter's terms. Made at two sites under two chains, it has no
        // one scope, and the function is declined.
        let chain = match placement.get(&(index as u32)).map(Vec::as_slice) {
            None | Some([]) => Vec::new(),
            Some([first, rest @ ..]) => {
                if rest.iter().any(|other| other != first) {
                    ctx.restore_lent_name(outer_name);
                    return Err("a closure made under two different pass environments".to_owned());
                }
                first.clone()
            }
        };
        let built = inner_scope(enclosing, &resolution, function, &lexical, &chain);
        let inside = built.as_ref().unwrap_or(enclosing);
        if let Some(name) = nested.lent.get(&inner.at)
            && !ctx.names.text(*name).starts_with("__rts_")
        {
            ctx.lend_name(*name);
        }
        // A HELPER is not offered to this door: nothing in the scope tree was written at
        // its position, and the class it returns is the running emitter's by design.
        ctx.mir_candidate = candidate;
        let made = super::function::nested_code(ctx, inside, inner, declared);
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
        // NO TAIL CALL IN A FRAME THAT PARKS: the frame is the generator's or the
        // promise's, and replacing it would hand the resumer somebody else's --
        // `emit/tail.rs::permitted` refuses the same two kinds.
        let ops = crate::machine::JsMachine::new(&domain, inferred);
        // Nor in a NON-STRICT one, which `emit/tail.rs` refuses for the reason it
        // gives: `f.caller` and `arguments` observe the frame a tail call would drop.
        let ops = match suspends || ctx.sloppy {
            true => ops,
            false => ops.tail_calls_of(&graph),
        };
        let mut ops = ops
            .unbound_reads(unbound)
            .parking(suspends)
            .sloppy(ctx.sloppy)
            .method_reads_of(&graph)
            .asking_once_in((!suspends).then_some(entry))
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
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Func(_),
                ..
            } => return Err("a call by number".to_owned()),
            // A WRITE TO A GLOBAL is the running emitter's `globals::write` only for a
            // name its lists place; any other is a program it refuses to compile, or a
            // page script's window, and neither is this door's to decide.
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Entry(entry),
                args,
                ..
            } if domain.entry_meaning(*entry) == Some(crate::runtime::RuntimeOp::GlobalSet) => {
                let Some(name) = args.first().and_then(|key| key_name(graph, domain, *key)) else {
                    return Err("a global write with no fixed key".to_owned());
                };
                let text = ctx.names.text(name);
                if in_page || enclosing.lookup(name).is_some() || !super::globals::resolves(ctx, name) {
                    return Err(format!("a write to the global {text}, which is not placed"));
                }
            }
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
                    // host, which no compile-time list sees. A page script is that
                    // emitter's alone, since its sibling scripts write its window;
                    // `typeof` is exempt from the error, as the language says.
                    if in_page {
                        return Err(format!("the unplaced global {text}, in a page script"));
                    }
                    // Under `typeof` the read stays `GlobalGet`, which answers `undefined`
                    // for an absent name: the exemption `globals::force_read` makes.
                    if typeof_reads(graph, domain, inst.result) {
                        continue;
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
    lexical: &[crate::names::Name],
    chain: &[crate::names::resolve::ScopeId],
) -> Option<Scope> {
    let owned: std::collections::BTreeSet<crate::names::Name> = resolution
        .function_scope(function.at)
        .map(|scope| resolution.environment_of(scope))
        .unwrap_or_default()
        .into_iter()
        // A class body's bindings are its helper's, and a pass's are its own
        // environment's, as `lower/environment.rs` leaves both out of the object it
        // builds.
        .filter(|binding| {
            let scope = resolution.binding(*binding).scope;
            !resolution.in_class_body(scope) && !resolution.pass_environment(scope)
        })
        .map(|binding| resolution.binding(binding).name)
        .chain(lexical.iter().copied())
        .collect();
    // THE LAYERS, innermost first: each pass environment in force, then this function's
    // own where it builds one.
    let mut layers: Vec<std::collections::BTreeSet<crate::names::Name>> = chain
        .iter()
        .map(|scope| {
            resolution
                .captured_in(*scope)
                .into_iter()
                .map(|binding| resolution.binding(binding).name)
                .collect()
        })
        .collect();
    if !owned.is_empty() {
        layers.push(owned);
    }
    let Some(innermost) = layers.first().cloned() else {
        return None;
    };
    let depth = layers.len() as u32;
    // Outermost first, so that an inner layer's spelling shadows an outer one's.
    let mut reachable: Vec<_> = enclosing
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + depth))
        .collect();
    for (hops, layer) in layers.iter().enumerate().skip(1).rev() {
        reachable.extend(layer.iter().map(|name| (*name, hops as u32)));
    }
    Some(Scope::for_function(None, innermost.clone(), &innermost, &reachable))
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

/// The functions written directly inside a body -- not inside those -- and the classes.
#[derive(Default)]
struct Nested<'a> {
    found: Vec<(&'a Function, bool)>,
    classes: Vec<&'a crate::syntax::Class>,
    /// The object literals this stage does not build, which a helper does.
    objects: Vec<&'a crate::syntax::Expr>,
    /// The tagged templates, each of which needs a site minted.
    templates: Vec<&'a crate::syntax::Expr>,
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
        match &value.kind {
            crate::syntax::ExprKind::Function(inner) if inner.name.is_none() => {
                self.lent.insert(inner.at, name);
            }
            // An anonymous CLASS is named by its binding too, and it takes the name
            // inside its helper, where the running emitter emits it.
            crate::syntax::ExprKind::Class(class) if class.name.is_none() => {
                self.lent.insert(class.at, name);
            }
            _ => {}
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
            StmtChild::Class(class) => self.classes.push(class),
        });
    }

    fn expression(&mut self, value: &'a crate::syntax::Expr) {
        use crate::emit::capture::Child;
        use crate::syntax::{AssignOp, AssignTarget, ExprKind, Property, PropertyKey};
        match &value.kind {
            ExprKind::Function(inner) => return self.found.push((inner, false)),
            ExprKind::Class(class) => return self.classes.push(class),
            ExprKind::Object { properties } if crate::lower::built_elsewhere(properties) => {
                return self.objects.push(value);
            }
            ExprKind::TaggedTemplate { .. } => self.templates.push(value),
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
            Child::Class(class) => self.classes.push(class),
        });
    }
}

/// `function () { return <value> }` -- a class, or an object literal this stage does
/// not build, written inside a function this door takes, as the running emitter's code.
///
/// # Why the class is not lowered here
///
/// Because a class is the running emitter's in every detail a program can observe:
/// methods are installed non-enumerable with a home object, the constructor refuses a
/// call without `new`, fields run in the constructor, `extends` links two prototype
/// chains. `lower/class.rs` builds a class out of a closure and property writes, which
/// is a shape the stage can reason about and not the same object. So the class is
/// evaluated by a helper the running emitter compiles, at the point it is written,
/// with this function's `this` as the helper's receiver -- which is what its computed
/// keys and `extends` expression read -- and the value it answers is bound here.
fn helper_of(value: crate::syntax::Expr) -> Function {
    let at = value.at;
    Function {
        name: None,
        parameters: Vec::new(),
        rest_parameter: None,
        directives: Vec::new(),
        body: crate::syntax::FunctionBody::Block(vec![crate::syntax::Stmt {
            kind: crate::syntax::StmtKind::Return(Some(value)),
            at,
        }]),
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        advertised_length: None,
        at,
    }
}

/// Whether an arrow reads what an arrow takes from the function it is written in --
/// `this`, `arguments`, `super`, `new.target` -- itself or through an arrow inside it.
fn reads_enclosing_this(arrow: &Function, names: &crate::names::Names) -> bool {
    arrow_reads(arrow, names, Reads::ALL)
}

/// Which of what a function takes from where it is called a walk counts.
#[derive(Clone, Copy)]
struct Reads {
    this: bool,
    arguments: bool,
}

impl Reads {
    const ALL: Reads = Reads {
        this: true,
        arguments: true,
    };
    /// `super` and `new.target` alone -- what no environment slot hands an arrow.
    const FRAME: Reads = Reads {
        this: false,
        arguments: false,
    };
}

fn arrow_reads(arrow: &Function, names: &crate::names::Names, reads: Reads) -> bool {
    match &arrow.body {
        crate::syntax::FunctionBody::Block(statements) => statements
            .iter()
            .any(|held| statement_reads(held, names, reads)),
        crate::syntax::FunctionBody::Expression(value) => expression_reads(value, names, reads),
    }
}

/// Whether what a class evaluates where it is WRITTEN -- its `extends` expression and its
/// computed keys -- reads `this`. Its methods, fields and static blocks have the class's
/// own, so they ask nothing of the function the class sits in.
fn class_reads_enclosing_this(class: &crate::syntax::Class, names: &crate::names::Names) -> bool {
    let heritage = class
        .heritage
        .as_ref()
        .is_some_and(|value| expression_reads_this(value, names));
    heritage
        || class.body.iter().any(|element| match element.key() {
            Some(crate::syntax::ClassKey::Public(crate::syntax::PropertyKey::Computed(key))) => {
                expression_reads_this(key, names)
            }
            _ => false,
        })
}

fn statement_reads(
    held: &crate::syntax::Stmt,
    names: &crate::names::Names,
    reads: Reads,
) -> bool {
    use crate::emit::capture::StmtChild;
    let mut found = false;
    crate::emit::capture::walk_stmt(held, &mut |child| {
        found |= match child {
            StmtChild::Stmt(inner) => statement_reads(inner, names, reads),
            StmtChild::Expr(value) => expression_reads(value, names, reads),
            StmtChild::Binding(binding) => binding
                .value
                .as_ref()
                .is_some_and(|value| expression_reads(value, names, reads)),
            StmtChild::Catch(clause) => clause
                .body
                .iter()
                .any(|held| statement_reads(held, names, reads)),
            StmtChild::Function(inner) => inner.captures_this && arrow_reads(inner, names, reads),
            StmtChild::Class(class) => class_reads_enclosing_this(class, names),
        };
    });
    found
}

fn expression_reads_this(value: &crate::syntax::Expr, names: &crate::names::Names) -> bool {
    expression_reads(value, names, Reads::ALL)
}

/// Whether an expression reads what a function takes from where it is called --
/// `arguments`, `super`, `new.target`, and `this` when `this_too`. An arrow inside
/// counts whatever it reads, which over-reports `this` and is the safe direction.
fn expression_reads(value: &crate::syntax::Expr, names: &crate::names::Names, reads: Reads) -> bool {
    use crate::emit::capture::Child;
    use crate::syntax::ExprKind;
    match &value.kind {
        ExprKind::This => return reads.this,
        ExprKind::NewTarget | ExprKind::SuperMember { .. } | ExprKind::SuperCall { .. } => {
            return true;
        }
        ExprKind::Ident(name) if names.spelled(*name) == Some("arguments") => {
            return reads.arguments;
        }
        _ => {}
    }
    let mut found = false;
    crate::emit::capture::walk_expr(value, &mut |child| {
        found |= match child {
            Child::Expr(inner) => expression_reads(inner, names, reads),
            Child::Function(inner) => inner.captures_this && arrow_reads(inner, names, reads),
            Child::Class(class) => class_reads_enclosing_this(class, names),
        };
    });
    found
}
