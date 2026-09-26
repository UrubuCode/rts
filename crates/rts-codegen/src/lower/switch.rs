//! `switch`, which is a chain of comparisons, one shared scope, and a merge.
//!
//! # What makes it more than an `if` chain
//!
//! Three things, and each is a semantic the tree's own comments state:
//!
//! 1. **One scope for every clause.** A `let` in one case is visible from the others,
//!    which is why the tree keeps the clauses flat instead of giving each a body that
//!    looks like a block.
//! 2. **Fall-through.** A clause that does not `break` continues into the NEXT
//!    clause's statements, so the blocks are chained rather than each jumping to the
//!    exit.
//! 3. **`default` keeps its position.** It is matched last and executed where it
//!    sits, so falling into it and out of it follows the written order.
//!
//! # Why the tests come first, all of them
//!
//! Every clause's test is evaluated in order until one matches, and only then does
//! any body run. So the chain is built as a run of comparison blocks that branch into
//! the bodies — not as a test beside each body, which would run the first body before
//! the second test.
//!
//! `default` is skipped by that chain and reached only when every test failed, which
//! is what makes "matched last" true whatever position it holds.
//!
//! # The merge, and the wrong answer that came of skipping it
//!
//! The first version of this file carried NO bindings, reasoning that a switch has no
//! back edge. It has no back edge and it does have several paths into one exit, which
//! is a different question — and the graph said so at once:
//!
//! ```text
//! b2:
//!   ; from b1, b5
//!   v8 = add(v6, v7)     ← v6 is defined in b1 only
//! b4:
//!   return v9            ← v9 is the default clause's value, on every path
//! ```
//!
//! So the exit takes a parameter per binding any clause assigns, every path into it
//! supplies what it holds, and a body reached by fall-through starts from its own
//! parameters rather than from what the previous clause left in the map. That is the
//! `if` join's mechanism with more than two predecessors.
//!
//! **`rts_mir::verify` did not catch the first version**, and its own header says why:
//! within a block it checks order, and across blocks only existence. Dominance is the
//! real rule and it is the check to add when a pass reorders blocks — this is the
//! first thing that would have been caught by it.

use rts_mir::Domain;
use rts_mir::cfg::{BlockId, Terminator, ValueId};

use super::{FrameKind, LoopFrame, Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::names::resolve::BindingId;
use crate::names::Name;
use crate::syntax::{Expr, Stmt, StmtKind, SwitchClause};

impl Lowering<'_> {
    /// Lowers a `switch`.
    pub(super) fn switch(
        &mut self,
        discriminant: &Expr,
        clauses: &[SwitchClause],
    ) -> Result<bool, Unsupported> {
        let subject = self.expression(discriminant)?;

        // WHAT THE CLAUSES DISAGREE ABOUT, before any of them is lowered — for the
        // reason a loop needs it early: a block's parameters must be declared before
        // anything jumps to it, and every clause jumps to the exit.
        let mut assigned = std::collections::BTreeSet::new();
        for clause in clauses {
            for statement in &clause.body {
                assigned.extend(self.assigned_in(statement)?);
            }
        }
        let carried: Vec<BindingId> = self.carried_now(assigned);

        // One block per clause BODY, each taking the carried bindings: a body is
        // reached from its own test AND from the clause before it falling through, so
        // it is a merge point like any other.
        let bodies: Vec<BlockId> = clauses.iter().map(|_| self.builder.block()).collect();
        let exit = self.builder.block();
        let mut body_params: Vec<Vec<ValueId>> = Vec::with_capacity(bodies.len());
        for block in &bodies {
            let mut params = Vec::with_capacity(carried.len());
            for _ in &carried {
                let param = self.builder.param(*block);
                self.types.insert(param, self.domain.top());
                params.push(param);
            }
            body_params.push(params);
        }
        let exiting: Vec<ValueId> = carried
            .iter()
            .map(|_| {
                let param = self.builder.param(exit);
                self.types.insert(param, self.domain.top());
                param
            })
            .collect();

        // The values as they stand before any clause runs. Every edge out of the test
        // chain carries these, because no body has run yet.
        let entering: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
        // EACH CLAUSE STARTS FROM WHAT HELD BEFORE THE SWITCH, and not from what the
        // clause lowered before it left behind: a clause is reached from its own test,
        // where nothing an earlier clause bound exists.
        let outside = self.values.clone();

        for (at_clause, clause) in clauses.iter().enumerate() {
            let Some(test) = &clause.test else {
                continue;
            };
            let held = self.expression(test)?;
            // `switch` compares with `===`, which coerces nothing — so a clause test
            // cannot run a getter, and the order of the tests is the only thing
            // observable about them.
            let matched = self.prim(JsPrim::StrictEquals, vec![subject, held], test);
            let carry_on = self.builder.block();
            self.builder.end(Terminator::Branch {
                condition: matched,
                then_block: bodies[at_clause],
                then_args: entering.clone(),
                else_block: carry_on,
                else_args: Vec::new(),
            });
            self.builder.switch_to(carry_on);
        }
        // Every test failed: the `default` if there is one, and the exit if not.
        let fell_off = clauses
            .iter()
            .position(|clause| clause.test.is_none())
            .map(|at_clause| bodies[at_clause]);
        self.builder.end(Terminator::Jump {
            target: fell_off.unwrap_or(exit),
            args: entering.clone(),
        });

        // THE BODIES, chained. A `break` inside one leaves through the exit, which is
        // the same question a loop asks — so it uses the same frame, with the carried
        // set the exit declares.
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Switch,
            header: exit,
            exit,
            carried: carried.clone(),
        });
        let mut lowered = Ok(());
        for at_clause in 0..clauses.len() {
            if lowered.is_err() {
                break;
            }
            self.builder.switch_to(bodies[at_clause]);
            self.values = outside.clone();
            for (binding, param) in carried.iter().zip(&body_params[at_clause]) {
                self.values.insert(*binding, *param);
            }
            lowered = self.statements(&clauses[at_clause].body).map(|ended| {
                if !ended {
                    // FALL THROUGH into the next clause, or out — the whole of what
                    // makes a switch not an if-chain, and why the bodies were created
                    // before any of them was filled.
                    let args: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
                    let next = bodies.get(at_clause + 1).copied().unwrap_or(exit);
                    self.builder.end(Terminator::Jump { target: next, args });
                }
            });
        }
        self.loops.pop();
        lowered?;

        self.builder.switch_to(exit);
        for (binding, param) in carried.iter().zip(&exiting) {
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }

    /// `L: statement`. On a loop or a switch the label names the frame that
    /// statement pushes, so it is handed on. On anything else it names a block `break
    /// L` can leave: a frame of the switch kind -- `continue` passes through it --
    /// whose exit takes what the block assigned, as a switch's does.
    pub(super) fn labelled(&mut self, label: Name, body: &Stmt) -> Result<bool, Unsupported> {
        if matches!(
            body.kind,
            StmtKind::While { .. }
                | StmtKind::DoWhile { .. }
                | StmtKind::For { .. }
                | StmtKind::ForEach { .. }
                | StmtKind::Switch { .. }
                | StmtKind::Labelled { .. }
        ) {
            self.pending_labels.push(label);
            let lowered = self.statement(body);
            self.pending_labels.clear();
            return lowered;
        }
        let mut labels = std::mem::take(&mut self.pending_labels);
        labels.push(label);
        let assigned = self.assigned_in(body)?;
        let carried: Vec<BindingId> = self.carried_now(assigned);
        let exit = self.builder.block();
        let exiting: Vec<ValueId> = carried
            .iter()
            .map(|_| {
                let param = self.builder.param(exit);
                self.types.insert(param, self.domain.top());
                param
            })
            .collect();
        self.loops.push(LoopFrame {
            labels,
            kind: FrameKind::Switch,
            header: exit,
            exit,
            carried: carried.clone(),
        });
        let lowered = self.statement(body);
        self.loops.pop();
        if !lowered? {
            let args: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump { target: exit, args });
        }
        self.builder.switch_to(exit);
        for (binding, param) in carried.iter().zip(&exiting) {
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }
}
