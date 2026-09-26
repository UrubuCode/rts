//! A `break` or `continue` that leaves a `try` whose `finally` is still owed, or a
//! `for`-`of` whose iterator is.
//!
//! The machine runs a region's cleanup where the region is left by a return or a
//! raise, and a jump is neither -- so a `break` out of `try { … } finally { f() }`
//! would skip `f()`. The running emitter emits the `finally` inline before the jump,
//! and so does this: one copy per `try` the jump leaves, innermost first, each emitted
//! OUTSIDE its own `try`'s regions, so a throw from the copy is not caught by the
//! `catch` beside it nor cleaned up by the very `finally` it is running.
//!
//! What makes a copy this simple is what `protect.rs` already refuses: neither the
//! protected body nor a `finally` may assign a binding the loop carries, so what the
//! jump hands its target is what the bindings held before the copies ran. A copy that
//! itself completes abruptly -- a `return`, a `throw`, a `break` of its own -- ends
//! the path there, which is the language's rule: the new completion REPLACES the jump.

use rts_mir::cfg::Terminator;

use super::{Lowering, Unsupported};
use crate::names::resolve::ScopeId;
use crate::syntax::Stmt;

/// What a jump owes a construct it leaves: a `finally`, or a `for`-`of`'s close.
#[derive(Clone)]
pub(super) struct Owed {
    /// How many loop frames were open when the construct was entered: a jump to any
    /// of those leaves it.
    pub(super) loops: usize,
    /// How many regions were open outside the construct, which is where what it is
    /// owed runs -- outside its own protection.
    pub(super) open: usize,
    /// How many `return` targets were set outside it: a `return` inside a `finally`'s
    /// copy returns past that `try`, not through its own returning copy again.
    pub(super) returns: usize,
    /// What is owed.
    pub(super) duty: Duty,
}

/// The two things a construct can be owed on the way out.
#[derive(Clone)]
pub(super) enum Duty {
    /// A `try`'s `finally`, and the scope it is lowered in.
    Finally(Vec<Stmt>, ScopeId),
    /// A `for`-`of`'s `it.return?.()` -- where `indexed` is false, since an array
    /// walked by index has no iterator to close -- at the loop's own position.
    Close {
        indexed: rts_mir::cfg::ValueId,
        iterator: rts_mir::cfg::ValueId,
        at: crate::syntax::Expr,
    },
}

impl Lowering<'_> {
    /// Runs every `finally` and close a jump to the loop frame at `frame` leaves, then jumps to
    /// `target` with `args`. Answers `false` where no `finally` is owed and nothing was
    /// emitted, so the caller jumps as it always did.
    pub(super) fn jump_through_finally(
        &mut self,
        frame: usize,
        target: rts_mir::BlockId,
        args: Vec<rts_mir::cfg::ValueId>,
    ) -> Result<bool, Unsupported> {
        let owed: Vec<usize> = (0..self.owed_finally.len())
            .rev()
            .filter(|at| self.owed_finally[*at].loops > frame)
            .collect();
        if owed.is_empty() {
            return Ok(false);
        }
        let values = self.values.clone();
        let scope = self.scope;
        let returns = self.returns_to.clone();
        let pending = std::mem::take(&mut self.owed_finally);
        let mut left = Vec::with_capacity(owed.len());
        let mut ended = false;
        for at in owed {
            let finally = &pending[at];
            // ONLY THE `finally` CLAUSES OUTSIDE THIS ONE are owed while its copy is
            // lowered: a `break` written inside it belongs to a loop further out, and
            // with its own entry still in place it would owe itself.
            self.owed_finally = pending[..at].to_vec();
            self.returns_to.truncate(finally.returns);
            left.push(self.builder.step_out_to(finally.open));
            let copy = self.builder.block();
            self.builder.end(Terminator::Jump {
                target: copy,
                args: Vec::new(),
            });
            self.builder.switch_to(copy);
            match &finally.duty {
                Duty::Finally(statements, inner) => {
                    self.scope = *inner;
                    let lowered = self.statements(statements);
                    self.scope = scope;
                    // A `finally` that returned or threw ends the path: the jump never
                    // happens.
                    if lowered? {
                        ended = true;
                        break;
                    }
                }
                Duty::Close {
                    indexed,
                    iterator,
                    at,
                } => self.close_stepped(*indexed, *iterator, at),
            }
        }
        self.owed_finally = pending;
        self.returns_to = returns;
        if !ended {
            self.builder.end(Terminator::Jump { target, args });
        }
        for regions in left.into_iter().rev() {
            self.builder.step_back_in(regions);
        }
        self.values = values;
        Ok(true)
    }
}
