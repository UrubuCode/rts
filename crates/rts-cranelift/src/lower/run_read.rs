//! Lowering a read of one element of a run the owner keeps.
//!
//! [`crate::ir::inst::Terminator::RunRead`] states the contract and why the
//! operand is the owner; this file performs it.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlags, types};
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::error::LowerError;
use crate::ir::{BlockCall, BlockId, ValueId};
use crate::mem::RunTable;

impl Body<'_> {
    /// An element of the owner's run, or the miss edge.
    ///
    /// # The shape
    ///
    /// ```text
    ///   cell < count ── no ──────────────────────────────┐
    ///        │ yes                                        │
    ///   position exact AND position < length ── no ───────┤
    ///        │ yes                                        │
    ///   element != absent marker ── no ───────────────────┤
    ///        │ yes                                        │
    ///      hit(element)                                 miss
    /// ```
    ///
    /// Three branches and no call. The row is read from the table every time,
    /// from the owner this terminator was given, and no address leaves it.
    ///
    /// # Why the position test is one compare
    ///
    /// The position is converted to a 64-bit integer with saturation, converted
    /// back and compared for equality with itself. Equality holds only for a
    /// whole number the integer could hold: a fraction, an infinity and a NaN
    /// all come back different (a NaN saturates to zero, which is not a NaN).
    /// `-0.0` converts to zero and compares equal, which is the right answer
    /// for a position. The length compare is UNSIGNED, so a negative position,
    /// which is a huge unsigned number, fails it without a separate test.
    ///
    /// # Why a missing table is a jump and not an error
    ///
    /// A destination with no table — an object file, a sharded heap — is not a
    /// malformed program. It is a program whose every read takes the edge it
    /// already has, which is the rule-12 form: slower and correct.
    pub(super) fn lower_run_read(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        owner: ValueId,
        index: ValueId,
        hit: &BlockCall,
        miss: &BlockCall,
    ) -> Result<(), LowerError> {
        let miss_target = self.blocks[&miss.block];
        let table = self.heap.as_ref().and_then(|heap| heap.bases.runs());
        let Some(table) = table else {
            let miss_args = self.block_args(&miss.args);
            builder.ins().jump(miss_target, &miss_args);
            return Ok(());
        };

        let flags = MemFlags::trusted();
        // A single region: a reference IS the cell number.
        let cell = self.value(owner);
        let position = self.value(index);
        // A position proven whole (see `super::whole_index`) skips the round trip
        // below; the bounds compare is never skipped.
        let known = self.whole.contains(&index);

        let heap = self.heap.as_ref().expect("a run table implies a heap");
        let header = super::memory::baked(builder, heap, super::memory::Baked::Runs, table);

        // Where both walks meet: the run's first byte, what it holds and the whole
        // position, however each was come by.
        let tail = builder.create_block();
        let tail_start = builder.append_block_param(tail, types::I64);
        let tail_kind = builder.append_block_param(tail, types::I64);
        let tail_whole = builder.append_block_param(tail, types::I64);

        // A row resolved ahead of the loop (see `super::row_hoist`): valid exactly
        // while the table's generation is the one it was resolved under. Anything
        // else -- the generation moved, the position is not a whole number inside
        // the hoisted length -- takes the walk below, which is the old code.
        if let Some(row) = self.hoisted_row(block) {
            let generation = builder
                .ins()
                .load(types::I64, flags, header, RunTable::GENERATION_OFFSET);
            let same = builder.ins().icmp(IntCC::Equal, generation, row.generation);
            let whole = builder.ins().fcvt_to_sint_sat(types::I64, position);
            let inside = builder.ins().icmp(IntCC::UnsignedLessThan, whole, row.length);
            let usable = super::whole_index::position_usable(builder, known, whole, position, inside);
            let usable = builder.ins().band(usable, same);
            let walk = builder.create_block();
            builder.ins().brif(
                usable,
                tail,
                &[
                    BlockArg::Value(row.start),
                    BlockArg::Value(row.kind),
                    BlockArg::Value(whole),
                ],
                walk,
                &[],
            );
            builder.switch_to_block(walk);
        }

        let count = builder
            .ins()
            .load(types::I64, flags, header, RunTable::COUNT_OFFSET);
        let in_table = builder.ins().icmp(IntCC::UnsignedLessThan, cell, count);

        let row_block = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(in_table, row_block, &[], miss_target, &miss_args);

        builder.switch_to_block(row_block);
        let rows = builder
            .ins()
            .load(types::I64, flags, header, RunTable::ROWS_OFFSET);
        let row_offset = builder.ins().imul_imm(cell, RunTable::ROW_BYTES);
        let row = builder.ins().iadd(rows, row_offset);
        let start = builder
            .ins()
            .load(types::I64, flags, row, RunTable::ROW_START_OFFSET);
        let length = builder
            .ins()
            .load(types::I64, flags, row, RunTable::ROW_LENGTH_OFFSET);

        let whole = builder.ins().fcvt_to_sint_sat(types::I64, position);
        let inside = builder.ins().icmp(IntCC::UnsignedLessThan, whole, length);
        let usable = super::whole_index::position_usable(builder, known, whole, position, inside);

        let load_block = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(usable, load_block, &[], miss_target, &miss_args);

        builder.switch_to_block(load_block);
        let walked_kind = builder
            .ins()
            .load(types::I64, flags, row, RunTable::ROW_KIND_OFFSET);
        builder.ins().jump(
            tail,
            &[
                BlockArg::Value(start),
                BlockArg::Value(walked_kind),
                BlockArg::Value(whole),
            ],
        );

        builder.switch_to_block(tail);
        let (start, kind, whole) = (tail_start, tail_kind, tail_whole);
        let words_block = builder.create_block();
        let numeric_block = builder.create_block();
        builder
            .ins()
            .brif(kind, numeric_block, &[], words_block, &[]);

        builder.switch_to_block(words_block);
        let byte_offset = builder.ins().ishl_imm(whole, 3);
        let at = builder.ins().iadd(start, byte_offset);
        let element = builder.ins().load(types::I64, flags, at, 0);
        let absent = builder
            .ins()
            .load(types::I64, flags, header, RunTable::ABSENT_OFFSET);
        let is_absent = builder.ins().icmp(IntCC::Equal, element, absent);

        let mut hit_args = vec![BlockArg::Value(element)];
        hit_args.extend(self.block_args(&hit.args));
        let hit_target = self.blocks[&hit.block];
        let present = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(is_absent, miss_target, &miss_args, present, &[]);

        builder.switch_to_block(present);
        builder.ins().jump(hit_target, &hit_args);

        // A typed array's elements: raw numbers at a fixed width, converted to a
        // double and boxed. The row's start is the first byte of the VIEW's own
        // window and its length is in elements, both published by the client
        // under the same contract as a word run.
        builder.switch_to_block(numeric_block);
        // Not `trusted()`: that promises natural alignment, and a byte store
        // owned by a `Vec<u8>` promises none.
        let flags = MemFlags::new().with_notrap();
        let joined = builder.create_block();
        let number = builder.append_block_param(joined, types::F64);
        let kinds = [
            RunTable::KIND_F64,
            RunTable::KIND_F32,
            RunTable::KIND_I32,
            RunTable::KIND_U32,
            RunTable::KIND_I16,
            RunTable::KIND_U16,
            RunTable::KIND_I8,
            RunTable::KIND_U8,
            RunTable::KIND_U8_CLAMPED,
        ];
        for (index, wanted) in kinds.iter().enumerate() {
            let this = builder.create_block();
            let next = if index + 1 == kinds.len() {
                None
            } else {
                Some(builder.create_block())
            };
            let is_kind = builder.ins().icmp_imm(IntCC::Equal, kind, *wanted);
            let miss_args = self.block_args(&miss.args);
            match next {
                Some(next) => builder.ins().brif(is_kind, this, &[], next, &[]),
                // Past the last kind it knows, a row is a miss and not a guess.
                None => builder.ins().brif(is_kind, this, &[], miss_target, &miss_args),
            };
            builder.switch_to_block(this);
            let shift = match *wanted {
                RunTable::KIND_F64 => 3,
                RunTable::KIND_F32 | RunTable::KIND_I32 | RunTable::KIND_U32 => 2,
                RunTable::KIND_I16 | RunTable::KIND_U16 => 1,
                _ => 0,
            };
            let scaled = if shift == 0 {
                whole
            } else {
                builder.ins().ishl_imm(whole, shift)
            };
            let at = builder.ins().iadd(start, scaled);
            let converted = match *wanted {
                RunTable::KIND_F64 => builder.ins().load(types::F64, flags, at, 0),
                RunTable::KIND_F32 => {
                    let narrow = builder.ins().load(types::F32, flags, at, 0);
                    builder.ins().fpromote(types::F64, narrow)
                }
                RunTable::KIND_I32 => {
                    let word = builder.ins().sload32(flags, at, 0);
                    builder.ins().fcvt_from_sint(types::F64, word)
                }
                RunTable::KIND_U32 => {
                    let word = builder.ins().uload32(flags, at, 0);
                    builder.ins().fcvt_from_sint(types::F64, word)
                }
                RunTable::KIND_I16 => {
                    let word = builder.ins().sload16(types::I64, flags, at, 0);
                    builder.ins().fcvt_from_sint(types::F64, word)
                }
                RunTable::KIND_U16 => {
                    let word = builder.ins().uload16(types::I64, flags, at, 0);
                    builder.ins().fcvt_from_sint(types::F64, word)
                }
                RunTable::KIND_I8 => {
                    let word = builder.ins().sload8(types::I64, flags, at, 0);
                    builder.ins().fcvt_from_sint(types::F64, word)
                }
                _ => {
                    let word = builder.ins().uload8(types::I64, flags, at, 0);
                    builder.ins().fcvt_from_sint(types::F64, word)
                }
            };
            builder.ins().jump(joined, &[BlockArg::Value(converted)]);
            if let Some(next) = next {
                builder.switch_to_block(next);
            }
        }

        // One boxing for every kind, with the same NaN canonicalisation a double
        // gets anywhere else: a `NaN` payload read out of bytes could otherwise
        // land in the encoded quadrant and be mistaken for a reference.
        builder.switch_to_block(joined);
        let bits = builder
            .ins()
            .bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), number);
        let canonical = builder
            .ins()
            .iconst(types::I64, crate::tags::CANONICAL_NAN as i64);
        let is_nan = builder.ins().fcmp(FloatCC::NotEqual, number, number);
        let boxed = builder.ins().select(is_nan, canonical, bits);
        let mut boxed_args = vec![BlockArg::Value(boxed)];
        boxed_args.extend(self.block_args(&hit.args));
        builder.ins().jump(hit_target, &boxed_args);
        Ok(())
    }
}
