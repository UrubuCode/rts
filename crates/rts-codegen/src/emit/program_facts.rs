//! The facts every body of a compilation shares, computed once.
//!
//! Split out of `emit/mod.rs`, which was far over the crate's 1000-line
//! ceiling, when the pickle's module key had to be added to `Ctx` beside it.

use super::{Ctx, inline, receiver, types};
use crate::syntax::Stmt;

/// The facts that are about the MODULE rather than about one body.
///
/// One function because there are two doors — a script through
/// [`super::emit_program_with_exports`] and a graph through `emit_unit` — and a setup
/// written at one of them is a setup the other silently does without. That is
/// not hypothetical: `class_fields` stood at the first door alone, so every
/// program compiled as a graph was emitted without it, and the receiver
/// analysis was written the same way — it answered under `rts run` and
/// answered nothing under `rts ir`, which is how the divergence was found.
/// `program` is every unit of the compilation, concatenated; `body` is the one
/// being emitted. At the script door they are the same slice.
///
/// The two are separate because the answers below are about the PROGRAM and one
/// of them was measurably not: `receiver::resolve` asks "is `C` ever read as a
/// value", and asked of one unit it answered yes for a class an IMPORTER writes
/// through. `C.prototype.m = f` in the importing module left the exporting
/// module deciding `o.m`, and the call answered 1 where node answers 2 — while
/// the identical program in ONE file answers 2, because there the write is a
/// value read the proof sees.
///
/// What stays per BODY is `inline::candidates`, and the reason is that a count
/// cannot cross a module: `graph::front_end` parses every file into one
/// `Names`, so two modules that each declare `function helper` share one `Name`
/// and a program-wide `declarations_of` would count 2 — both losing a
/// substitution they have today. `receiver::resolve` may go program-wide
/// precisely because its counts GATE rather than select: counting more refuses
/// more, which is the safe direction.
pub(super) fn whole_program_facts(program: &[Stmt], ctx: &mut Ctx) {
    let body = program;
    ctx.class_fields = types::declared(body);
    // WHICH `o.m` THE PROGRAM ALREADY DECIDES. Whole-program and once, for the
    // same reason the line above is: every clause it rests on — what `C` is,
    // whether anything writes through it, whether `o` is ever read as a value —
    // is about the module rather than about one body.
    //
    // A method call is one runtime crossing, measured at 19.00 ns against 6.00
    // for the property read alone and 1.00 for a substituted call, so what this
    // removes is the crossing rather than anything about being a method.
    let length_name = ctx.names.intern("length");
    let eval_name = ctx.names.intern("eval");
    let global_this_name = ctx.names.intern("globalThis");
    let arguments_name = ctx.names.intern("arguments");
    ctx.static_methods = receiver::resolve(body, ctx.names.intern("constructor"))
        .methods
        .into_iter()
        .filter_map(|((held, method), function)| {
            // `this_ok` is TRUE here and nowhere else: the receiver was proved,
            // so a body reading `this` has an answer at every site this
            // candidate serves.
            let (mut built, free) =
                inline::local_candidate(&function, length_name, method, arguments_name, true)?;
            // THE ORDINARY WHOLE-PROGRAM PROOF, because a static method has no
            // locality argument to offer: its body is emitted in the caller's
            // scope like any other, so a free name it reads must mean the same
            // thing there. `omit` is the only door that may skip this, and it
            // skips it by proving something stronger.
            built.free_proved =
                inline::free_names_proved(body, &free, eval_name, global_this_name, arguments_name);
            if !built.free_proved {
                return None;
            }
            Some(((held, method), std::rc::Rc::new(built)))
        })
        .collect();
}
