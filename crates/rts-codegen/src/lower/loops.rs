//! Loops: the back edge, and the pre-pass a branch does not need.
//!
//! Apart from the rest of the lowering because of the ceiling --  reached
//! 923 lines and the crate holds 1000 -- and the split is along the seam the code
//! already had. A branch computes what to merge by comparing two finished arms; a
//! loop cannot, because the header has a predecessor that does not exist yet. That
//! one difference is the whole of what is in this file.

use std::collections::BTreeSet;

use rts_mir::Domain;
use rts_mir::cfg::{Terminator, ValueId};

use super::{FrameKind, LoopFrame, Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::emit::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::names::Name;
use crate::names::resolve::BindingId;
use crate::syntax::{AssignTarget, Expr, ExprKind, ForInit, FunctionBody, Stmt};

impl Lowering<'_> {
    /// Lowers a `while`, and every loop is this shape once its header is decided.
    ///
    /// # Why the carried set has to be computed BEFORE the body
    ///
    /// The header is a block with a predecessor that does not exist yet — the back
    /// edge — and a block's parameters must be declared before anything jumps to
    /// it. So the question "which bindings does this loop carry" cannot be answered
    /// the way the `if` join answers it, by comparing what two finished arms hold.
    /// It has to be answered from the tree.
    ///
    /// That is the whole structural difference between a branch and a loop, and it
    /// is why one of them needed a pre-pass and the other did not.
    ///
    /// # Why over-approximating is the safe direction
    ///
    /// [`Self::assigned_in`] resolves each assigned name in the scope the loop is
    /// written in, so a name the body shadows resolves to the OUTER binding and the
    /// loop carries one it did not need to. That costs a block parameter and
    /// nothing else: the value passes through unchanged on every edge.
    ///
    /// Under-approximating would be a wrong answer — a binding the body assigns and
    /// the header does not carry would be read across the back edge as the value
    /// from before the loop, for ever.
    ///
    /// # What the types at the header are, and why they are the domain's top
    ///
    /// A header parameter's type is the join of what arrives from before the loop
    /// and what arrives across the back edge, and the second is not known until the
    /// body has been lowered — which needs the parameter to exist. The loop is real
    /// and it is what [`rts_mir::infer`] exists to solve: it iterates to a fixed
    /// point over the finished graph and answers exactly that join.
    ///
    /// So this lowering records `top()` for a header parameter and the effects
    /// decided inside the body are pessimistic in consequence: arithmetic over a
    /// carried value is `CALLS_USER` even where inference will prove it numeric.
    /// That is sound and it is the one place this file knowingly leaves speed on
    /// the table — recovering it is a pass that recomputes effects from inference's
    /// answer, which is a pass over a finished graph and not a second traversal of
    /// the tree.
    pub(super) fn loop_while(
        &mut self,
        condition: &Expr,
        body: &Stmt,
    ) -> Result<bool, Unsupported> {
        let carried = self.carried_now(self.assigned_in(body)?);

        let header = self.builder.block();
        let into_body = self.builder.block();
        let exit = self.builder.block();

        // The values at the top of the loop, in the carried order.
        let entering: Vec<ValueId> = carried.iter().map(|binding| self.values[binding]).collect();
        self.builder.end(Terminator::Jump {
            target: header,
            args: entering,
        });

        self.builder.switch_to(header);
        let mut params = Vec::with_capacity(carried.len());
        for binding in &carried {
            let param = self.builder.param(header);
            self.types.insert(param, self.domain.top());
            self.values.insert(*binding, param);
            params.push(param);
        }
        // The test is in the HEADER, which is what makes a `while` check before
        // each pass including the first, and what makes the value a carried
        // binding holds after the loop be the header's parameter.
        let tested = self.expression(condition)?;
        let tested = self.prim(JsPrim::Truthy, vec![tested], condition);
        let leaving: Vec<ValueId> = params.clone();
        self.builder.end(Terminator::Branch {
            condition: tested,
            then_block: into_body,
            then_args: Vec::new(),
            else_block: exit,
            else_args: leaving,
        });

        self.builder.switch_to(into_body);
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Loop,
            header,
            exit,
            carried: carried.clone(),
        });
        let ended = self.statement(body);
        self.loops.pop();
        let ended = ended?;
        // A body that left through a `return` or a `break` has already terminated
        // its block, so there is no back edge to write from here.
        if !ended {
            let back: Vec<ValueId> = carried.iter().map(|binding| self.values[binding]).collect();
            self.builder.end(Terminator::Jump {
                target: header,
                args: back,
            });
        }

        self.builder.switch_to(exit);
        // After the loop, a carried binding holds what the exit block received --
        // which is the header's parameter, because that is where the test decided
        // to leave.
        let exiting: Vec<ValueId> = carried.iter().map(|_| self.builder.param(exit)).collect();
        for (binding, param) in carried.iter().zip(&exiting) {
            self.types.insert(*param, self.domain.top());
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }

    /// Every binding an assignment in `body` may write.
    ///
    /// Over-approximating on purpose — see [`Self::loop_while`]. It walks through
    /// `emit::capture`'s traversal rather than matching statement kinds here, which
    /// is what keeps a statement added to the tree tomorrow from being silently
    /// skipped: that file's own header records how a second copy of the tree's
    /// shape is how a node comes to be walked by one analysis and missed by the
    /// other.
    pub(super) fn assigned_in(&self, body: &Stmt) -> Result<BTreeSet<BindingId>, Unsupported> {
        let mut names = Vec::new();
        assigned_names_in_statement(body, &mut names);
        let mut found = BTreeSet::new();
        for name in names {
            // A NAME THIS SCOPE DOES NOT RESOLVE IS SKIPPED, and the first version
            // of this refused it as a global instead. That was wrong twice over.
            //
            // It is wrong about what the name is: a `let` inside the body is
            // declared in a scope this one cannot see, so a nested loop's own
            // counter looked like a global and every nested loop was refused.
            //
            // And it is wrong about where the refusal belongs. A name that really
            // is a global has no binding for a block parameter to hold, so there is
            // nothing to carry and skipping it is the correct answer here — and the
            // assignment itself is still refused, loudly, when the body reaches it
            // and asks `bind` for a binding that does not exist. The pre-pass
            // decides what to CARRY; what a program may do is not its question.
            if let Some(binding) = self.resolution.binding_in(self.scope, name) {
                found.insert(binding);
            }
        }
        Ok(found)
    }

    /// Leaves the innermost loop or switch, or skips to a loop's test.
    ///
    /// `break` takes the innermost frame of either kind. `continue` NAMES a loop, so it
    /// walks past any switch between here and one -- a `continue` inside a `switch`
    /// inside a loop takes the loop's next pass, and treating the two stacks as one
    /// would have it leave the loop instead. That is a wrong answer that compiles, and
    /// the graph would look perfectly well formed.
    pub(super) fn jump_out_of_loop(
        &mut self,
        to_header: bool,
        label: Option<crate::names::Name>,
    ) -> Result<bool, Unsupported> {
        let frame = match (to_header, label) {
            // A LABEL names its frame, whichever kind: `break L` leaves it, `continue
            // L` takes that loop's next pass.
            (_, Some(label)) => self
                .loops
                .iter()
                .rev()
                .find(|held| held.labels.contains(&label)),
            (true, None) => self
                .loops
                .iter()
                .rev()
                .find(|held| held.kind == FrameKind::Loop),
            (false, None) => self.loops.last(),
        };
        let Some(frame) = frame else {
            return Err(Unsupported::Statement(
                "a break or continue with nothing to leave",
            ));
        };
        let target = match to_header {
            true => frame.header,
            false => frame.exit,
        };
        let carried = frame.carried.clone();
        let args: Vec<ValueId> = carried.iter().map(|binding| self.values[binding]).collect();
        self.builder.end(Terminator::Jump { target, args });
        Ok(true)
    }

    /// A classic `for`, which is the `while` shape once its head is accounted for.
    ///
    /// # What the head adds, and why it is a scope
    ///
    /// A lexical `init` declares in a scope that WRAPS the loop, so `for (let i …)`
    /// has one binding per pass rather than one for the whole loop — which is what
    /// makes a closure made in the body capture that pass's value. `resolve` opens a
    /// `ForHead` scope for exactly this, and the lowering enters it the way it
    /// enters a block's.
    ///
    /// # Why the update belongs to the carried set
    ///
    /// It runs after the body, before the test, so its writes cross the back edge
    /// like the body's. Both are scanned together — and a test that also writes
    /// (`for (; (i = i + 1) < n; )`) is scanned too, because a write is a write
    /// wherever it is written.
    ///
    /// # Why a missing test is not a missing branch
    ///
    /// `for (;;)` has no test and never leaves on its own, so the header jumps
    /// straight into the body. The exit block still exists and still declares the
    /// carried parameters, because a `break` inside needs somewhere to go — and a
    /// loop with no test and no break is an infinite loop, which is a program a
    /// compiler must be able to emit.
    pub(super) fn loop_for(
        &mut self,
        at: rts_cranelift::fault::Position,
        init: Option<&ForInit>,
        test: Option<&Expr>,
        update: Option<&Expr>,
        body: &Stmt,
    ) -> Result<bool, Unsupported> {
        let Some(head) = self.resolution.head_scope(at) else {
            return Err(Unsupported::NoScope);
        };
        let outer = std::mem::replace(&mut self.scope, head);
        let lowered = self.for_in_head(init, test, update, body);
        self.scope = outer;
        lowered
    }

    /// The same, with the head's scope already entered.
    ///
    /// Apart so that the scope is restored however this answers — a refusal partway
    /// through would otherwise leave the lowering reading the head's bindings for
    /// the rest of the function.
    fn for_in_head(
        &mut self,
        init: Option<&ForInit>,
        test: Option<&Expr>,
        update: Option<&Expr>,
        body: &Stmt,
    ) -> Result<bool, Unsupported> {
        match init {
            Some(ForInit::Declare { kind, bindings }) => {
                // A `var` belongs to the FUNCTION and not the head, and the scope tree
                // already says so: resolved from the head, the name reaches the
                // function's binding. So each is an assignment, and one with no
                // initialiser does nothing, as `var x;` anywhere does.
                let is_var = matches!(kind, crate::syntax::BindingKind::Var);
                for binding in bindings {
                    if is_var && binding.value.is_none() {
                        continue;
                    }
                    let Some(value) = &binding.value else {
                        return Err(Unsupported::Statement(
                            "a loop head declaration with no initialiser",
                        ));
                    };
                    let held = self.expression(value)?;
                    self.destructure(&binding.target, held, value)?;
                }
            }
            Some(ForInit::Expr(expr)) => {
                self.expression(expr)?;
            }
            None => {}
        }

        let mut carried: BTreeSet<BindingId> = self.assigned_in(body)?;
        if let Some(expr) = update {
            carried.extend(self.assigned_in_expr(expr)?);
        }
        if let Some(expr) = test {
            carried.extend(self.assigned_in_expr(expr)?);
        }
        let carried = self.carried_now(carried);

        let header = self.builder.block();
        let into_body = self.builder.block();
        let exit = self.builder.block();

        let entering: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
        self.builder.end(Terminator::Jump {
            target: header,
            args: entering,
        });

        self.builder.switch_to(header);
        let mut params = Vec::with_capacity(carried.len());
        for binding in &carried {
            let param = self.builder.param(header);
            self.types.insert(param, self.domain.top());
            self.values.insert(*binding, param);
            params.push(param);
        }
        match test {
            Some(expr) => {
                let tested = self.expression(expr)?;
                let tested = self.prim(JsPrim::Truthy, vec![tested], expr);
                self.builder.end(Terminator::Branch {
                    condition: tested,
                    then_block: into_body,
                    then_args: Vec::new(),
                    else_block: exit,
                    else_args: params.clone(),
                });
            }
            // No test: the header goes straight in, and the exit is reached only by
            // a `break`.
            None => self.builder.end(Terminator::Jump {
                target: into_body,
                args: Vec::new(),
            }),
        }

        // THE UPDATE HAS A BLOCK OF ITS OWN, which is where `continue` goes: it runs
        // after every pass, the ones a `continue` cut short included. `continue` went to
        // the header, and `for (let i = 0; i < n; i++) { if (odd(i)) continue; … }`
        // never incremented on those passes -- a loop that never ends, in a graph that
        // looked well formed. Without an update the header is the step.
        let step = match update {
            Some(_) => self.builder.block(),
            None => header,
        };
        self.builder.switch_to(into_body);
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Loop,
            header: step,
            exit,
            carried: carried.clone(),
        });
        let ended = self.statement(body);
        self.loops.pop();
        let ended = ended?;
        if !ended {
            let back: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump {
                target: step,
                args: back,
            });
        }
        if let Some(expr) = update {
            self.builder.switch_to(step);
            for binding in &carried {
                let param = self.builder.param(step);
                self.types.insert(param, self.domain.top());
                self.values.insert(*binding, param);
            }
            self.expression(expr)?;
            let back: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump {
                target: header,
                args: back,
            });
        }

        self.builder.switch_to(exit);
        let exiting: Vec<ValueId> = carried.iter().map(|_| self.builder.param(exit)).collect();
        for (binding, param) in carried.iter().zip(&exiting) {
            self.types.insert(*param, self.domain.top());
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }

    /// A `do`-`while`, which differs from a `while` in one edge.
    ///
    /// The body runs before the test, so the entry jumps into the BODY rather than
    /// into the header — and the header is then only reached from the body's end.
    /// Everything else is the same, which is why this is eight lines and not a
    /// second loop lowering.
    pub(super) fn loop_do_while(
        &mut self,
        body: &Stmt,
        condition: &Expr,
    ) -> Result<bool, Unsupported> {
        let carried = self.carried_now(self.assigned_in(body)?);

        let into_body = self.builder.block();
        let header = self.builder.block();
        let exit = self.builder.block();

        let entering: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
        // INTO THE BODY, which is the whole difference. The body's own block takes
        // the carried parameters here, because it is the merge point: the entry and
        // the header both arrive at it.
        self.builder.end(Terminator::Jump {
            target: into_body,
            args: entering,
        });

        self.builder.switch_to(into_body);
        let mut params = Vec::with_capacity(carried.len());
        for binding in &carried {
            let param = self.builder.param(into_body);
            self.types.insert(param, self.domain.top());
            self.values.insert(*binding, param);
            params.push(param);
        }
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Loop,
            header,
            exit,
            carried: carried.clone(),
        });
        let ended = self.statement(body);
        self.loops.pop();
        let ended = ended?;
        if !ended {
            let after: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump {
                target: header,
                args: after,
            });
        }

        self.builder.switch_to(header);
        let held: Vec<ValueId> = carried.iter().map(|_| self.builder.param(header)).collect();
        for (binding, param) in carried.iter().zip(&held) {
            self.types.insert(*param, self.domain.top());
            self.values.insert(*binding, *param);
        }
        let tested = self.expression(condition)?;
        let tested = self.prim(JsPrim::Truthy, vec![tested], condition);
        self.builder.end(Terminator::Branch {
            condition: tested,
            then_block: into_body,
            then_args: held.clone(),
            else_block: exit,
            else_args: held,
        });

        self.builder.switch_to(exit);
        let exiting: Vec<ValueId> = carried.iter().map(|_| self.builder.param(exit)).collect();
        for (binding, param) in carried.iter().zip(&exiting) {
            self.types.insert(*param, self.domain.top());
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }

    /// The carried set, reduced to the bindings that actually hold a value here.
    ///
    /// # The crash this exists to have prevented
    ///
    /// Without it, `self.values[binding]` panicked with "no entry found for key" on
    /// `bench/analytic.ts` and `bench/pi_machin.ts` — and the survey that ran
    /// afterwards read 3 of 37 where the corpus holds 386 functions, because a
    /// process that dies writes no line at all. **A denominator that falls by ten
    /// times is not a result, it is a crash**, which is why that reading was thrown
    /// away rather than recorded.
    ///
    /// The refusal is not the fix either. A binding the pre-pass found but that
    /// holds nothing here is one of two things, and neither is carried:
    ///
    /// - declared INSIDE the body, so it is fresh on every pass and nothing crosses
    ///   the back edge;
    /// - declared after the loop, so reading it here is its temporal dead zone —
    ///   which the body's own read refuses, by name, where it happens.
    ///
    /// The pre-pass over-approximates on purpose (see [`Self::loop_for`]), and this
    /// is where the over-approximation is taken back. Filtering is safe in the
    /// direction that matters: a binding that IS live and assigned is bound here, so
    /// it survives the filter.
    pub(super) fn carried_now(&self, found: BTreeSet<BindingId>) -> Vec<BindingId> {
        found
            .into_iter()
            .filter(|held| self.values.contains_key(held))
            .collect()
    }

    /// The bindings an expression assigns, for a loop head's update and test.
    pub(super) fn assigned_in_expr(&self, expr: &Expr) -> Result<BTreeSet<BindingId>, Unsupported> {
        let mut names = Vec::new();
        assigned_names_in_expr(expr, &mut names);
        let mut found = BTreeSet::new();
        for name in names {
            if let Some(binding) = self.resolution.binding_in(self.scope, name) {
                found.insert(binding);
            }
        }
        Ok(found)
    }
}

/// Every name an assignment or an increment in this statement writes, at any
/// depth, including inside a nested function.
///
/// A nested function is descended into deliberately. It cannot be lowered here
/// yet, so a body containing one is refused before this matters — but the day one
/// is, a closure that assigns an outer name across the back edge is exactly the
/// case a carried set must not miss, and a traversal that stopped at the function
/// boundary would miss it silently.
fn assigned_names_in_statement(statement: &Stmt, found: &mut Vec<Name>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => assigned_names_in_statement(inner, found),
        StmtChild::Expr(expr) => assigned_names_in_expr(expr, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                assigned_names_in_expr(value, found);
            }
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                assigned_names_in_statement(inner, found);
            }
        }
        StmtChild::Function(function) => {
            if let FunctionBody::Block(statements) = &function.body {
                for inner in statements {
                    assigned_names_in_statement(inner, found);
                }
            }
        }
        StmtChild::Class(_) => {}
    });
}

fn assigned_names_in_expr(expr: &Expr, found: &mut Vec<Name>) {
    match &expr.kind {
        ExprKind::Assign {
            target: AssignTarget::Place(place),
            ..
        } => {
            if let ExprKind::Ident(name) = &place.kind {
                found.push(*name);
            }
        }
        ExprKind::Update { target, .. } => {
            if let ExprKind::Ident(name) = &target.kind {
                found.push(*name);
            }
        }
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => assigned_names_in_expr(inner, found),
        Child::Function(function) => {
            if let FunctionBody::Block(statements) = &function.body {
                for inner in statements {
                    assigned_names_in_statement(inner, found);
                }
            }
        }
        Child::Class(_) => {}
    });
}
