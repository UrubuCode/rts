//! A branch, and what its two arms disagree about.
//!
//! Apart from the lowering because `mod.rs` passed its 1000-line ceiling a second
//! time, and this is the other seam the code already had: a branch is the one
//! construct that MERGES, and everything in this file is that one question. A loop
//! merges too and lives in `loops.rs`, for the reason its header gives -- it cannot
//! compare two finished arms, because one of its predecessors does not exist yet.

use rts_mir::Domain;
use rts_mir::cfg::Terminator;

use super::{Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::names::resolve::BindingId;
use crate::syntax::{Expr, Stmt};

impl Lowering<'_> {
    /// Lowers an `if`, merging what the two arms disagree about.
    ///
    /// # Why the merge is computed and not declared
    ///
    /// The join block's parameters are exactly the bindings the two arms leave
    /// holding different values. Declaring one per live binding instead would be
    /// correct and would make every `if` in a program carry the whole scope
    /// through a parameter list, which is the shape `emit/merge.rs` records as the
    /// cost of not asking.
    ///
    /// Asking needs both arms lowered from the SAME starting map, which is why
    /// `before` is cloned twice rather than mutated through: an arm that rebinds a
    /// name must not be visible to the other arm, and the two orders would
    /// otherwise disagree.
    ///
    /// # Why an arm that returned contributes nothing
    ///
    /// Control does not arrive at the join from it, so its bindings are not a
    /// second opinion — they are no opinion. Joining them in would widen every
    /// type at the join for a path that cannot be taken, which is sound and is
    /// exactly the giving-up a type domain exists to avoid.
    pub(super) fn branch(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
    ) -> Result<bool, Unsupported> {
        let tested = self.expression(condition)?;
        // A machine branch wants a machine boolean and a value of this language is
        // not one, so the language's truth rule is an operation. The domain folds
        // it where the type decides it.
        let tested = self.prim(JsPrim::Truthy, vec![tested], condition);

        let then_block = self.builder.block();
        let else_block = self.builder.block();
        let before = self.values.clone();
        self.builder.end(Terminator::Branch {
            condition: tested,
            then_block,
            then_args: Vec::new(),
            else_block,
            else_args: Vec::new(),
        });

        self.builder.switch_to(then_block);
        self.values = before.clone();
        let then_ended = self.statement(then_branch)?;
        let then_values = std::mem::replace(&mut self.values, before.clone());
        let then_exit = self.builder.current();

        self.builder.switch_to(else_block);
        let else_ended = match else_branch {
            Some(branch) => self.statement(branch)?,
            None => false,
        };
        let mut else_values = std::mem::take(&mut self.values);
        let else_exit = self.builder.current();
        let mut then_values = then_values;

        // Both arms left the function, so nothing follows the `if` at all.
        if then_ended && else_ended {
            self.values = before;
            return Ok(true);
        }

        let join = self.builder.block();
        // One arm returning leaves the other as the only path, so there is nothing
        // to merge: its map IS the answer.
        if then_ended || else_ended {
            let (surviving, exit) = match then_ended {
                true => (else_values, else_exit),
                false => (then_values, then_exit),
            };
            self.values = surviving;
            self.builder.switch_to(exit);
            self.builder.end(Terminator::Jump {
                target: join,
                args: Vec::new(),
            });
            self.builder.switch_to(join);
            return Ok(false);
        }

        // A BINDING ONE ARM MADE AND THE OTHER DID NOT is either gone after the `if` --
        // a `let` of the arm's own block -- or a `var` of the function, first assigned
        // in that arm and `undefined` on the other path, because a `var` is hoisted and
        // starts there. Taking the arm's map whole left the second case holding a value
        // from one arm on both paths: a wrong answer on the other one, and a value that
        // does not dominate the join, which Cranelift's verifier refused on the first
        // program that ran through this stage.
        self.settle_one_sided(&mut then_values, &else_values, then_exit)?;
        self.settle_one_sided(&mut else_values, &then_values, else_exit)?;

        // Every binding the two arms disagree about, in a stable order: the map is
        // a `BTreeMap`, so this is the same list on every run, which is what keeps
        // one program compiling to one program.
        let merged: Vec<BindingId> = then_values
            .iter()
            .filter(|(binding, held)| else_values.get(binding).is_some_and(|other| other != *held))
            .map(|(binding, _)| *binding)
            .collect();

        let mut params = Vec::with_capacity(merged.len());
        for binding in &merged {
            let param = self.builder.param(join);
            // The type at the join is the join of the two types, which is the
            // domain doing the one thing a lattice is for. `Int32` from one arm
            // and `Double` from the other is a `Double` here — and in another
            // language it is neither.
            let of = self.domain.join(
                &self.type_of(then_values[binding]),
                &self.type_of(else_values[binding]),
            );
            self.types.insert(param, of);
            params.push(param);
        }

        self.builder.switch_to(then_exit);
        self.builder.end(Terminator::Jump {
            target: join,
            args: merged.iter().map(|held| then_values[held]).collect(),
        });
        self.builder.switch_to(else_exit);
        self.builder.end(Terminator::Jump {
            target: join,
            args: merged.iter().map(|held| else_values[held]).collect(),
        });

        // What each binding holds after the `if`: the parameter where the arms
        // disagreed, and what both said where they agreed.
        self.values = then_values;
        for (binding, param) in merged.iter().zip(params) {
            self.values.insert(*binding, param);
        }
        self.builder.switch_to(join);
        Ok(false)
    }

    /// Gives `lacking` a value for each binding `having` made that is still in scope
    /// after the arms, and makes it `undefined` -- read in `lacking_exit`, the arm whose
    /// path did not assign it.
    fn settle_one_sided(
        &mut self,
        lacking: &mut std::collections::BTreeMap<BindingId, rts_mir::ValueId>,
        having: &std::collections::BTreeMap<BindingId, rts_mir::ValueId>,
        lacking_exit: rts_mir::BlockId,
    ) -> Result<(), Unsupported> {
        let missing: Vec<BindingId> = having
            .keys()
            .filter(|held| !lacking.contains_key(held) && self.visible_here(**held))
            .copied()
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let resume = self.builder.current();
        self.builder.switch_to(lacking_exit);
        for binding in missing {
            if self.resolution.binding(binding).origin != crate::names::resolve::Origin::Var {
                // A `let` of an enclosing scope that one arm initialised and the other
                // did not is still in its dead zone on that path, which this stage
                // does not represent -- refused rather than answered.
                self.builder.switch_to(resume);
                return Err(Unsupported::Expression(
                    "a binding initialised on one arm only, still in its dead zone on the other",
                ));
            }
            let at = Expr {
                kind: crate::syntax::ExprKind::This,
                at: rts_cranelift::fault::Position::default(),
            };
            let undefined = self.singleton_at(crate::values::Singleton::Undefined, &at);
            lacking.insert(binding, undefined);
        }
        self.builder.switch_to(resume);
        Ok(())
    }

    /// Whether a binding is declared in the scope being lowered or one around it.
    pub(super) fn visible_here(&self, binding: BindingId) -> bool {
        self.visible_from(binding, self.scope)
    }

    /// What holds after a construct that merges paths: the map from before it, with the
    /// carried bindings taking what the exit received.
    ///
    /// # Why the map from before and not the one the lowering ended with
    ///
    /// The lowering walks the paths one after another through ONE map, so what it ends
    /// with holds whatever the last path bound -- a value defined on one path, which
    /// does not dominate the exit. A `var` of the function first assigned inside the
    /// construct is still in scope after it and holds a value per path, which is a cell
    /// this stage does not build, so it is refused; a binding of an inner block is gone
    /// and dropped. Every construct doing this by hand is how three of them came not to:
    /// `if`, `try` and `switch` each handed the next statement a value from one path,
    /// and the Cranelift verifier refused the first programs that ran.
    ///
    /// A value is CARRIED when it is a parameter of the block the construct left the
    /// builder in: that is what the merge made, and the only kind of value that
    /// dominates what follows while differing from what held before.
    pub(super) fn settle_after(
        &mut self,
        outside: std::collections::BTreeMap<BindingId, rts_mir::ValueId>,
    ) -> Result<(), Unsupported> {
        let after = self.scope;
        let merged: Vec<rts_mir::ValueId> = self.builder.params_of(self.builder.current()).to_vec();
        let inside = std::mem::replace(&mut self.values, outside);
        for (binding, value) in inside {
            if merged.contains(&value) {
                self.values.insert(binding, value);
            } else if let Some(before) = self.values.get(&binding) {
                // CHANGED, AND NOT BY THE MERGE: reverting it to what held before would
                // be a stale value with no error at all, so it is refused -- the one
                // failure this function must not have is a quiet one.
                if *before != value {
                    return Err(Unsupported::Statement(
                        "a binding a construct changed without its exit carrying it",
                    ));
                }
            } else if self.visible_from(binding, after) {
                return Err(Unsupported::Statement(
                    "a binding first bound inside a loop or a switch, read after it, needs a cell",
                ));
            }
        }
        Ok(())
    }

    /// Whether a binding is declared in `from` or a scope around it.
    fn visible_from(&self, binding: BindingId, from: crate::names::resolve::ScopeId) -> bool {
        let held = self.resolution.binding(binding).scope;
        let mut at = Some(from);
        while let Some(scope) = at {
            if scope == held {
                return true;
            }
            at = self.resolution.scope(scope).parent;
        }
        false
    }

    /// The five constructs [`Self::settle_after`] is applied around, dispatched.
    pub(super) fn merging(&mut self, statement: &Stmt) -> Result<bool, Unsupported> {
        use crate::syntax::StmtKind;
        match &statement.kind {
            StmtKind::While { condition, body } => self.loop_while(condition, body),
            StmtKind::DoWhile { body, condition } => self.loop_do_while(body, condition),
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => self.loop_for(
                statement.at,
                init.as_ref(),
                test.as_ref(),
                update.as_ref(),
                body,
            ),
            StmtKind::ForEach {
                source,
                target,
                subject,
                body,
                ..
            } => self.for_each(*source, target, subject, body, statement),
            StmtKind::Switch {
                discriminant,
                clauses,
            } => self.switch(discriminant, clauses),
            _ => Err(Unsupported::Statement("not a construct that merges paths")),
        }
    }

    /// `return c ? a : b` IS `if (c) return a; else return b;` -- the condition is
    /// read once, for truthiness, and the arm it picks is returned. Rewritten so
    /// that a call in either arm is the last thing its block does, which is what
    /// makes it a TAIL call: lowered as a choice, the call's answer went through a
    /// join first and an accumulator recursion a million deep overflowed the stack
    /// it runs flat in under the running emitter, which rewrites it the same way
    /// (`emit/tail.rs`).
    pub(super) fn conditional_return(
        &mut self,
        returned: &Expr,
        statement: &Stmt,
    ) -> Result<bool, Unsupported> {
        use crate::syntax::{ExprKind, StmtKind};
        let ExprKind::Conditional {
            condition: test,
            then_branch: consequent,
            else_branch: alternate,
        } = &returned.kind
        else {
            unreachable!("the guard matched a conditional");
        };
        let returning =
            |arm: &Expr| Box::new(Stmt::new(StmtKind::Return(Some(arm.clone())), statement.at));
        let rewritten = Stmt::new(
            StmtKind::If {
                condition: (**test).clone(),
                then_branch: returning(consequent),
                else_branch: Some(returning(alternate)),
            },
            statement.at,
        );
        self.statement(&rewritten)
    }
}
