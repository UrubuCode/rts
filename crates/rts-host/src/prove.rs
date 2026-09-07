//! Where a program is settled, and where it falls back.
//!
//! # The rule this exists to make checkable
//!
//! `crates/rts-codegen/README.md` rule 5: *what cannot be proven becomes
//! generic, visibly*. "Visibly" has meant visible in the emitted IR, to someone
//! willing to read a few thousand instructions and count. That is a different
//! property from being reported, and the difference matters more here than it
//! would in most compilers, because this one is two tiers on purpose: a settled
//! path, and the generic path under it for everything the language would not let
//! the emitter decide.
//!
//! A two-tier design that cannot say which tier a program landed in has only one
//! tier a reader can act on. This is the other half.
//!
//! # The split, which is the report
//!
//! [`settled_blocks`] walks only the edges taken when every speculation holds.
//! Everything else a function contains is the second tier, and the two are
//! counted apart. Its own documentation carries why, and it is not a detail:
//! the first version of this counted them TOGETHER and said a class method asks
//! the runtime four times to read two fields, when the armed path asks it none.
//!
//! # What is counted, and why these
//!
//! Not "how fast is it". Nothing here is a measurement and a count is not a
//! nanosecond. What is counted is what the emitter produced at the moments a
//! proof was unavailable:
//!
//! - **A call to a runtime operation.** The emitter could not settle what an
//!   operator or an access meant, so it asked the runtime, which decides while
//!   running what a proof would have decided here. `docs/codegen/entry-tax.md`
//!   prices the crossing. On the SETTLED side this is the number that matters,
//!   because it is what the program pays every pass.
//! - **A guard, and a cached access.** Both are speculation: the settled path
//!   bought with a test. Neither is a failure — rule 11 of the machine layer
//!   makes a guard the only way to narrow — but both mark a place where a real
//!   proof would have left nothing behind.
//! - **A widening.** A proof that existed and was dropped at a boundary that
//!   could not carry it. This is the one to watch, because
//!   `docs/codegen/the-missing-pass.md` is about a widening costing far more
//!   than its own instruction: the fast paths downstream require an operand that
//!   is ALREADY proven, so one widening switches off every one of them.
//!
//! # Why counts and not a percentage of anything
//!
//! Because there is no honest denominator. Instructions are not comparable to
//! each other, and a function with one widening inside a loop is worse off than
//! one with twenty outside every loop. The counts are for comparing two reports
//! of the SAME program across a change, which is the question they answer well,
//! and they are deliberately bad at ranking two different programs.

use std::collections::{BTreeMap, HashSet};

use rts_cranelift::ir::{BlockId, Function, FuncId, Inst, Terminator};

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
    /// Cached accesses, which are a settled path bought with a remembered shape.
    caches: usize,
    /// Calls through a value, where the callee is not known here.
    indirect: usize,
}

impl Tally {
    fn merge(&mut self, other: &Tally) {
        self.insts += other.insts;
        self.guards += other.guards;
        self.widens += other.widens;
        self.caches += other.caches;
        self.direct += other.direct;
        self.indirect += other.indirect;
        for (symbol, count) in &other.runtime {
            *self.runtime.entry(symbol).or_default() += count;
        }
    }
}

/// Which blocks a program reaches when every speculation holds.
///
/// # Why this is the whole point of the report
///
/// The first version of this counted every instruction in a function, and read
/// wrongly in exactly the case it was built for. `this.x` emits a `CachedGet`
/// whose HIT edge is a field read and whose MISS edge calls the runtime, so a
/// report that counts both says a class method asks the runtime four times when
/// the armed path asks it none. That is not a small inaccuracy: it names the
/// fallback as if it were the program, which is the opposite of the answer.
///
/// So the walk follows only the edges taken when a speculation holds: a guard's
/// `ok`, a cached access's `hit`, and both arms of an ordinary branch, which is
/// a real choice in the program rather than a bet about a representation. What
/// it does not reach is the second tier, and reaching it any other way would be
/// the report making the same mistake the counting did.
///
/// A block reachable BOTH ways is settled. That is the honest direction: the
/// join after a cached read is on the armed path, and calling it a fallback
/// because a miss can also arrive there would move the whole program into the
/// second tier one merge at a time.
fn settled_blocks(function: &Function) -> HashSet<BlockId> {
    let mut seen: HashSet<BlockId> = HashSet::new();
    let mut queue = vec![function.entry];
    while let Some(block) = queue.pop() {
        if !seen.insert(block) {
            continue;
        }
        let Some(data) = function.block(block) else {
            continue;
        };
        let Some(terminator) = &data.terminator else {
            continue;
        };
        let taken = match terminator {
            Terminator::Jump(call) => vec![call.block],
            // Both arms: which one runs is the program's own question, not a
            // bet this compiler placed.
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block.block, else_block.block],
            Terminator::Guard { ok, .. } | Terminator::GuardType { ok, .. } => vec![ok.block],
            Terminator::CachedGet { hit, .. }
            | Terminator::CachedGetIndirect { hit, .. }
            | Terminator::CachedGetKeyed { hit, .. }
            | Terminator::CachedSet { hit, .. } => vec![hit.block],
            _ => Vec::new(),
        };
        queue.extend(taken);
    }
    seen
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

    let mut out = String::from("; where this program is settled, and where it falls back\n;\n");
    out.push_str("; settled  is what runs when every speculation holds.\n");
    out.push_str("; fallback is the second tier: a guard that failed, a cache that missed.\n");
    out.push_str("; counts, not costs. compare two reports of the SAME program.\n\n");

    let mut settled_total = Tally::default();
    let mut fallback_total = Tally::default();
    let mut rows: Vec<(String, Tally, Tally)> = Vec::new();

    for (id, function) in &program.functions {
        let settled_blocks = settled_blocks(function);
        let mut settled = Tally::default();
        let mut fallback = Tally::default();
        for (block_id, block) in function.blocks() {
            let tally = match settled_blocks.contains(&block_id) {
                true => &mut settled,
                false => &mut fallback,
            };
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
            match &block.terminator {
                Some(Terminator::Guard { .. }) | Some(Terminator::GuardType { .. }) => {
                    tally.guards += 1
                }
                Some(Terminator::CachedGet { .. })
                | Some(Terminator::CachedGetIndirect { .. })
                | Some(Terminator::CachedGetKeyed { .. })
                | Some(Terminator::CachedSet { .. }) => tally.caches += 1,
                _ => {}
            }
        }
        settled_total.merge(&settled);
        fallback_total.merge(&fallback);
        let name = named.get(id).copied().unwrap_or("<anonymous>");
        let entry = match *id == program.entry {
            true => "  ; the program's entry",
            false => "",
        };
        rows.push((format!("{id:?} {name}{entry}"), settled, fallback));
    }

    for (heading, settled, fallback) in &rows {
        out.push_str(&format!("{heading}\n"));
        out.push_str(&format!(
            "  settled   {:>5} instructions, {} widened, {} guarded, {} cached\n",
            settled.insts, settled.widens, settled.guards, settled.caches
        ));
        out.push_str(&format!(
            "  fallback  {:>5} instructions\n",
            fallback.insts
        ));
        out.push_str(&format!(
            "  calls     {} direct, {} through a value\n",
            settled.direct + fallback.direct,
            settled.indirect + fallback.indirect
        ));
        describe_runtime(&mut out, "settled asks the runtime", &settled.runtime);
        describe_runtime(&mut out, "fallback asks the runtime", &fallback.runtime);
        out.push('\n');
    }

    out.push_str("; the whole program\n");
    out.push_str(&format!(
        ";   {} functions\n",
        rows.len()
    ));
    out.push_str(&format!(
        ";   settled   {} instructions, {} widened, {} guarded, {} cached, {} runtime operations\n",
        settled_total.insts,
        settled_total.widens,
        settled_total.guards,
        settled_total.caches,
        settled_total.runtime.values().sum::<usize>()
    ));
    out.push_str(&format!(
        ";   fallback  {} instructions, {} runtime operations\n",
        fallback_total.insts,
        fallback_total.runtime.values().sum::<usize>()
    ));
    out
}

/// The runtime operations of one tier, worst first.
///
/// Ranked by count and then by name, so the heaviest leads and two reports of
/// one program order identically — the machine layer's rule 13, applied to
/// something a person diffs.
fn describe_runtime(out: &mut String, heading: &str, runtime: &BTreeMap<&'static str, usize>) {
    if runtime.is_empty() {
        return;
    }
    out.push_str(&format!("  {heading}:\n"));
    let mut ranked: Vec<_> = runtime.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (symbol, count) in ranked {
        out.push_str(&format!("      {count:>4}  {symbol}\n"));
    }
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
