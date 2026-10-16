//! Lowering a store of one element of a run the owner keeps.
//!
//! [`crate::ir::inst::Terminator::RunWrite`] states the contract; this file
//! performs it, and is the mirror of `run_read.rs`.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlags, types};
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::error::LowerError;
use crate::ir::{BlockCall, BlockId, ValueId};
use crate::mem::RunTable;

impl Body<'_> {
    /// An in-place overwrite of an element that already exists, or the miss edge.
    ///
    /// # The shape
    ///
    /// ```text
    ///   cell < count ── no ───────────────────────────────┐
    ///        │ yes                                         │
    ///   position exact AND position < WRITE length ── no ──┤
    ///        │ yes                                         │
    ///   words?  ── yes ── store the value ── hit           │
    ///        │ no (a typed array)                          │
    ///   value is a double or an inline int ── no ──────────┤
    ///        │ yes                                         │
    ///   convert to the kind, store ── hit          (kind unknown) ──┘
    /// ```
    ///
    /// Everything is decided BEFORE the first byte is written, so a miss never
    /// leaves a half-done store for the client to repeat.
    ///
    /// # Why the compare is against the WRITE length
    ///
    /// The row's length says what can be READ. What may be overwritten is a
    /// different set: a frozen array reads all of its elements and overwrites
    /// none, so the client publishes a write length of zero for it. A position
    /// at or past the write length is an append, a hole or a typed array's silent
    /// no-op -- three different answers, all of them the client's.
    ///
    /// # Why nothing here can move the run
    ///
    /// A store of a word into an element that exists, or of a number's bytes into
    /// a view's window, changes no length and reallocates nothing. Growth is the
    /// only operation that moves a `Vec`, and it is the miss edge by construction.
    pub(super) fn lower_run_write(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        owner: ValueId,
        index: ValueId,
        value: ValueId,
        hit: &BlockCall,
        miss: &BlockCall,
    ) -> Result<(), LowerError> {
        let miss_target = self.blocks[&miss.block];
        let hit_target = self.blocks[&hit.block];
        let table = self.heap.as_ref().and_then(|heap| heap.bases.runs());
        let Some(table) = table else {
            let miss_args = self.block_args(&miss.args);
            builder.ins().jump(miss_target, &miss_args);
            return Ok(());
        };

        let flags = MemFlags::trusted();
        let cell = self.value(owner);
        let position = self.value(index);
        let stored = self.value(value);

        let heap = self.heap.as_ref().expect("a run table implies a heap");
        let header = super::memory::baked(builder, heap, super::memory::Baked::Runs, table);

        // Where both walks meet; see `run_read`.
        let tail = builder.create_block();
        let tail_start = builder.append_block_param(tail, types::I64);
        let tail_kind = builder.append_block_param(tail, types::I64);
        let tail_whole = builder.append_block_param(tail, types::I64);

        // A row resolved ahead of the loop, trusted exactly while the table's
        // generation is the one it was resolved under; see `super::row_hoist`.
        // The bound is the hoisted WRITE length, so a frozen array, an append and a
        // typed array's out-of-range no-op still all fall to the walk.
        if let Some(row) = self.hoisted_row(block) {
            let generation = builder
                .ins()
                .load(types::I64, flags, header, RunTable::GENERATION_OFFSET);
            let same = builder.ins().icmp(IntCC::Equal, generation, row.generation);
            let whole = builder.ins().fcvt_to_sint_sat(types::I64, position);
            let again = builder.ins().fcvt_from_sint(types::F64, whole);
            let exact = builder.ins().fcmp(FloatCC::Equal, again, position);
            let inside = builder
                .ins()
                .icmp(IntCC::UnsignedLessThan, whole, row.writable);
            let usable = builder.ins().band(exact, inside);
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
        let writable = builder
            .ins()
            .load(types::I64, flags, row, RunTable::ROW_WRITE_LENGTH_OFFSET);

        let whole = builder.ins().fcvt_to_sint_sat(types::I64, position);
        let again = builder.ins().fcvt_from_sint(types::F64, whole);
        let exact = builder.ins().fcmp(FloatCC::Equal, again, position);
        let inside = builder.ins().icmp(IntCC::UnsignedLessThan, whole, writable);
        let usable = builder.ins().band(exact, inside);

        let store_block = builder.create_block();
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(usable, store_block, &[], miss_target, &miss_args);

        builder.switch_to_block(store_block);
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

        // A plain array's element: the word, as given.
        builder.switch_to_block(words_block);
        let byte_offset = builder.ins().ishl_imm(whole, 3);
        let at = builder.ins().iadd(start, byte_offset);
        builder.ins().store(flags, stored, at, 0);
        let hit_args = self.block_args(&hit.args);
        builder.ins().jump(hit_target, &hit_args);

        // A typed array's element: the value as a number first, then the kind's
        // own conversion. Not `trusted()`, for the reason the read gives.
        builder.switch_to_block(numeric_block);
        let flags = MemFlags::new().with_notrap();

        // A double is any word outside the encoded quadrant; an inline integer is
        // the one encoded form that is a number. Anything else is not stored here.
        let base = builder
            .ins()
            .band_imm(stored, crate::tags::BOX_BASE as i64);
        let is_encoded = builder
            .ins()
            .icmp_imm(IntCC::Equal, base, crate::tags::BOX_BASE as i64);
        let double_block = builder.create_block();
        let encoded_block = builder.create_block();
        let int_block = builder.create_block();
        let joined = builder.create_block();
        let number = builder.append_block_param(joined, types::F64);
        builder
            .ins()
            .brif(is_encoded, encoded_block, &[], double_block, &[]);

        builder.switch_to_block(double_block);
        let as_double =
            builder
                .ins()
                .bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), stored);
        builder
            .ins()
            .jump(joined, &[BlockArg::Value(as_double)]);

        builder.switch_to_block(encoded_block);
        let shifted = builder
            .ins()
            .ushr_imm(stored, i64::from(crate::tags::TAG_SHIFT));
        let tag = builder
            .ins()
            .band_imm(shifted, crate::tags::TAG_MASK as i64);
        let is_int = builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, i64::from(crate::tags::TAG_INT32));
        let miss_args = self.block_args(&miss.args);
        builder
            .ins()
            .brif(is_int, int_block, &[], miss_target, &miss_args);

        builder.switch_to_block(int_block);
        let low = builder.ins().ireduce(types::I32, stored);
        let as_number = builder.ins().fcvt_from_sint(types::F64, low);
        builder
            .ins()
            .jump(joined, &[BlockArg::Value(as_number)]);

        builder.switch_to_block(joined);
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

            match *wanted {
                RunTable::KIND_F64 => {
                    builder.ins().store(flags, number, at, 0);
                }
                RunTable::KIND_F32 => {
                    let narrow = builder.ins().fdemote(types::F32, number);
                    builder.ins().store(flags, narrow, at, 0);
                }
                RunTable::KIND_U8_CLAMPED => {
                    let clamped = clamp_to_byte(builder, number);
                    builder.ins().istore8(flags, clamped, at, 0);
                }
                _ => {
                    // ToIntegerOrInfinity, then modulo 2^bits. The truncation to a
                    // 64-bit integer is exact for any magnitude below 2^63, and
                    // the low bits of that integer ARE the value modulo the
                    // width. At or past 2^63 (and an infinity) the saturating
                    // conversion would answer a wrong number, so those are the
                    // client's. A NaN converts to zero, which is the right answer.
                    let magnitude = builder.ins().fabs(number);
                    let limit = builder.ins().f64const(9_223_372_036_854_775_808.0);
                    let too_big = builder
                        .ins()
                        .fcmp(FloatCC::GreaterThanOrEqual, magnitude, limit);
                    let convert = builder.create_block();
                    let miss_args = self.block_args(&miss.args);
                    builder
                        .ins()
                        .brif(too_big, miss_target, &miss_args, convert, &[]);
                    builder.switch_to_block(convert);
                    let whole_number = builder.ins().fcvt_to_sint_sat(types::I64, number);
                    match *wanted {
                        RunTable::KIND_I32 | RunTable::KIND_U32 => {
                            builder.ins().istore32(flags, whole_number, at, 0);
                        }
                        RunTable::KIND_I16 | RunTable::KIND_U16 => {
                            builder.ins().istore16(flags, whole_number, at, 0);
                        }
                        _ => {
                            builder.ins().istore8(flags, whole_number, at, 0);
                        }
                    }
                }
            }
            let hit_args = self.block_args(&hit.args);
            builder.ins().jump(hit_target, &hit_args);
            if let Some(next) = next {
                builder.switch_to_block(next);
            }
        }
        Ok(())
    }
}

/// `Uint8ClampedArray`'s conversion: NaN and everything at or below zero store 0,
/// everything at or above 255 stores 255, the rest round to the nearest integer
/// with a tie going to the even one.
///
/// Written as integer arithmetic on the truncation so that it needs no rounding
/// instruction the target may not have: a double below 255 has an exact
/// fractional part, so comparing it with one half is exact.
fn clamp_to_byte(
    builder: &mut FunctionBuilder,
    number: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let zero = builder.ins().f64const(0.0);
    let half = builder.ins().f64const(0.5);
    let top = builder.ins().f64const(255.0);
    // False for a NaN, which is what sends it to zero.
    let positive = builder.ins().fcmp(FloatCC::GreaterThan, number, zero);
    let saturated = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, number, top);
    let truncated = builder.ins().fcvt_to_sint_sat(types::I64, number);
    let floor = builder.ins().fcvt_from_sint(types::F64, truncated);
    let fraction = builder.ins().fsub(number, floor);
    let above = builder.ins().fcmp(FloatCC::GreaterThan, fraction, half);
    let tied = builder.ins().fcmp(FloatCC::Equal, fraction, half);
    let odd = builder.ins().band_imm(truncated, 1);
    let is_odd = builder.ins().icmp_imm(IntCC::NotEqual, odd, 0);
    let tie_up = builder.ins().band(tied, is_odd);
    let up = builder.ins().bor(above, tie_up);
    let bump = builder.ins().uextend(types::I64, up);
    let rounded = builder.ins().iadd(truncated, bump);
    let full = builder.ins().iconst(types::I64, 255);
    let low = builder.ins().iconst(types::I64, 0);
    let capped = builder.ins().select(saturated, full, rounded);
    builder.ins().select(positive, capped, low)
}
