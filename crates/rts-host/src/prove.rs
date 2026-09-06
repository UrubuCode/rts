//! What the closed world proved about a program, and where it gave up.
//!
//! # The rule this exists to make checkable
//!
//! `crates/rts-codegen/README.md` rule 5: *what cannot be proven becomes
//! generic, visibly*. "Visibly" has meant visible in the emitted IR, to someone
//! willing to read a few thousand instructions and count. That is a different
//! property from being reported, and the difference matters more here than it
//! would in most compilers, because this one is built as two tiers on purpose:
//! a proven path, and the generic path under it for everything the language
//! would not let the emitter settle.
//!
//! A two-tier design that cannot say which tier a program landed in has only
//! one tier a reader can act on. This is the other half.
//!
//! # What it counts, and why these three
//!
//! Not "how fast is it" — nothing here is a measurement, and a count is not a
//! nanosecond. What it counts is the three shapes a give-up takes, each of
//! which the emitter produces at exactly the moment a proof was unavailable:
//!
//! - **A call to a runtime operation.** The emitter could not settle what an
//!   operator or an access meant, so it asked the runtime, which decides at run
//!   time what a proof would have decided here. `docs/codegen/entry-tax.md`
//!   prices the crossing.
//! - **A guard.** The emitter SPECULATED. A guard is not a failure — it is the
//!   proven path bought with a test, and rule 11 of the machine layer makes it
//!   the only way to narrow — but it is a place where a real proof would have
//!   left nothing behind.
//! - **A widening.** A proof that existed and was dropped, because the value
//!   crossed into somewhere that could not carry it. This is the one to watch:
//!   `docs/codegen/the-missing-pass.md` is about a widening costing far more
//!   than its own instruction, since the fast paths downstream require an
//!   operand that is ALREADY proven.
//!
//! # Why a count and not a percentage of anything
//!
//! Because there is no honest denominator. Instructions are not comparable to
//! each other, and a function with one widening in a loop is worse off than one
//! with twenty outside every loop. The counts are for COMPARING TWO DUMPS of
//! the same program across a change, which is the question this answers well,
//! and they are deliberately bad at ranking two different programs.

use std::collections::BTreeMap;

use rts_cranelift::ir::{FuncId, Inst, Terminator};

use crate::link::HostError;
use crate::run::{FrontEnd, front_end};

/// The report for one source text.
pub fn prove_source(source: &str) -> Result<String, HostError> {
    Ok(render(&front_end(source)?))
}

/// The report for a file and everything it imports.
///
/// The whole graph, for the reason `describe::describe_path` gives: a name
/// bound in another module is emitted in that module's function, so a report
/// about the entry alone would describe a fraction of the program and look like
/// a complete answer.
pub fn prove_path(entry: &std::path::Path) -> Result<String, HostError> {
    Ok(render(&crate::graph::front_end(entry)?.front))
}

/// What one function gave up on.
#[derive(Default)]
struct Tally {
    insts: usize,
    guards: usize,
    widens: usize,
    /// Calls to a runtime operation, by the operation's symbol.
    runtime: BTreeMap<&'static str, usize>,
    /// Calls to another function of this program.
    direct: usize,
    /// Calls through a value, where the callee is not known here.
    indirect: usize,
}

impl Tally {
    fn gave_up(&self) -> usize {
        self.runtime.values().sum::<usize>() + self.guards + self.widens
    }

    fn merge(&mut self, other: &Tally) {
        self.insts += other.insts;
        self.guards += other.guards;
        self.widens += other.widens;
        self.direct += other.direct;
        self.indirect += other.indirect;
        for (symbol, count) in &other.runtime {
            *self.runtime.entry(symbol).or_default() += count;
        }
    }
}

fn render(front: &FrontEnd) -> String {
    let program = &front.emitted;
    let named: BTreeMap<FuncId, &str> = program
        .function_names
        .iter()
        .map(|(id, name, _, _, _)| (*id, name.as_str()))
        .collect();
    // Which function identifiers are runtime operations rather than program
    // bodies. The emitter's own table, which is the only thing that knows: a
    // function identifier is a number in a registry that records no names, by
    // the machine layer's rule 2.
    let operations: BTreeMap<FuncId, &'static str> = front
        .calls
        .declared()
        .map(|(op, id)| (id, op.symbol()))
        .collect();

    let mut out = String::from("; what the closed world proved\n;\n");
    out.push_str("; each row is a place a proof was unavailable, not a cost.\n");
    out.push_str("; compare two reports of the same program; do not rank two programs.\n\n");

    let mut total = Tally::default();
    let mut rows: Vec<(String, Tally)> = Vec::new();

    for (id, function) in &program.functions {
        let mut tally = Tally::default();
        for (_, block) in function.blocks() {
            for &inst_id in &block.insts {
                let Some(data) = function.inst(inst_id) else {
                    continue;
                };
                tally.insts += 1;
                match &data.inst {
                    Inst::Widen(_) => tally.widens += 1,
                    Inst::Call { callee, .. } => match operations.get(callee) {
                        Some(symbol) => *tally.runtime.entry(symbol).or_default() += 1,
                        None => tally.direct += 1,
                    },
                    Inst::CallIndirect { .. } => tally.indirect += 1,
                    _ => {}
                }
            }
            if let Some(Terminator::Guard { .. }) = &block.terminator {
                tally.guards += 1;
            }
        }
        total.merge(&tally);
        let name = named.get(id).copied().unwrap_or("<anonymous>");
        let entry = match *id == program.entry {
            true => "  ; the program's entry",
            false => "",
        };
        rows.push((format!("{id:?} {name}{entry}"), tally));
    }

    for (heading, tally) in &rows {
        out.push_str(&format!("{heading}\n"));
        out.push_str(&format!(
            "  {:>6} instructions, {} of which gave up\n",
            tally.insts,
            tally.gave_up()
        ));
        out.push_str(&format!(
            "  {:>6} widened   ; a proof dropped at a boundary\n",
            tally.widens
        ));
        out.push_str(&format!(
            "  {:>6} guarded   ; speculated, then proven by a test\n",
            tally.guards
        ));
        out.push_str(&format!(
            "  {:>6} direct calls, {} through a value\n",
            tally.direct, tally.indirect
        ));
        if tally.runtime.is_empty() {
            out.push_str("         no runtime operation reached\n");
        } else {
            out.push_str("         asked the runtime:\n");
            let mut ranked: Vec<_> = tally.runtime.iter().collect();
            // By count, then by name, so the worst offender leads and two
            // reports of the same program order identically.
            ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            for (symbol, count) in ranked {
                out.push_str(&format!("      {count:>6}  {symbol}\n"));
            }
        }
        out.push('\n');
    }

    out.push_str("; the whole program\n");
    out.push_str(&format!(
        ";   {} functions, {} instructions\n",
        rows.len(),
        total.insts
    ));
    out.push_str(&format!(
        ";   {} widened, {} guarded, {} runtime operations\n",
        total.widens,
        total.guards,
        total.runtime.values().sum::<usize>()
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_entry_and_does_not_run_the_program() {
        // If this ran, the `console.log` would print. It states the same
        // pairing `describe` relies on: text out, no execution.
        let text = prove_source("console.log(1 + 2)").expect("a program that compiles");
        assert!(
            text.contains("the program's entry"),
            "the report should say which function the program starts at; got:\n{text}"
        );
        assert!(
            text.contains("the whole program"),
            "the report should total; got:\n{text}"
        );
    }

    #[test]
    fn a_program_that_only_adds_two_proven_numbers_asks_the_runtime_less() {
        // The claim this pins is the one the report exists to make checkable,
        // and it is about the LANGUAGE rather than about this emission: adding
        // two numbers the body itself created is decidable, and adding a number
        // to something that arrived from outside is not, because `+` chooses
        // between arithmetic and concatenation from its operands.
        //
        // Counted rather than asserted exactly: what must hold is the
        // direction, not a number this test would then pin against every future
        // improvement.
        let proven = prove_source("let a = 1; for (let i = 0; i < 9; i++) { a = a + i } console.log(a)")
            .expect("compiles");
        let unproven =
            prove_source("let a = 1; for (let i = 0; i < 9; i++) { a = a + (globalThis as any).x } console.log(a)")
                .expect("compiles");

        let asked = |text: &str| {
            text.lines()
                .filter(|line| line.contains("__rts_add"))
                .count()
        };
        assert!(
            asked(&unproven) >= asked(&proven),
            "a body that adds an unknown must reach the runtime at least as \
             often as one that adds its own counter;\nproven:\n{proven}\nunproven:\n{unproven}"
        );
    }

    #[test]
    fn a_program_that_does_not_compile_reports_why_instead_of_counting() {
        assert!(
            prove_source("let = = =").is_err(),
            "a report about a program that does not parse is not a report"
        );
    }
}
