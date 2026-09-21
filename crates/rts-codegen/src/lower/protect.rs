//! `try`/`catch`, as a protected region with one handler.
//!
//! # What a handler is here, and why `catch` needs no test
//!
//! This language catches everything with one clause. There is no type to match and no
//! tag to compare, so the handler is reached whenever anything inside the region
//! raises — which is why `rts_mir::region::Region` carries one handler, and the tag it
//! will need at the machine boundary is left to `MachineOps`.
//!
//! The handler receives the raised value as its first block parameter, and `catch (e)`
//! binds that parameter. `catch {}` with no binding receives it and ignores it: the
//! value still arrives, because a handler that did not receive it would have to find
//! it somewhere else.
//!
//! # The restriction, and it is a finding rather than a shortcut
//!
//! **A binding the protected body ASSIGNS cannot reach the handler as an SSA value**,
//! and the first draft of this file tried to pass one as a block parameter. That is
//! wrong, and wrong in a way that compiles: nothing JUMPS to a handler. Control
//! arrives along an exception edge from an unknown point of the body, so there is no
//! jump to carry an argument and no single value to carry — the raise may happen
//! before the assignment or after it.
//!
//! ```js
//! let x = 1;
//! try { x = 2; mayThrow(); } catch { use(x); }   // x is 2 here
//! ```
//!
//! The answer real compilers give is memory: a binding like that lives in a cell, and
//! the handler reads the cell. That is the same machinery an outer binding already
//! uses — `OuterRead` names a cell whose location the machine decides — so the piece
//! this waits on is a local that has been *moved into a cell*, which is escape
//! analysis in reverse and a pass rather than a lowering.
//!
//! Until then a `try` whose body assigns a binding the rest of the function can see is
//! refused, by name. What lowers is the shape that needs no cell: a body that reads and
//! calls, and a handler that does what it likes.
//!
//! # `finally` is a CLEANUP PIECE, and the graph does not route the paths
//!
//! The refusal that stood here said a `finally` *"is not a block reached from one
//! place"* and that each path out has to route through it — true, and it made the
//! wrong thing this lowering's problem. Which paths need a cleanup is
//! `rts_cranelift::unwind::plan_unwind` and `plan_normal_exit`, computed from the
//! region tree; a cleanup is then COPIED into each of them. So what this builds is
//! one piece with one entry and one exit, and `Terminator::CleanupDone` is what says
//! structurally that it has one exit.
//!
//! The cleanup block is created BEFORE the region is opened, which is what puts it
//! outside the region it cleans up after. That is not a detail: a cleanup inside its
//! own region would be protected by the handler it runs on the way out of, so a
//! `finally` that threw would re-enter its own `catch`.
//!
//! # What is refused, and it is a different shape rather than a missing feature
//!
//! **A `finally` that can complete ABRUPTLY.** `try { return "t" } finally { return
//! "f" }` answers `"f"`: the language says an abrupt completion in the `finally`
//! REPLACES the pending one, so the return happens and the unwind is abandoned. A
//! `return` inside a copied cleanup is a terminator with no successor, which is a
//! copy left through a path the unwind knows nothing about — the machine's verifier
//! names it, `CleanupDoesNotEnd`.
//!
//! The shape that IS correct for it is a catch-all handler rather than a cleanup: a
//! `return` in a handler is an ordinary return, and re-raising when the body falls
//! off its end is what puts the pending throw back. `emit/protect.rs` already builds
//! both shapes and chooses between them, and this lowering builds one of the two.
//!
//! **The predicate is SHARED with that file rather than written again here.**
//! `emit::protect::leaves_abruptly` over-approximates in the safe direction — a
//! `break` belonging to a loop written inside the `finally` counts although it never
//! leaves — and a second copy of a rule whose safe direction is stated in prose is a
//! second place for the direction to be got backwards. Its home is `syntax/` rather
//! than either emitter, which is where it goes when the walkers move there.

use rts_mir::Domain;
use rts_mir::cfg::{Terminator, ValueId};

use super::{Lowering, Unsupported};
use crate::names::resolve::BindingId;
use crate::syntax::{Catch, Expr, ExprKind, Pattern, Stmt};

impl Lowering<'_> {
    /// Lowers a `try`/`catch`.
    pub(super) fn protect(
        &mut self,
        body: &[Stmt],
        catch: Option<&Catch>,
        finally: Option<&Vec<Stmt>>,
        at: &Stmt,
    ) -> Result<bool, Unsupported> {
        if let Some(cleanup) = finally {
            if crate::emit::protect::leaves_abruptly(cleanup) {
                return Err(Unsupported::Statement(
                    "a finally that can complete abruptly is a handler rather than a cleanup",
                ));
            }
            // THE SAME CELL THE PROTECTED BODY WAITS ON. A cleanup is copied into
            // each path that needs it, so a binding it assigns is assigned in every
            // copy -- and what each copy leaves has to merge somewhere the copies do
            // not share. That is memory, not an SSA value.
            for statement in cleanup.iter() {
                if !self.carried_now(self.assigned_in(statement)?).is_empty() {
                    return Err(Unsupported::Statement(
                        "an assignment in a cleanup is assigned once per copy, which needs a cell",
                    ));
                }
            }
        }
        // A `try` WITH NO CATCH IS NOW ORDINARY, and the refusal that stood here
        // called it "only a finally". `Region::handler` is an `Option` precisely so
        // that a region can protect nothing and still clean up.
        if catch.is_none() && finally.is_none() {
            return Err(Unsupported::Statement(
                "a try with neither a catch nor a finally protects nothing",
            ));
        }

        // THE BODY MAY NOT ASSIGN A CARRIED BINDING. The header carries the reason: an
        // exception edge has no jump, so there is nothing to carry an argument along
        // and no single value to carry.
        for statement in body {
            if !self.carried_now(self.assigned_in(statement)?).is_empty() {
                return Err(Unsupported::Statement(
                    "an assignment in a protected body is not visible to the handler without a cell",
                ));
            }
        }

        // What the HANDLER assigns is another matter: it is reached by one edge and
        // leaves by one, so its writes merge at the join like any other arm's.
        let mut assigned = std::collections::BTreeSet::new();
        for statement in catch.iter().flat_map(|held| held.body.iter()) {
            assigned.extend(self.assigned_in(statement)?);
        }
        let carried: Vec<BindingId> = self.carried_now(assigned);
        // A CLEANUP BESIDE A HANDLER THAT ASSIGNS is the one combination refused for a
        // reason neither half has alone. The cleanup is copied into the path out of the
        // body AND the path out of the handler, and those two disagree about what the
        // binding holds -- so one copy would read a value the other's path defined.
        // Each half is fine on its own; together they need the cell.
        if finally.is_some() && !carried.is_empty() {
            return Err(Unsupported::Statement(
                "a cleanup beside a handler that assigns needs a cell, because the copies disagree",
            ));
        }

        // BOTH BLOCKS BEFORE THE REGION OPENS, which is what puts them outside it. For
        // the cleanup that is load-bearing and the header says why: inside its own
        // region, a `finally` that threw would re-enter its own `catch`.
        let handler = catch.map(|_| {
            let block = self.builder.block();
            // The raised value, and nothing else. A handler takes exactly one
            // parameter because exactly one thing arrives with the exception.
            let raised = self.builder.param(block);
            self.types.insert(raised, self.domain.top());
            (block, raised)
        });
        // A cleanup takes NO parameter. Nothing jumps to it either -- it is copied --
        // so there is no edge to carry one, which is the same argument the handler's
        // single parameter rests on, reaching the opposite answer because a cleanup is
        // not handed a value.
        let cleanup = finally.map(|_| self.builder.block());

        let join = self.builder.block();
        let joined: Vec<ValueId> = carried
            .iter()
            .map(|_| {
                let param = self.builder.param(join);
                self.types.insert(param, self.domain.top());
                param
            })
            .collect();
        // What the bindings hold before the `try`, which is what the body's path
        // supplies: the body assigns none of them, as the refusal above establishes.
        let before: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();

        let protected = self.builder.block();
        self.builder.end(Terminator::Jump {
            target: protected,
            args: Vec::new(),
        });
        self.builder.switch_to(protected);
        self.builder
            .open_region(handler.map(|(block, _)| block), cleanup);
        let ended = self.statements(body)?;
        if !ended {
            self.builder.end(Terminator::Jump {
                target: join,
                args: before.clone(),
            });
        }
        self.builder.close_region();

        // THE CLEANUP PIECE. One entry, and `CleanupDone` is what makes its single exit
        // structural -- a piece that fell off its end would be a copy the unwind never
        // finishes. Lowered from the values as they stood BEFORE the `try`, which is
        // sound here only because of the two refusals above: neither the body nor the
        // cleanup assigns a carried binding, and a handler that does is refused beside
        // a cleanup.
        if let (Some(entry), Some(statements)) = (cleanup, finally) {
            self.builder.switch_to(entry);
            for (binding, held) in carried.iter().zip(&before) {
                self.values.insert(*binding, *held);
            }
            let ended = self.statements(statements)?;
            if !ended {
                self.builder.end(Terminator::CleanupDone);
            }
        }

        // THE HANDLER, outside the region it handles — a raise inside a `catch` is not
        // caught by its own `try`, which is what closing the region before switching
        // here says.
        if let (Some((block, raised)), Some(catch)) = (handler, catch) {
            self.builder.switch_to(block);
            // THE CLAUSE HAS ITS OWN SCOPE, which is where the caught value is bound.
            // Without entering it the binding is not found at all and the name reads as
            // a global -- which is exactly what the first run of this reported.
            let Some(clause) = self.resolution.catch_scope(at.at) else {
                return Err(Unsupported::NoScope);
            };
            let outer = std::mem::replace(&mut self.scope, clause);
            match &catch.binding {
                Some(Pattern::Name(name)) => {
                    let target = Expr {
                        kind: ExprKind::Ident(*name),
                        at: at.at,
                    };
                    let of = self.type_of(raised);
                    self.bind(*name, raised, of, &target)?;
                }
                // A pattern binding destructures what was raised, which is the same
                // question a declaration asks and gets the same answer.
                Some(other) => {
                    let target = Expr {
                        kind: ExprKind::This,
                        at: at.at,
                    };
                    self.destructure(other, raised, &target)?;
                }
                // `catch {}` receives the value and ignores it.
                None => {}
            }
            let ended = self.statements(&catch.body);
            self.scope = outer;
            if !ended? {
                let args: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
                self.builder.end(Terminator::Jump { target: join, args });
            }
        }

        self.builder.switch_to(join);
        for (binding, param) in carried.iter().zip(&joined) {
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }
}
