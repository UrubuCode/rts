//! The object field / element read as real Cranelift `load`s
//! (`RTS_OPTIMIZATION.md` §5 Tier 3.2).
//!
//! Every property read the engine emits — a shape-hit slot load
//! ([`super::ic`]), a method-table read ([`super::class::vdispatch`]), an
//! element read in `for…of` ([`super::loops`]), a closure env slot
//! ([`super::thunk`]) — funnels through ONE emission,
//! [`crate::value::emit_marshal::emit_vec_get`], and that emission was a `call
//! __rtsn_vec_get_by_payload`. A call is an optimization barrier for the
//! egraph, a shard lock, and a function prologue, all to move eight bytes out
//! of a slot whose address is a pure arithmetic function of the payload word
//! already in a register. This module emits that arithmetic instead, and keeps
//! the call as the arm every rejection lands on.
//!
//! ## The emitted sequence
//!
//! ```text
//!   payload = boxed & SLOT_MASK          ; the mask happens HERE, not at the
//!                                        ; call site — see emit_payload
//!   shard   = payload & SHARD_MASK
//!   idx     = payload >> SHARD_BITS
//!   chunk   = idx >> chunk_bits
//!   if chunk >= max_chunks                                  -> slow
//!   base    = load.i64 [chunk_table_base + (shard*max_chunks + chunk)*8]
//!   if base == 0                                            -> slow
//!   slot    = base + (idx & chunk_mask) * slot_stride
//!   if uload8  [slot + entry_tag_off] != entry_tag_vec       -> slow
//!   if uload8  [slot + slots_tag_off] != slots_tag_inline    -> slow
//!   if index unsigned>= uload32 [slot + inline_len_off]      -> slow
//!   fast    = load.i64 [slot + inline_words_off + index*8]
//! ```
//!
//! Every constant in it comes from
//! [`rts_engine::heap::slab::layout::FieldLoadLayout`] at emission time. None is
//! written here: a hardcoded copy of a Rust type layout is a silent-miscompile
//! generator the day someone adds a field, which is exactly why that module
//! derives the numbers from a real `Slot` and verifies them.
//!
//! The five bail edges all jump to the SAME cold block, so the function carries
//! one slow-path call, not five. The blocks are marked cold under the existing
//! `RTS_COLD_BLOCKS` knob; `RTS_OPTIMIZATION.md` §10 records that cold blocks
//! have measured ZERO so far, so no win is claimed from that — it is used here
//! because it costs nothing and keeps this emission consistent with the other
//! guards.
//!
//! ### Why the length compare is UNSIGNED, and why that is two checks in one
//!
//! `index` is an `i64` and may be negative;
//! `payload_ops::vec_get_by_payload` answers `0` for a negative index just as it
//! does for one past the end. Comparing the index UNSIGNED against the
//! zero-extended `u32` length makes a negative index a huge unsigned value, so
//! it fails the same compare an over-long index fails and takes the same slow
//! path — which then returns `0`. One `icmp` therefore covers both conditions,
//! and no separate `index >= 0` test is emitted.
//!
//! ## Why there is no generation check
//!
//! Because the function this fast-paths does not perform one.
//! `payload_ops::vec_get_by_payload` receives a 48-bit PAYLOAD, and a payload
//! carries no generation — there is nothing to compare against. What it rejects
//! is an out-of-range or unallocated slot and a slot whose `Entry` is not a
//! `Vec`, and the chunk-bound test, the null-chunk test and the entry-tag test
//! above reject exactly those. Reproducing its semantics is the requirement;
//! adding a check it does not perform would be a *different* function, not a
//! safer one.
//!
//! ## Why a raw load of a live slot is sound
//!
//! The three properties are established and argued in
//! [`rts_engine::heap::slab::layout`]'s module docs, and are only cited here:
//! the chunk addresses are published once and never moved, reallocated or freed
//! (which is why this is gated on `RTS_SLAB=1`); the inline words live INSIDE
//! the slot, so no second buffer exists for a sweep to drop or a `push` to
//! reallocate; and the receiver's handle word is live in the frame — a
//! conservative root — while a GC tick can only fire from `alloc_entry`, i.e.
//! only at a call, and this sequence contains none.
//!
//! ## Promotion is why the inline test is in the emitted code
//!
//! `Slots::promote` moves the words out of the slot into a heap `Vec` when an
//! object grows past `INLINE_CAP`. Promotion is one-way, so the test costs one
//! byte compare rather than a revalidation loop — but it must be emitted and
//! executed on EVERY read, because the compiler cannot prove an object never
//! gains a property between two reads. There is no way to hoist it.
//!
//! ## Gating
//!
//! Three independent conditions, each for its own reason — see
//! [`available_here`].

use cranelift_codegen::ir::{Block, InstBuilder, MemFlags, Value, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

use rts_engine::abi::handles::{HANDLE_SHARD_BITS, HANDLE_SHARD_MASK, HANDLE_SLOT_MASK};
use rts_engine::heap::slab::layout::{self, FieldLoadLayout};

/// Whether the inline field-load fast path may be emitted at all.
///
/// All three must hold, and each rules out a different way the sequence would
/// be wrong:
///
/// 1. [`layout::available`] — false unless `RTS_SLAB=1` selected the chunked,
///    stable-address slot store AND the derived layout verified. Without it the
///    slots live in a `Vec` that reallocates, so no address into it is stable
///    and no load through one is sound.
/// 2. **JIT only.** The chunk-table base is a PROCESS address, materialized
///    below as an `iconst` immediate. An AOT object outlives the process that
///    compiled it, so an address baked into it is meaningless — and the AOT
///    path also feeds [`super::bake`], whose captured functions are replayed
///    later. `aot_mode()` is the flag both already set.
///    TODO(3.2-aot): declare the chunk table's base as an `#[rtse::abi]` data
///    symbol and LOAD it (one extra `load` on the fast path) instead of
///    embedding it; the rest of the sequence is address-independent and needs
///    no change.
/// 3. [`super::clifflags::field_load`] — its own kill switch, so this Tier item
///    is A/B-measurable on one binary independently of every other switch.
///
/// And one more that follows from (2) rather than standing beside it: the
/// opt-in program cache ([`super::progcache`]) replays BAKED MACHINE CODE from
/// disk into a LATER process, and `CHUNK_TABLE` is a BSS static whose runtime
/// address moves under ASLR — so a baked immediate is a wrong address in the
/// process that replays it. A version bump cannot fix that (the address differs
/// run to run, not version to version), so the emission is refused while that
/// cache is on. The prelude cache is unaffected: it persists the HIR-level
/// `LoweredProgram`, and the machine code is compiled fresh every run.
pub(crate) fn available_here() -> bool {
    super::clifflags::field_load()
        && !super::aot_str::aot_mode()
        && !super::progcache::enabled()
        && layout::available()
}

/// Emit the inline-slot fast path in front of the ordinary
/// `__rtsn_vec_get_by_payload` call, returning the merged `i64` result.
///
/// `boxed` is the WHOLE NaN-boxed receiver word — not a masked payload; see
/// `emit_marshal::emit_payload` for why the mask must not happen at the call
/// site — and `index` the `i64` element index. `slow` emits the ordinary
/// call itself; it is invoked at most once, inside the cold block, and must
/// return the `i64` word that block jumps to the merge with.
///
/// `None` means the fast path is not available (see [`available_here`]) and
/// NOTHING was emitted — the caller emits its ordinary call alone.
pub(crate) fn try_emit(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    boxed: Value,
    index: Value,
    slow: impl FnOnce(&mut dyn Module, &mut FunctionBuilder) -> Value,
) -> Option<Value> {
    if !available_here() {
        return None;
    }
    let l = layout::field_load_layout()?;

    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    let bail = builder.create_block();
    if super::clifflags::cold_blocks() {
        builder.set_cold_block(bail);
    }

    let word = emit_fast(builder, &l, boxed, index, bail);
    builder.ins().jump(merge, &[word.into()]);

    // The one slow-path call every rejection above jumped to.
    builder.seal_block(bail);
    builder.switch_to_block(bail);
    let slow_word = slow(module, builder);
    builder.ins().jump(merge, &[slow_word.into()]);

    builder.switch_to_block(merge);
    builder.seal_block(merge);
    Some(builder.block_params(merge)[0])
}

/// The fast path proper: the address arithmetic, the four guards (all branching
/// to `bail`), and the final word load.
///
/// Returns the loaded word; the caller jumps it to the merge block. Each guard
/// branches to a fresh continuation block on success and to the shared `bail`
/// on failure, so the merge keeps exactly two predecessors: this path's final
/// jump and the cold call.
fn emit_fast(
    builder: &mut FunctionBuilder,
    l: &FieldLoadLayout,
    boxed: Value,
    index: Value,
    bail: Block,
) -> Value {
    // `MemFlags::trusted` on every load below: `notrap` because each address is
    // inside a chunk the store published and never frees (layout.rs, property
    // one), and `aligned` because the chunk allocation and `size_of::<Slot>()`
    // stride keep each slot at its natural alignment.
    let flags = MemFlags::trusted();

    // The caller hands over the whole NaN-boxed word, not a masked payload —
    // see `emit_marshal::emit_payload` for why that is a GC requirement and not
    // a convenience. The 48-bit slot payload is recovered HERE, inside the fast
    // path, so the boxed word stays the value the cold-block call consumes and
    // therefore the value the conservative scanner can still see on the stack.
    let payload = builder.ins().band_imm(boxed, HANDLE_SLOT_MASK as i64);
    let shard = builder
        .ins()
        .band_imm(payload, HANDLE_SHARD_MASK as i64);
    let idx = builder.ins().ushr_imm(payload, HANDLE_SHARD_BITS as i64);

    // (1) chunk in range.
    let chunk = builder.ins().ushr_imm(idx, l.chunk_bits as i64);
    let in_range = builder
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, chunk, l.max_chunks as i64);
    guard(builder, in_range, bail);

    // (2) the chunk is published. The table is flat: `shard * max_chunks + chunk`.
    let row = builder.ins().imul_imm(shard, l.max_chunks as i64);
    let entry = builder.ins().iadd(row, chunk);
    let byte_off = builder.ins().imul_imm(entry, 8);
    let table = builder.ins().iconst(types::I64, l.chunk_table_base as i64);
    let addr = builder.ins().iadd(table, byte_off);
    let base = builder.ins().load(types::I64, flags, addr, 0);
    let published = builder.ins().icmp_imm(IntCC::NotEqual, base, 0);
    guard(builder, published, bail);

    let off = builder.ins().band_imm(idx, l.chunk_mask as i64);
    let slot_off = builder.ins().imul_imm(off, l.slot_stride as i64);
    let slot = builder.ins().iadd(base, slot_off);

    // (3) the entry really is a `Vec`.
    let etag = builder
        .ins()
        .uload8(types::I64, flags, slot, l.entry_tag_off as i32);
    let is_vec = builder
        .ins()
        .icmp_imm(IntCC::Equal, etag, l.entry_tag_vec as i64);
    guard(builder, is_vec, bail);

    // (4) the words are still INLINE — `Slots::promote` is one-way, but it can
    // have happened before this read, so the test is on every read.
    let stag = builder
        .ins()
        .uload8(types::I64, flags, slot, l.slots_tag_off as i32);
    let is_inline = builder
        .ins()
        .icmp_imm(IntCC::Equal, stag, l.slots_tag_inline as i64);
    guard(builder, is_inline, bail);

    // (5) the index is in bounds. UNSIGNED, which is also what rejects a
    // negative index — see the module doc.
    // `uload32` yields an `i64` already zero-extended, which is exactly the
    // widening the unsigned compare needs.
    let len = builder.ins().uload32(flags, slot, l.inline_len_off as i32);
    let in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, index, len);
    guard(builder, in_bounds, bail);

    let word_off = builder.ins().imul_imm(index, 8);
    let word_addr = builder.ins().iadd(slot, word_off);
    builder
        .ins()
        .load(types::I64, flags, word_addr, l.inline_words_off as i32)
}

/// Continue in a fresh block when `cond` holds, jump to the shared `bail`
/// otherwise. The builder is left positioned in the continuation block, so the
/// guards chain by simply being written in sequence.
fn guard(builder: &mut FunctionBuilder, cond: Value, bail: Block) {
    let cont = builder.create_block();
    builder.ins().brif(cond, cont, &[], bail, &[]);
    builder.seal_block(cont);
    builder.switch_to_block(cont);
}
