//! The program as text, compiled but never placed.
//!
//! `rts ir` is how an optimization is checked: emit, read what came out, change
//! the emitter, read it again. That loop did not exist for this engine — the
//! only IR dump in the repository belonged to the old one, over a different
//! representation — so every claim about what this engine emits was made by
//! reading the emitter rather than its output.
//!
//! # Where the work is
//!
//! Almost nowhere, which is the point. `rts_cranelift::ir::text` renders a
//! function; this names the functions, because the machine may not know what a
//! name is (its rule 2) and `rts_codegen::emit::Program` carries them. Nothing
//! here decides anything about the program — rule 1 of this crate.
//!
//! # Why it stops before placement
//!
//! A dump is asked for when something is wrong, and what is wrong is often what
//! placement would refuse. Verifying or lowering first would answer with the
//! refusal instead of with the IR that caused it, which is the one thing the
//! caller already knows.

use crate::link::HostError;
use crate::run::{FrontEnd, front_end};

/// The IR of one source text, as text.
pub fn describe_source(source: &str) -> Result<String, HostError> {
    Ok(render(&front_end(source)?))
}

/// The IR of a file and everything it imports, as text.
///
/// The whole graph, not the entry alone: a name bound in another module is
/// emitted in that module's function, so a dump of the entry by itself shows a
/// call to something with no body — which reads as a missing feature and is not
/// one. `suite_run` learned the same lesson the expensive way (see `CLAUDE.md`).
pub fn describe_path(entry: &std::path::Path) -> Result<String, HostError> {
    let (front, _) = crate::graph::front_end(entry)?;
    Ok(render(&front))
}

/// One function per stanza, in the order they were emitted.
///
/// Order is the emitter's, not a sort by name: two dumps taken across an
/// emitter change should differ where the emission differs, and sorting would
/// move unrelated functions every time one is added.
fn render(front: &FrontEnd) -> String {
    let program = &front.emitted;
    let named: std::collections::HashMap<_, _> = program
        .function_names
        .iter()
        .map(|(id, name, _)| (*id, name.as_str()))
        .collect();

    let mut out = legend(front, &named);
    for (id, function) in &program.functions {
        let called = named.get(id).copied().unwrap_or("<anonymous>");
        let entry = match *id == program.entry {
            true => "  ; the program's entry",
            false => "",
        };
        let generator = match program.generators.contains(id) {
            true => "  ; a generator body, rewritten before placement",
            false => "",
        };
        out.push_str(&format!("; {id:?} {called}{entry}{generator}\n"));
        out.push_str(&function.to_string());
        out.push('\n');
    }
    out
}

/// What every callee a body names actually is, once, at the top.
///
/// A call prints as `Call { callee: FuncId(2), … }` and stops being readable
/// there. The machine cannot help: a function identifier is a number in a
/// registry that records no names, which is its rule 2 and is right — the two
/// things that DO know are the language layer's `RuntimeCalls` (a runtime
/// operation's symbol) and the emitter's own name list.
///
/// A legend rather than substitution inside each instruction: substituting
/// means a match arm per instruction that carries a callee, which is the second
/// statement of the vocabulary `rts_cranelift::ir::text` explains it refuses to
/// write. Ordered by identifier so two dumps diff cleanly (the machine's rule
/// 13).
fn legend(
    front: &FrontEnd,
    named: &std::collections::HashMap<rts_cranelift::ir::FuncId, &str>,
) -> String {
    let mut rows: Vec<(rts_cranelift::ir::FuncId, String)> = front
        .calls
        .declared()
        .map(|(op, id)| (id, op.symbol().to_owned()))
        .collect();
    rows.extend(
        front
            .emitted
            .functions
            .iter()
            .map(|(id, _)| (*id, named.get(id).copied().unwrap_or("<anonymous>").to_owned())),
    );
    rows.sort_by_key(|(id, _)| id.index());

    let mut out = String::from("; callees\n");
    for (id, name) in rows {
        out.push_str(&format!(";   {id:?} {name}\n"));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dump_shows_the_entry_and_does_not_run_the_program() {
        // If this ran, the `console.log` would print. It states the pairing the
        // command relies on: text out, no execution.
        let text = describe_source("console.log(1 + 2)").expect("a program that compiles");
        assert!(
            text.contains("the program's entry"),
            "the dump should say which function the program starts at; got:\n{text}"
        );
        assert!(
            text.contains("block0"),
            "the dump should contain a function body; got:\n{text}"
        );
    }

    #[test]
    fn a_program_that_does_not_compile_reports_why_instead_of_dumping() {
        let refused = describe_source("let = = =");
        assert!(
            refused.is_err(),
            "a dump of a program that does not parse is not a dump"
        );
    }
}
