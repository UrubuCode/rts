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
        _block: BlockId,
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

        let heap = self.heap.as_ref().expect("a run table implies a heap");
        let header = super::memory::baked(builder, heap, super::memory::Baked::Runs, table);
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
        let again = builder.ins().fcvt_from_sint(types::F64, whole);
        let exact = builder.ins().fcmp(FloatCC::Equal, again, position);
        let inside = builder.ins().icmp(IntCC::UnsignedLessThan, whole, length);
        let usable = builder.ins().band(exact, inside);

        let load_block = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(usable, load_block, &[], miss_target, &miss_args);

        builder.switch_to_block(load_block);
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
        Ok(())
    }
}
