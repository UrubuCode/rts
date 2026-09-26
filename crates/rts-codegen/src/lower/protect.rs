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
//! # A `finally` that can complete abruptly is a different shape
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
//! off its end is what puts the pending throw back. `emit/protect.rs` builds both
//! shapes and chooses between them, and so does this lowering: the handler shape
//! copies the `finally` onto the raise (then raises again), onto a RETURN (then
//! returns) and onto falling off the end. Every `return` written in the body or the
//! `catch` jumps to the return copy, and the region names that copy as where a return
//! injected at a parked suspension goes -- `rts_mir::region::Region::resume_return`,
//! because that one is written nowhere and cannot be routed by a jump.
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
        // A `finally` THAT CAN COMPLETE ABRUPTLY is a catch-all HANDLER rather than a
        // cleanup -- the module header says why -- and a RETURN target, which is where
        // every `return` written in the body or the `catch` goes, and where a return
        // injected at a parked suspension is sent: `emit/protect.rs`'s shape.
        let abrupt = finally.is_some_and(|held| crate::emit::protect::leaves_abruptly(held));
        if let Some(cleanup) = finally {
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
        // A `break` OR `continue` LEAVING A `try` WITH A `finally` is a jump out of the
        // region, and the machine runs a cleanup on a return and on a throw, not on a
        // jump -- so the `finally` would be skipped. Refused, and counted over the whole
        // body including loops written inside it, which over-refuses in the safe
        // direction.
        if finally.is_some() && (jumps_out(body) || catch.is_some_and(|held| jumps_out(&held.body)))
        {
            return Err(Unsupported::Statement(
                "a break or continue inside a try with a finally skips the cleanup",
            ));
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
        //
        // WITH A `finally` AS WELL, the handler is made INSIDE the cleanup's region
        // instead -- see below.
        let make_handler = |lowering: &mut Self| {
            catch.map(|_| {
                let block = lowering.builder.block();
                // The raised value, and nothing else. A handler takes exactly one
                // parameter because exactly one thing arrives with the exception.
                let raised = lowering.builder.param(block);
                lowering.types.insert(raised, lowering.domain.top());
                (block, raised)
            })
        };
        let mut handler = match finally {
            Some(_) => None,
            None => make_handler(self),
        };
        // A cleanup takes NO parameter. Nothing jumps to it either -- it is copied --
        // so there is no edge to carry one, which is the same argument the handler's
        // single parameter rests on, reaching the opposite answer because a cleanup is
        // not handed a value.
        let cleanup = finally.filter(|_| !abrupt).map(|_| self.builder.block());
        let receiving = |lowering: &mut Self| {
            let block = lowering.builder.block();
            let value = lowering.builder.param(block);
            lowering.types.insert(value, lowering.domain.top());
            (block, value)
        };
        let unwind = finally.filter(|_| abrupt).map(|_| receiving(self));
        let returning = finally.filter(|_| abrupt).map(|_| receiving(self));

        // THE ORDINARY WAY OUT of a `try` with a `finally` runs a copy of it. The machine
        // runs a cleanup where the region is left by a return or a throw; a path that
        // simply falls off the end of the body or the handler JUMPS out, and the running
        // emitter emits the `finally` inline there for that reason. Outside every region,
        // so a throw from this copy is not caught by the `try` it belongs to.
        let normal = finally.map(|_| self.builder.block());
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

        // WHAT EVERY PATH OUT OF THE `try` AGREES ON is what held before it. A handler is
        // entered from anywhere in the body, so nothing the body bound dominates it; the
        // map is restored before the cleanup and the handler rather than inherited from
        // the body, which is what it was -- and a `catch` then read a value the body had
        // defined, which the Cranelift verifier refused on a program that ran.
        let outside = self.values.clone();
        let protected = self.builder.block();
        self.builder.end(Terminator::Jump {
            target: protected,
            args: Vec::new(),
        });
        self.builder.switch_to(protected);
        // TWO REGIONS WHEN THERE ARE BOTH, and which encloses which is the semantics --
        // `emit/protect.rs` builds the same pair. The `catch` runs BEFORE the `finally`,
        // and a throw from the `catch` still owes the `finally`: so the cleanup's region
        // is outside, the handler's inside, and the handler block lives in the outer one.
        // One region holding both ran the `finally` first, because the machine runs a
        // region's cleanup before its handler -- which a program that ran showed.
        let nested = finally.is_some() && catch.is_some();
        if nested {
            self.builder.open_region(unwind.map(|(block, _)| block), cleanup);
            if let Some((block, _)) = returning {
                self.builder.set_region_return(block);
            }
            handler = make_handler(self);
            self.builder
                .open_region(handler.map(|(block, _)| block), None);
        } else {
            let catching = handler.or(unwind).map(|(block, _)| block);
            self.builder.open_region(catching, cleanup);
            if let Some((block, _)) = returning {
                self.builder.set_region_return(block);
            }
        }
        if let Some((block, _)) = returning {
            self.returns_to.push(block);
        }
        let leaving = normal.unwrap_or(join);
        // THE BODY AND THE `finally` HAVE SCOPES OF THEIR OWN, as the clause has: a
        // `const` in a `try` body is declared there, and read from the enclosing scope
        // it was not found at all -- the name was taken for a global.
        let Some((protected, after)) = self.resolution.try_scopes(at.at) else {
            return Err(Unsupported::NoScope);
        };
        let finally_scope = after.unwrap_or(self.scope);
        let enclosing = std::mem::replace(&mut self.scope, protected);
        let ended = self.statements(body);
        self.scope = enclosing;
        let ended = ended?;
        if !ended {
            self.builder.end(Terminator::Jump {
                target: leaving,
                args: match normal {
                    Some(_) => Vec::new(),
                    None => before.clone(),
                },
            });
        }
        self.builder.close_region();
        let from_body = std::mem::replace(&mut self.values, outside.clone());

        // THE CLEANUP PIECE. One entry, and `CleanupDone` is what makes its single exit
        // structural -- a piece that fell off its end would be a copy the unwind never
        // finishes. Lowered from the values as they stood BEFORE the `try`, which is
        // sound here only because of the two refusals above: neither the body nor the
        // cleanup assigns a carried binding, and a handler that does is refused beside
        // a cleanup.
        let mut from_cleanup = std::collections::BTreeMap::new();
        if let (Some(entry), Some(statements)) = (cleanup, finally) {
            self.builder.switch_to(entry);
            self.values = outside.clone();
            for (binding, held) in carried.iter().zip(&before) {
                self.values.insert(*binding, *held);
            }
            let enclosing = std::mem::replace(&mut self.scope, finally_scope);
            let ended = self.statements(statements);
            self.scope = enclosing;
            if !ended? {
                self.builder.end(Terminator::CleanupDone);
            }
            from_cleanup = std::mem::take(&mut self.values);
        }

        // THE HANDLER, outside the region it handles — a raise inside a `catch` is not
        // caught by its own `try`, which is what closing the region before switching
        // here says.
        let mut from_handler = std::collections::BTreeMap::new();
        if let (Some((block, raised)), Some(catch)) = (handler, catch) {
            self.builder.switch_to(block);
            self.values = outside.clone();
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
                let args: Vec<ValueId> = match normal {
                    Some(_) => Vec::new(),
                    None => carried.iter().map(|held| self.values[held]).collect(),
                };
                self.builder.end(Terminator::Jump {
                    target: leaving,
                    args,
                });
            }
            from_handler = std::mem::take(&mut self.values);
        }
        if returning.is_some() {
            self.returns_to.pop();
        }
        if nested {
            self.builder.close_region();
        }

        // THE ABRUPT SHAPE'S TWO COPIES, outside every region of this `try`: on a raise,
        // the `finally` and then the raise again; on a return, the `finally` and then
        // the return, to whatever encloses this one. An abrupt completion inside either
        // replaces the pending one, which is the language's rule and why this shape.
        if let (Some((entry, raised)), Some(statements)) = (unwind, finally) {
            self.builder.switch_to(entry);
            self.values = outside.clone();
            let enclosing = std::mem::replace(&mut self.scope, finally_scope);
            let ended = self.statements(statements);
            self.scope = enclosing;
            if !ended? {
                self.builder.end(Terminator::Raise(raised));
            }
            from_cleanup.extend(std::mem::take(&mut self.values));
        }
        if let (Some((entry, value)), Some(statements)) = (returning, finally) {
            self.builder.switch_to(entry);
            self.values = outside.clone();
            let enclosing = std::mem::replace(&mut self.scope, finally_scope);
            let ended = self.statements(statements);
            self.scope = enclosing;
            if !ended? {
                self.end_return(value);
            }
            from_cleanup.extend(std::mem::take(&mut self.values));
        }

        // THE ORDINARY COPY of the `finally`, from the values as they stood before the
        // `try` -- which is all either path agrees on, and all the refusals above leave.
        if let (Some(entry), Some(statements)) = (normal, finally) {
            self.builder.switch_to(entry);
            self.values = outside.clone();
            let enclosing = std::mem::replace(&mut self.scope, finally_scope);
            let ended = self.statements(statements);
            self.scope = enclosing;
            if !ended? {
                self.builder.end(Terminator::Jump {
                    target: join,
                    args: Vec::new(),
                });
            }
            from_cleanup.extend(std::mem::take(&mut self.values));
        }

        // A BINDING FIRST BOUND INSIDE the body, the handler or the cleanup and still in
        // scope after
        // -- a `var` -- holds what one path gave it and nothing the other did: the body
        // may have thrown before its assignment, and the handler may not run at all.
        // That is a value per path with no edge to carry it along, which is the cell the
        // header's refusals name.
        let first_bound = from_body
            .keys()
            .chain(from_handler.keys())
            .chain(from_cleanup.keys())
            .any(|held| !outside.contains_key(held) && self.visible_here(*held));
        if first_bound {
            return Err(Unsupported::Statement(
                "a binding first bound inside a try or its catch, read after it, needs a cell",
            ));
        }

        self.builder.switch_to(join);
        self.values = outside;
        for (binding, param) in carried.iter().zip(&joined) {
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }
}

impl Lowering<'_> {
    /// A written `return`: to the innermost abrupt `finally` it owes, or out.
    pub(super) fn end_return(&mut self, value: ValueId) {
        match self.returns_to.last() {
            Some(block) => {
                let target = *block;
                self.builder.end(Terminator::Jump {
                    target,
                    args: vec![value],
                });
            }
            None => self.builder.end(Terminator::Return(Some(value))),
        }
    }
}

/// Whether a `break` or `continue` appears anywhere in these statements, nested
/// functions aside -- a jump that may leave the region they are written in.
fn jumps_out(statements: &[Stmt]) -> bool {
    fn one(statement: &Stmt) -> bool {
        use crate::syntax::StmtKind;
        match &statement.kind {
            StmtKind::Break(_) | StmtKind::Continue(_) => return true,
            StmtKind::Function(_) | StmtKind::Class(_) => return false,
            _ => {}
        }
        let mut found = false;
        crate::emit::capture::walk_stmt(statement, &mut |child| {
            if let crate::emit::capture::StmtChild::Stmt(inner) = child
                && one(inner)
            {
                found = true;
            }
        });
        found
    }
    statements.iter().any(one)
}
