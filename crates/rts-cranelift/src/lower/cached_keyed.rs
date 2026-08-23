//! Lowering a property read whose key the program computed.
//!
//! Its own module rather than a method beside the other two cached reads,
//! because `lower/body.rs` is already past this crate's 1000-line ceiling and
//! rule 5 says new code lands in a small focused module rather than being
//! appended to one that is over.
//!
//! What it emits, and why the comparison is on the key's raw bits rather than on
//! a resolved key, is stated on
//! [`crate::ir::inst::Terminator::CachedGetKeyed`] — the instruction is the one
//! place that decision belongs, and this file performs it.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::error::{Capability, LowerError};
use super::memory;
use crate::ir::{BlockId, ValueId};
use crate::repr::Repr;
use crate::symbols::RtEntry;

impl Body<'_> {
    /// A cached read whose key the program computed.
    ///
    /// # The shape
    ///
    /// ```text
    ///   header == remembered  AND  key == remembered key ── no ── ask ──┐
    ///                     │ yes                                   │     │
    ///                     └──────────── read ◄────────────── found ┘  miss
    /// ```
    ///
    /// Two loads and one compare more than [`Self::lower_cached_get`], and no
    /// branch more: the two comparisons are folded with a `band` rather than
    /// nested, because both words are in the same cache line the first load
    /// already brought in and a second branch would be a second thing to
    /// predict for no information gained.
    ///
    /// # Where the key is remembered
    ///
    /// Word six of the cell, which was padding. The cell is not resized and no
    /// other kind of site loses anything, because a site's kind decides what its
    /// words MEAN and every site already gets the same eight — see
    /// `crate::target`, where the cold values are written.
    ///
    /// # Why the cold key needs no sentinel
    ///
    /// Word zero starts at a layout no object has and both comparisons must
    /// pass, so a site that has never resolved is refused before its key word
    /// matters. That is worth stating because zero is an ordinary operand: a
    /// client whose singletons start at zero spells `o[undefined]` as exactly
    /// those bits, and a design that needed the key word to be impossible would
    /// have had nowhere to put it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_cached_get_keyed(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        object: ValueId,
        key: ValueId,
        cache: crate::ir::CacheId,
        hit: &crate::ir::BlockCall,
        miss: &crate::ir::BlockCall,
    ) -> Result<(), LowerError> {
        let heap = self.heap.ok_or(LowerError::TerminatorNotYetLowered {
            block,
            needs: Capability::Memory,
        })?;

        let reference = self.value(object);
        let key_value = self.value(key);
        let address = memory::address_of(builder, reference, &heap);
        let header = memory::field_load(
            builder,
            address,
            crate::mem::HeaderLayout::TYPE_OFFSET,
            Repr::I64,
        );

        let cell = self.cache_address(builder, block, cache)?;
        let remembered = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            0,
        );
        let remembered_key = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            crate::symbols::CACHE_KEY_OFFSET,
        );
        let same_layout = builder.ins().icmp(IntCC::Equal, header, remembered);
        let same_key = builder.ins().icmp(IntCC::Equal, key_value, remembered_key);
        let recognized = builder.ins().band(same_layout, same_key);

        let read = builder.create_block();
        let ask = builder.create_block();
        builder.ins().brif(recognized, read, &[], ask, &[]);

        // Not recognized: ask once. The resolver fills the layout, the offset
        // and the key, so the loads below read what this call just wrote.
        builder.switch_to_block(ask);
        let resolved = self.call_entry_at(
            builder,
            block,
            RtEntry::CacheResolveKeyed,
            &[reference, key_value, cell],
        )?;
        let answer = builder.inst_results(resolved)[0];
        let found = builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThanOrEqual, answer, 0);
        let miss_args = self.block_args(&miss.args);
        let miss_target = self.blocks[&miss.block];
        builder
            .ins()
            .brif(found, read, &[], miss_target, &miss_args);

        // The same three words the direct read finishes on, and the same reason
        // they are read once from two paths rather than once on each: two would
        // be two places for the addressing to be written.
        builder.switch_to_block(read);
        let offset = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            8,
        );
        let indirect = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            cell,
            16,
        );
        let direct = builder.create_block();
        let through = builder.create_block();
        let based = builder.create_block();
        let base = builder.append_block_param(based, types::I64);
        builder.ins().brif(indirect, through, &[], direct, &[]);

        builder.switch_to_block(direct);
        builder
            .ins()
            .jump(based, &[cranelift_codegen::ir::BlockArg::Value(address)]);

        builder.switch_to_block(through);
        let holder = builder.ins().iadd(address, indirect);
        let elsewhere = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            holder,
            0,
        );
        builder
            .ins()
            .jump(based, &[cranelift_codegen::ir::BlockArg::Value(elsewhere)]);

        builder.switch_to_block(based);
        let at = builder.ins().iadd(base, offset);
        let value = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlags::trusted(),
            at,
            0,
        );

        let mut hit_args = vec![cranelift_codegen::ir::BlockArg::Value(value)];
        hit_args.extend(self.block_args(&hit.args));
        let hit_target = self.blocks[&hit.block];
        builder.ins().jump(hit_target, &hit_args);
        Ok(())
    }

}
