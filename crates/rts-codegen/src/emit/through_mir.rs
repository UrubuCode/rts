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
//! - **An environment built here, or a closure made here.** The running emitter lays
//!   environments out per block and per loop pass (`emit/binding.rs`); the MIR stage
//!   lays out one per activation. A closure made by one and read by the other would
//!   walk a different number of links. A read or write of a name the ENCLOSING function
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
    match attempt(ctx, enclosing, function) {
        // WHICH FUNCTIONS RAN THROUGH HERE, on request: a count of what compiled says
        // nothing about what executed, and this is the one place that knows both.
        Ok(machine) => {
            if traced.is_some() {
                let named = function
                    .name
                    .map_or("<anonymous>", |name| ctx.names.text(name));
                eprintln!("[mir] {named} @{}", function.at.0);
            }
            Some(machine)
        }
        // AND WHY THE REST DID NOT, with `why`: the work list, measured on what runs.
        Err(why) => {
            if traced.as_deref() == Some("why") {
                eprintln!("[mir-declined] {why}");
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
    if ctx.sloppy
        || function.is_async
        || function.is_generator
        || !ctx.with_objects.is_empty()
        || ctx.in_field_initializer
    {
        return Err("sloppy, async, generator, with or field initialiser".to_owned());
    }
    let resolution = ctx
        .mir_resolution
        .clone()
        .ok_or("no scope tree for this program")?;
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
        &crate::lower::Callees::default(),
        &mut domain,
        ctx.names,
        rts_mir::guard::Tier::Generic,
        Some(&layout),
    )
    .map_err(|held| format!("lowering: {held:?}"))?;
    agrees(ctx, enclosing, &graph, &domain)?;
    let written = graph.block(graph.entry()).params.len();
    if written > crate::runtime::ARGUMENT_SLOTS {
        return Err("more parameters than the convention has slots".to_owned());
    }

    let inferred = rts_mir::infer::infer(&graph, &domain);
    let mut machine = MachineFunction::new(super::function::signature());
    let entry = machine.entry;
    let start: Vec<_> = machine.block(entry).ok_or("no entry block")?.params.clone();
    let module: [rts_cranelift::ir::FuncId; 0] = [];
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
    ctx: &Ctx,
    enclosing: &Scope,
    graph: &rts_mir::cfg::Func,
    domain: &crate::domain::Js,
) -> Result<(), String> {
    for inst in &graph.insts {
        match &inst.op {
            rts_mir::Op::Suspend { .. } => return Err("a suspension".to_owned()),
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Func(_),
                ..
            } => return Err("a call by number".to_owned()),
            rts_mir::Op::Prim { prim, args } => match domain.meaning(*prim) {
                // A CLOSURE MADE HERE, or an environment built here, is laid out by
                // this stage and read by whatever the running emitter makes inside it.
                // Reads and writes of the ENCLOSING layout are not: they come from the
                // running emitter's own scope, through `lower_within`.
                Some(JsPrim::EnvNew) => return Err("builds an environment".to_owned()),
                Some(JsPrim::MakeClosure) => return Err("makes a closure".to_owned()),
                Some(JsPrim::GlobalRead) => {
                    let Some(name) = args.first().and_then(|key| key_name(graph, domain, *key))
                    else {
                        return Err("a global read with no fixed key".to_owned());
                    };
                    let text = ctx.names.text(name);
                    let placed = super::globals::resolves(ctx, name)
                        || matches!(text, "undefined" | "NaN" | "Infinity");
                    if !placed {
                        return Err(format!("the unplaced global {text}"));
                    }
                    if enclosing.lookup(name).is_some() {
                        return Err(format!("{text}, which the enclosing scope binds"));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(())
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
