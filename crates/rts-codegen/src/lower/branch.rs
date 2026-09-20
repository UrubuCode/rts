//! A branch, and what its two arms disagree about.
//!
//! Apart from the lowering because `mod.rs` passed its 1000-line ceiling a second
//! time, and this is the other seam the code already had: a branch is the one
//! construct that MERGES, and everything in this file is that one question. A loop
//! merges too and lives in `loops.rs`, for the reason its header gives -- it cannot
//! compare two finished arms, because one of its predecessors does not exist yet.

use rts_mir::cfg::Terminator;
use rts_mir::Domain;

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
        let else_values = std::mem::take(&mut self.values);
        let else_exit = self.builder.current();

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
}
