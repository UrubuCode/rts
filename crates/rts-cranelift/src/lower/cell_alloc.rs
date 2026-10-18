//! Lowering the taking of one cell.
//!
//! [`crate::ir::inst::Terminator::CellAlloc`] states the contract and why it
//! holds no address; [`crate::mem::CellCursor`] states what the client owes.
//! This file performs it.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlags, types};
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::error::LowerError;
use super::memory;
use crate::ir::BlockCall;
use crate::mem::{Addressing, CellCursor, HeaderLayout};
use crate::repr::{RefKind, Repr};

impl Body<'_> {
    /// A new cell, or the miss edge.
    ///
    /// # The shape
    ///
    /// ```text
    ///   plain header published ── no ──────────────────────────┐
    ///        │ yes                                             │
    ///   free list not empty ── yes ─► pop: head = cell's link  │
    ///        │ no                     write header, zero payload│
    ///   next < limit ── no ───────────────────────────────────►│
    ///        │ yes                                             │
    ///   take `next`, advance it, write header                  │
    ///        │                                              miss
    ///   hit(reference)
    /// ```
    ///
    /// The popped cell is zeroed because it held an earlier occupant; the bump
    /// cell is not, because cells past `next` are zero by the cursor's contract
    /// and the client's own allocation relies on the same fact.
    ///
    /// # Why a missing cursor is a jump and not an error
    ///
    /// A destination with no cursor -- an object file, a sharded heap -- is not a
    /// malformed program. It is a program whose every allocation takes the edge
    /// it already has, which is slower and correct.
    pub(super) fn lower_cell_alloc(
        &mut self,
        builder: &mut FunctionBuilder,
        hit: &BlockCall,
        miss: &BlockCall,
    ) -> Result<(), LowerError> {
        let miss_target = self.blocks[&miss.block];
        let hit_target = self.blocks[&hit.block];
        let heap = self.heap;
        let (cursor, stride) = match heap.as_ref() {
            Some(heap) => match (heap.bases.cursor(), heap.bases.addressing()) {
                (Some(cursor), Addressing::Single { stride, .. }) => (Some(cursor), *stride),
                _ => (None, 0),
            },
            None => (None, 0),
        };
        let (Some(cursor), Some(heap)) = (cursor, heap.as_ref()) else {
            let miss_args = self.block_args(&miss.args);
            builder.ins().jump(miss_target, &miss_args);
            return Ok(());
        };

        let flags = MemFlags::trusted();
        let header = super::memory::baked(builder, heap, super::memory::Baked::Cursor, cursor);
        let plain = builder
            .ins()
            .load(types::I64, flags, header, CellCursor::PLAIN_OFFSET);

        let probe = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(plain, probe, &[], miss_target, &miss_args);

        builder.switch_to_block(probe);
        let free = builder
            .ins()
            .load(types::I64, flags, header, CellCursor::FREE_OFFSET);
        let has_free = builder
            .ins()
            .icmp_imm(IntCC::NotEqual, free, CellCursor::NONE as i64);
        let pop = builder.create_block();
        let bump = builder.create_block();
        builder.ins().brif(has_free, pop, &[], bump, &[]);

        // The free list: the cell's first payload word is the link, and it is
        // the new head. Read BEFORE the payload is cleared.
        builder.switch_to_block(pop);
        let address = memory::address_of(builder, free, heap);
        let link = builder
            .ins()
            .load(types::I64, flags, address, HeaderLayout::BYTES as i32);
        builder
            .ins()
            .store(flags, link, header, CellCursor::FREE_OFFSET);
        builder
            .ins()
            .store(flags, plain, address, HeaderLayout::TYPE_OFFSET);
        let zero = builder.ins().iconst(types::I64, 0);
        let mut at = HeaderLayout::BYTES;
        while at < stride {
            builder.ins().store(flags, zero, address, at as i32);
            at += 8;
        }
        let reference = super::value::widen(builder, free, Repr::Ref(RefKind::Opaque))?;
        let mut hit_args = vec![BlockArg::Value(reference)];
        hit_args.extend(self.block_args(&hit.args));
        builder.ins().jump(hit_target, &hit_args);

        // Fresh space: already zero, so only the header is written.
        builder.switch_to_block(bump);
        let next = builder
            .ins()
            .load(types::I64, flags, header, CellCursor::NEXT_OFFSET);
        let limit = builder
            .ins()
            .load(types::I64, flags, header, CellCursor::LIMIT_OFFSET);
        let room = builder.ins().icmp(IntCC::UnsignedLessThan, next, limit);
        let take = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(room, take, &[], miss_target, &miss_args);

        builder.switch_to_block(take);
        let advanced = builder.ins().iadd_imm(next, 1);
        builder
            .ins()
            .store(flags, advanced, header, CellCursor::NEXT_OFFSET);
        let address = memory::address_of(builder, next, heap);
        builder
            .ins()
            .store(flags, plain, address, HeaderLayout::TYPE_OFFSET);
        let reference = super::value::widen(builder, next, Repr::Ref(RefKind::Opaque))?;
        let mut hit_args = vec![BlockArg::Value(reference)];
        hit_args.extend(self.block_args(&hit.args));
        builder.ins().jump(hit_target, &hit_args);
        Ok(())
    }
}
