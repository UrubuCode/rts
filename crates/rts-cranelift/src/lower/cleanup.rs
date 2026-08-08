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

use std::collections::{HashMap, HashSet};

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
        // `seen` is a membership index beside `order`, not a replacement for
        // it: `order.contains(&id)` was a linear scan of a `Vec` that grows
        // with every block already collected, O(n^2) in the blocks of one
        // cleanup — the same shape as `verify::rules::cleanup_piece`, which
        // this mirrors and which carries the full measurement (60
        // `try`/`finally`, 29.32 ms). Sound under rule 13 for the same reason
        // as there: `order` is sorted below before it is ever read, so which
        // order the worklist visited blocks in — and hash order along with
        // it — is never observed.
        let mut seen: HashSet<BlockId> = HashSet::new();
        let mut queue = vec![entry];
        while let Some(id) = queue.pop() {
            if !seen.insert(id) {
                continue;
            }
            order.push(id);
            let data = self
                .func
                .block(id)
                .ok_or(LowerError::UnterminatedBlock { block: id })?;
            // Every successor, not the two obvious ones. A guard and an inline
            // cache are branches too, so a piece that followed only jumps would
            // stop at the first arithmetic on unproven operands -- which is
            // most cleanups.
            if let Some(terminator) = &data.terminator {
                queue.extend(terminator.successors());
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
    /// `CleanupDone` is the only one this handles itself, because it is the only
    /// one that means something different in a copy: it leaves the piece, and
    /// where it leaves to is where this copy was placed.
    ///
    /// Everything else goes through the ordinary terminator lowering, with the
    /// copy's blocks and values temporarily standing in for the function's own.
    /// A guard and an inline cache are branches, and re-implementing them here
    /// would be a second statement of how each one lowers — which is the shape
    /// of bug where a copy of a cleanup and the original disagree about the
    /// same construct.
    fn emit_cleanup_terminator(
        &mut self,
        builder: &mut FunctionBuilder,
        id: BlockId,
        copies: &HashMap<BlockId, cranelift_codegen::ir::Block>,
        local: &HashMap<ValueId, Value>,
        after: cranelift_codegen::ir::Block,
    ) -> Result<(), LowerError> {
        let terminator = self
            .func
            .block(id)
            .and_then(|data| data.terminator.clone())
            .ok_or(LowerError::UnterminatedBlock { block: id })?;

        if matches!(terminator, Terminator::CleanupDone) {
            builder.ins().jump(after, &[]);
            return Ok(());
        }

        let saved_values: Vec<_> = local
            .iter()
            .map(|(&ours, &theirs)| (ours, self.values.insert(ours, theirs)))
            .collect();
        let saved_blocks: Vec<_> = copies
            .iter()
            .map(|(&ours, &theirs)| (ours, self.blocks.insert(ours, theirs)))
            .collect();

        let outcome = self.lower_terminator(builder, id, &terminator);

        for (block, previous) in saved_blocks {
            match previous {
                Some(previous) => self.blocks.insert(block, previous),
                None => self.blocks.remove(&block),
            };
        }
        for (value, previous) in saved_values {
            match previous {
                Some(previous) => self.values.insert(value, previous),
                None => self.values.remove(&value),
            };
        }
        outcome
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
        // Only this instruction's own operands are staged into `self.values`,
        // not the whole `local` map. `lower_inst` reads `self.values` by
        // looking each operand up individually (`self.value(id)`) — it never
        // iterates the map — so nothing beyond what this instruction actually
        // names needs to be visible. The previous version copied every entry
        // of `local` in and back out for every instruction lowered: O(locals x
        // instructions) over a cleanup copy, where O(locals + instructions)
        // is what the work requires. Measured on the language-layer side of
        // this same shape: 60 `try`/`finally` cost 29.32 ms against 300
        // `if`/`else` at 7.55 ms, roughly 4x the per-statement cost. The
        // rejected alternative was swapping the whole map in and out once per
        // cleanup copy (outside this function, around the whole piece); that
        // is sound too, but it was not chosen because `local` grows across the
        // copy as results are produced, so "once per copy" would still mean
        // restoring entries this particular instruction never touched.
        // Deduplicated first: an instruction like `x + x` names the same
        // operand twice, and staging it twice would save the restore-value
        // from the *first* staging as the "previous" value for the second —
        // so restoring in order would overwrite the correct original value
        // with the value this instruction saw. `seen` keeps one save/restore
        // pair per distinct operand.
        let mut seen = HashSet::new();
        let saved: Vec<_> = inst
            .operands()
            .into_iter()
            .filter(|operand| seen.insert(*operand))
            .filter_map(|operand| {
                let theirs = *local.get(&operand)?;
                Some((operand, self.values.insert(operand, theirs)))
            })
            .collect();

        // The results get the same save/restore discipline as the operands
        // above, and for a sharper reason. They used to be REMOVED after the
        // copy, unconditionally — which is right when the copy is the only
        // thing that ever defined them, and wrong the moment the original
        // block was already lowered: `self.values` is shared, so removing a
        // result deleted the ORIGINAL block's binding for the same `ValueId`.
        //
        // A later block dominated by that original then read a value that was
        // no longer in the map, and the lowering panicked with `no entry found
        // for key`. It needed a cleanup piece spanning more than one block —
        // which a global read gives, because `console` is a call and `.log` is
        // a guard plus a cache — so a value defined in the piece's entry was
        // still live at its exit. A local call produced nothing that outlived
        // its own block and never reached it.
        //
        // Restoring rather than removing makes a copy invisible to the map,
        // which is the property that makes copying sound at all. Simply not
        // removing would be worse than the panic: the copy's values would leak
        // into later blocks that its defining block does not dominate, and that
        // is a miscompilation rather than a crash.
        let restored: Vec<_> = results
            .iter()
            .map(|&result| (result, self.values.get(&result).copied()))
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
        for (result, previous) in restored {
            match previous {
                Some(previous) => self.values.insert(result, previous),
                None => self.values.remove(&result),
            };
        }
        outcome
    }
}
