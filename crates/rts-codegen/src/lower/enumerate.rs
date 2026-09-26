//! `for (k in o)`, as the running emitter expands it.
//!
//! # A snapshot and a guard, not a cursor
//!
//! The keys are collected ONCE by `EnumerateKeys` -- own and inherited, enumerable,
//! strings -- because a cursor into a shape is a mechanism this engine does not have.
//! A key ADDED while the loop runs is then not visited, which the language allows. A
//! key DELETED must not be, so each pass asks `ForInHas` before running the body --
//! the same guard `emit/foreach.rs` writes, and for its reason: `ForInHas` and not
//! `in`, because the operator refuses a primitive subject and this loop converts one.
//!
//! # The shape
//!
//! A counter over the snapshot's length, read once: the array is fresh and nothing
//! else can name it, so its length cannot change. The counter and the loop's carried
//! bindings cross the back edge; `continue` goes to the STEP block, which is where the
//! counter moves, the same arrangement a classic `for` has.

use rts_mir::Domain as _;
use rts_mir::cfg::{Const, Op, Terminator, ValueId};

use super::{FrameKind, LoopFrame, Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, Pattern, Stmt};

impl Lowering<'_> {
    /// A `for`-`in` whose target is DECLARED -- fresh per pass, in the head's scope,
    /// which the caller has entered.
    pub(super) fn for_in(
        &mut self,
        pattern: &Pattern,
        subject: &Expr,
        body: &Stmt,
    ) -> Result<bool, Unsupported> {
        let object = self.expression(subject)?;
        let keys = self.entry(RuntimeOp::EnumerateKeys, vec![object], subject);
        let length = self.entry(RuntimeOp::ArrayLength, vec![keys], subject);
        let zero = self.integer(0, subject);

        let mut written = self.assigned_in(body)?;
        written.extend(self.assigned_by_pattern(pattern));
        let carried = self.carried_now(written);
        let header = self.builder.block();
        let checking = self.builder.block();
        let binding = self.builder.block();
        let step = self.builder.block();
        let exit = self.builder.block();

        let mut entering = vec![zero];
        entering.extend(carried.iter().map(|held| self.values[held]));
        self.builder.end(Terminator::Jump {
            target: header,
            args: entering,
        });

        // THE HEADER: the counter, then the carried bindings, and the test.
        self.builder.switch_to(header);
        let counter = self.builder.param(header);
        self.types.insert(counter, self.domain.top());
        let mut params = Vec::with_capacity(carried.len());
        for held in &carried {
            let param = self.builder.param(header);
            self.types.insert(param, self.domain.top());
            self.values.insert(*held, param);
            params.push(param);
        }
        let more = self.prim(JsPrim::LessThan, vec![counter, length], subject);
        let more = self.prim(JsPrim::Truthy, vec![more], subject);
        self.builder.end(Terminator::Branch {
            condition: more,
            then_block: checking,
            then_args: Vec::new(),
            else_block: exit,
            else_args: params.clone(),
        });

        // THE GUARD: a key deleted since the snapshot is skipped, straight to the step.
        self.builder.switch_to(checking);
        // `ElementAt` and not an ordinary `keys[i]`: the list is the fresh array
        // `EnumerateKeys` answered, which no program can name, and the counter is
        // below its length -- every question `GetIndexed` would ask is answered by
        // construction, which is `emit/foreach.rs`'s reason for the same entry.
        let key = self.entry(RuntimeOp::ElementAt, vec![keys, counter], subject);
        let present = self.entry(RuntimeOp::ForInHas, vec![key, object], subject);
        let present = self.prim(JsPrim::Truthy, vec![present], subject);
        self.builder.end(Terminator::Branch {
            condition: present,
            then_block: binding,
            then_args: Vec::new(),
            else_block: step,
            else_args: params.clone(),
        });

        // THE BODY, with the key bound fresh.
        self.builder.switch_to(binding);
        let pass = self.open_pass(self.scope, subject);
        self.destructure(pattern, key, subject)?;
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Loop,
            header: step,
            exit,
            carried: carried.clone(),
        });
        let left = self.statement(body);
        self.loops.pop();
        if let Some((restored, _)) = pass {
            self.close_pass(restored);
        }
        if !left? {
            let args: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump { target: step, args });
        }

        // THE STEP, where `continue` lands: the counter moves and the pass repeats.
        self.builder.switch_to(step);
        let mut again = Vec::with_capacity(1 + carried.len());
        let mut stepped = Vec::with_capacity(carried.len());
        for held in &carried {
            let param = self.builder.param(step);
            self.types.insert(param, self.domain.top());
            self.values.insert(*held, param);
            stepped.push(param);
        }
        let one = self.integer(1, subject);
        again.push(self.prim(JsPrim::Add, vec![counter, one], subject));
        again.extend(stepped);
        self.builder.end(Terminator::Jump {
            target: header,
            args: again,
        });

        self.builder.switch_to(exit);
        for held in &carried {
            let param = self.builder.param(exit);
            self.types.insert(param, self.domain.top());
            self.values.insert(*held, param);
        }
        Ok(false)
    }

    /// An integer constant, typed as the domain types one.
    pub(super) fn integer(&mut self, value: i64, at: &Expr) -> ValueId {
        let value = Const::Int(value);
        let of = self.domain.of_const(&value);
        let pushed = self.builder.push(Op::Const(value), rts_mir::Effect::PURE, at.at);
        self.types.insert(pushed, of);
        pushed
    }

    /// A truth constant.
    pub(super) fn boolean(&mut self, value: bool, at: &Expr) -> ValueId {
        let value = Const::Bool(value);
        let of = self.domain.of_const(&value);
        let pushed = self.builder.push(Op::Const(value), rts_mir::Effect::PURE, at.at);
        self.types.insert(pushed, of);
        pushed
    }
}
