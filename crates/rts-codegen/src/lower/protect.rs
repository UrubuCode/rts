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
//! # Why `finally` is refused
//!
//! It runs on EVERY way out — falling off the end, `return`, a raise the handler did
//! not take, a `break` that leaves the region — so it is not a block reached from one
//! place. Each of those paths has to route through it and then carry on to where it was
//! going, which is the cleanup CHAIN the machine's regions carry and this lowering does
//! not build. A block placed after the `try` would run it on the falling-off path and
//! silently skip it on the other three.

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
        if finally.is_some() {
            return Err(Unsupported::Statement(
                "a finally runs on every way out, which needs the cleanup chain",
            ));
        }
        let Some(catch) = catch else {
            return Err(Unsupported::Statement(
                "a try with no catch is only a finally, which needs the cleanup chain",
            ));
        };

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
        for statement in &catch.body {
            assigned.extend(self.assigned_in(statement)?);
        }
        let carried: Vec<BindingId> = self.carried_now(assigned);

        let handler = self.builder.block();
        // The raised value, and nothing else. A handler takes exactly one parameter
        // because exactly one thing arrives with the exception.
        let raised = self.builder.param(handler);
        self.types.insert(raised, self.domain.top());

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
        self.builder.open_region(Some(handler), None);
        let ended = self.statements(body)?;
        if !ended {
            self.builder.end(Terminator::Jump {
                target: join,
                args: before.clone(),
            });
        }
        self.builder.close_region();

        // THE HANDLER, outside the region it handles — a raise inside a `catch` is not
        // caught by its own `try`, which is what closing the region before switching
        // here says.
        self.builder.switch_to(handler);
        // THE CLAUSE HAS ITS OWN SCOPE, which is where the caught value is bound. Without
        // entering it the binding is not found at all and the name reads as a global --
        // which is exactly what the first run of this reported.
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

        self.builder.switch_to(join);
        for (binding, param) in carried.iter().zip(&joined) {
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }
}
