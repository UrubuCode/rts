//! Copying a cleanup into the paths that need it.
//!
//! # Why a copy and not a jump
//!
//! A cleanup is entered from every path that unwinds through it, and those
//! paths have nothing in common to return to. A jump would need one exit that
//! served all of them; a copy needs none, because each copy leaves where the
//! path it was copied into goes next.
//!
//! Three rules make that sound, and the verifier enforces all three: the piece
//! has one entry, every `CleanupDone` in it leaves to the same place, and it
//! reads only values it defines itself. A copy of something that read a value
//! from outside would read something that does not exist where the copy lands.
//!
//! # Why the piece is several blocks
//!
//! It was one, and nothing a language runs on the way out fits in one. `x + y`
//! alone emits a fast path and a slow one; a disposer is a call. A cleanup
//! limited to a single straight line could hold arithmetic on proven operands
//! and nothing else, which left `finally` and `using` with no cleanup they
//! could be. None of the three rules depended on the block count.

use std::collections::HashMap;

use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::error::LowerError;
use super::types::machine_type;
use crate::ir::{BlockId, Inst, Terminator, ValueId};
use cranelift_codegen::ir::Value;

impl Body<'_> {
    /// Copies a chain of cleanups into the current block, innermost first.
    ///
    /// Copied rather than jumped to, and the representation is what makes that
    /// sound: a cleanup ends by saying it is done, declares no parameters, and
    /// reads only what it defines itself — so a copy of it is the same thing
    /// wherever it lands. All three are checked by the verifier.
    ///
    /// The values a copy defines are local to that copy. Two copies of one
    /// cleanup in one function share nothing, which is the whole reason copying
    /// is safe where jumping was not.
    pub(super) fn emit_cleanups(
        &mut self,
        builder: &mut FunctionBuilder,
        chain: &[BlockId],
    ) -> Result<(), LowerError> {
        for &cleanup in chain {
            self.emit_cleanup_copy(builder, cleanup)?;
        }
        Ok(())
    }

    /// Copies one cleanup, which is a piece of the function rather than a block.
    ///
    /// # Why a subgraph and not a block
    ///
    /// It was a block, and the reason it stopped being one is that nothing a
    /// language wants to run on the way out fits in one. `x + y` alone emits a
    /// fast path and a slow one; a call to a disposer is a call. So a cleanup
    /// that was a single straight line could hold arithmetic on proven operands
    /// and nothing else, which meant `finally` and `using` had no cleanup they
    /// could be.
    ///
    /// What has *not* changed is the reason a cleanup is copied at all: it is
    /// entered from every path that unwinds through it, and those paths have
    /// nothing in common to return to. So it still has one entry and one
    /// logical exit — every `CleanupDone` in the piece leaves to the same
    /// place — and it still reads only values it defines itself. Those three
    /// rules are what make a copy sound, and none of them needs the piece to be
    /// one block.
    ///
    /// Control arrives at a fresh block afterwards, so a chain of cleanups is
    /// each copy's exit flowing into the next one's entry.
    fn emit_cleanup_copy(
        &mut self,
        builder: &mut FunctionBuilder,
        entry: BlockId,
    ) -> Result<(), LowerError> {
        let piece = self.cleanup_piece(entry)?;

        // Every block of the copy is created before any is filled, for the same
        // reason the function's own are: a branch may target a block later in
        // the order, and a copy that created targets on demand would have to
        // know the order in advance.
        let mut copies: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();
        let mut local: HashMap<ValueId, Value> = HashMap::new();
        for &id in &piece {
            let block = builder.create_block();
            let data = self
                .func
                .block(id)
                .ok_or(LowerError::UnterminatedBlock { block: id })?;
            for &param in &data.params {
                let value = builder.append_block_param(block, machine_type(self.repr(param)));
                local.insert(param, value);
            }
            copies.insert(id, block);
        }
        let after = builder.create_block();

        builder.ins().jump(copies[&entry], &[]);

        for &id in &piece {
            builder.switch_to_block(copies[&id]);
            let data = self
                .func
                .block(id)
                .ok_or(LowerError::UnterminatedBlock { block: id })?;

            for &inst_id in &data.insts {
                let Some(inst) = self.func.inst(inst_id) else {
                    continue;
                };
                self.emit_cleanup_inst(builder, inst_id, &inst.inst, &inst.results, &mut local)?;
            }

            self.emit_cleanup_terminator(builder, id, &copies, &local, after)?;
        }

        builder.switch_to_block(after);
        Ok(())
    }

    /// The blocks one cleanup is made of, entry first.
    ///
    /// Everything reachable from the entry without leaving through a
    /// `CleanupDone`. Collected here rather than remembered on the region,
    /// because a piece that was *declared* could disagree with the piece that
    /// is actually reachable — and the copy would then omit a block the
    /// original reaches.
    fn cleanup_piece(&self, entry: BlockId) -> Result<Vec<BlockId>, LowerError> {
        let mut order = Vec::new();
        let mut queue = vec![entry];
        while let Some(id) = queue.pop() {
            if order.contains(&id) {
                continue;
            }
            order.push(id);
            let data = self
                .func
                .block(id)
                .ok_or(LowerError::UnterminatedBlock { block: id })?;
            match &data.terminator {
                Some(Terminator::Jump(call)) => queue.push(call.block),
                Some(Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                }) => {
                    queue.push(then_block.block);
                    queue.push(else_block.block);
                }
                _ => {}
            }
        }
        // Sorted so the copy is emitted in the function's own block order. Rule
        // 13: the same input has to produce the same output, and a traversal
        // order that depended on which branch was pushed last would not.
        order.sort();
        let at = order.iter().position(|&id| id == entry).unwrap_or(0);
        order.swap(0, at);
        Ok(order)
    }

    /// Ends one block of a cleanup copy.
    ///
    /// Only three terminators can appear, and the verifier says so: a jump or a
    /// branch within the piece, or the `CleanupDone` that leaves it. A return
    /// or a throw would leave the copy through a path the unwinding it is part
    /// of knows nothing about.
    fn emit_cleanup_terminator(
        &mut self,
        builder: &mut FunctionBuilder,
        id: BlockId,
        copies: &HashMap<BlockId, cranelift_codegen::ir::Block>,
        local: &HashMap<ValueId, Value>,
        after: cranelift_codegen::ir::Block,
    ) -> Result<(), LowerError> {
        let data = self
            .func
            .block(id)
            .ok_or(LowerError::UnterminatedBlock { block: id })?;
        let args = |calls: &[ValueId]| -> Vec<cranelift_codegen::ir::BlockArg> {
            calls
                .iter()
                .filter_map(|value| local.get(value).copied())
                .map(cranelift_codegen::ir::BlockArg::Value)
                .collect()
        };

        match &data.terminator {
            Some(Terminator::CleanupDone) => {
                builder.ins().jump(after, &[]);
            }
            Some(Terminator::Jump(call)) => {
                let target = copies[&call.block];
                let arguments = args(&call.args);
                builder.ins().jump(target, &arguments);
            }
            Some(Terminator::Branch {
                cond,
                then_block,
                else_block,
            }) => {
                let cond = *local
                    .get(cond)
                    .ok_or(LowerError::UnterminatedBlock { block: id })?;
                let (t, e) = (copies[&then_block.block], copies[&else_block.block]);
                let then_args = args(&then_block.args);
                let else_args = args(&else_block.args);
                builder.ins().brif(cond, t, &then_args, e, &else_args);
            }
            _ => return Err(LowerError::UnterminatedBlock { block: id }),
        }
        Ok(())
    }

    /// Emits one instruction of a cleanup copy.
    ///
    /// Operands come from this copy's own values, never from the function around
    /// it — which is exactly what the verifier's rule about reading outside
    /// itself guarantees is possible.
    fn emit_cleanup_inst(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        inst: &Inst,
        results: &[ValueId],
        local: &mut HashMap<ValueId, Value>,
    ) -> Result<(), LowerError> {
        // Temporarily present this copy's values as the function's own, so that
        // one emission path serves both. Restored afterwards, so a cleanup's
        // values never leak into the code around it.
        let saved: Vec<_> = local
            .iter()
            .map(|(&ours, &theirs)| (ours, self.values.insert(ours, theirs)))
            .collect();

        let outcome = self.lower_inst(builder, id, inst, results);

        for &result in results {
            if let Some(&emitted) = self.values.get(&result) {
                local.insert(result, emitted);
            }
        }
        for (value, previous) in saved {
            match previous {
                Some(previous) => self.values.insert(value, previous),
                None => self.values.remove(&value),
            };
        }
        for &result in results {
            self.values.remove(&result);
        }
        outcome
    }
}
