//! Kernel W — the CONSTRUCTION + WRITE ladder. `RTS_CLASS_IMPLEMENTATION.md`
//! §7 C0.
//!
//! ## Why this kernel exists
//!
//! Everything the probe had measured before it was READS. The engine's worst
//! number is `new P()` = **555 ns/iter** [M] — 37x a field read — and §8.5 lists
//! "writes, construction, write barrier" as unmeasured. C0 says in as many
//! words: *do not start C2 before this exists — if construction stays at 555 ns,
//! cheap reads do not move class-heavy code.*
//!
//! So W prices the other half of §4.1/§4.2 on the SAME movable block layout
//! kernel H validated for reads:
//!
//! ```text
//! stab  = load [SLOT_TABLES + shard*8]   ; loop-invariant
//! block = load [stab + idx*8]            ; THE indirection — one word
//! store [block + 8*(1 + slot)] = value   ; the write twin of _sh_prload_direct
//! ```
//!
//! ## The workload
//!
//! Per iteration: construct ONE object with [`FIELDS`] `number` fields
//! (`f_j = i + j`), then read all of them back and accumulate. Same shape as
//! kernel B (`const p = new P(...)`, then use it), widened from 2 fields to 4
//! because a 2-field object makes the per-field store cost a rounding error next
//! to the allocation.
//!
//! **The read-back is not incidental — it is the checksum.** Every row sums the
//! same F values, so a variant that silently skipped a store, stored to the
//! wrong slot, or reused a previous iteration's block fails the check instead of
//! looking fast. The read path used is each row's own natural one, and reads are
//! already priced by kernel H (H0 11.20 ns locked, H4 1.49 ns direct), so the
//! read component of each row is a known quantity, not a confound.
//!
//! Unboxing is UNGUARDED in every row (`bitcast`, no `is_double` test). Kernel H
//! already priced the guard at 0.23 ns (H3→H4) and it is a read-side cost;
//! carrying it here would add the same constant to every row and blur the write
//! signal this kernel exists to isolate.
//!
//! ## The ladder — one variable per row
//!
//! | row | removes / adds, relative to the row named |
//! |---|---|
//! | W0 | — today: `vec_new_object` + one LOCKED `push` per field |
//! | W1 | −the lock and the `Box<Vec<i64>>`: block alloc, direct stores |
//! | W2 | −the allocator's returned address: recompute it from the handle |
//! | W3 | +a card-mark write barrier per field store |
//! | W4 | −shard routing: one region, slot-table base as an `iconst` |
//! | W5 | −the separate store pass: fields supplied AT allocation |
//!
//! W1→W2 is the row that matters for C2: it is what a separately-lowered
//! `this.x = v` must pay, because a real store site holds a handle, not a
//! pointer. W3 is what §8.3 makes mandatory if unboxed fields ever meet a moving
//! collector *without* a precise field map — with `fieldmap.rs`, a double-field
//! store needs no barrier and W3's delta is avoidable rather than owed.
//!
//! Numbers: see `README.md`. `TODO(measure)` — this file quotes none it did not
//! read from a doc.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_box_double, emit_unbox_double};
use crate::slab::blocks::moving_slab;
use crate::slab::cards;

/// Fields per constructed object. Must be ≤ `moving_slab::DIRECT_SLOTS` (6) and
/// must equal the arity `probe_block_alloc_filled` was written for.
pub const FIELDS: i64 = 4;

/// Word 0 of a block is the shape id, so field `j` lives at word `1 + j` — the
/// same layout kernel H reads (`SLOT_X = 1`, `SLOT_Y = 2`) and the same one the
/// locked `Vec` object gets, since `vec_new_object` pushes the shape first.
fn field_offset(j: i64) -> i32 {
    (8 * (1 + j)) as i32
}

const ALLOC_LOCKED_EMPTY: (&str, usize) = ("probe_alloc_locked_empty", 1);
const VEC_PUSH_LOCKED: (&str, usize) = ("probe_vec_push_locked", 2);
const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const BLOCK_ALLOC_ADDR: (&str, usize) = ("probe_block_alloc_addr", 1);
const BLOCK_ALLOC: (&str, usize) = ("probe_block_alloc", 1);
const REGION_ALLOC: (&str, usize) = ("probe_region_alloc", 1);
const BLOCK_ALLOC_FILLED: (&str, usize) = ("probe_block_alloc_filled", 5);

/// How the object is created and how its fields are written.
#[derive(Clone, Copy, PartialEq)]
enum W {
    /// TODAY: `__rtsn_vec_new_object` + one `__rtsn_vec_push_by_payload` per
    /// field, every one of them taking the shard `Mutex`.
    LockedPush,
    /// Movable block; the allocator hands the constructor the block ADDRESS, so
    /// the stores are `store [addr + 8*(1+j)]` with no address computation.
    BlockDirect,
    /// Same block, but the allocator returns the HANDLE and the store site
    /// recomputes the address through the slot table. What a separately-lowered
    /// `this.x = v` actually costs.
    BlockFromHandle,
    /// `BlockFromHandle` + an unconditional card mark per field store.
    BlockFromHandleBarrier,
    /// `BlockFromHandle` with one region: no shard decode, no per-shard table
    /// load, the slot table's base is an `iconst`.
    RegionFromHandle,
    /// Fields supplied to the allocator — ONE pass over the block instead of
    /// allocate-then-store-each.
    FilledAtAlloc,
}

pub fn w0_today() -> Compiled {
    build("w0_today", W::LockedPush)
}
pub fn w1_block_direct() -> Compiled {
    build("w1_block_direct", W::BlockDirect)
}
pub fn w2_store_via_handle() -> Compiled {
    build("w2_store_via_handle", W::BlockFromHandle)
}
pub fn w3_store_barrier() -> Compiled {
    build("w3_store_barrier", W::BlockFromHandleBarrier)
}
pub fn w4_region_const_base() -> Compiled {
    build("w4_region_const_base", W::RegionFromHandle)
}
pub fn w5_filled_at_alloc() -> Compiled {
    build("w5_filled_at_alloc", W::FilledAtAlloc)
}

// ---------------------------------------------------------------------------

fn build(name: &str, w: W) -> Compiled {
    let needed: &[(&'static str, usize)] = &[
        ALLOC_LOCKED_EMPTY,
        VEC_PUSH_LOCKED,
        VEC_GET_LOCKED,
        BLOCK_ALLOC_ADDR,
        BLOCK_ALLOC,
        REGION_ALLOC,
        BLOCK_ALLOC_FILLED,
    ];
    compile(name, needed, move |fb, im| emit_loop(fb, im, w))
}

/// `handle payload -> block address`, in pure IR — §4.1 verbatim, and the exact
/// computation kernel H's `Src::MovableLoad` does on the read side. Kept
/// character-for-character equivalent so a W row and an H row differ only in
/// `store` vs `load`.
fn block_from_handle(
    fb: &mut FunctionBuilder,
    slot_tables: Value,
    payload: Value,
    region: bool,
) -> Value {
    let t = MemFlags::trusted();
    let (stab, idx) = if region {
        // One region ⇒ the base is an immediate AND the payload spends no bits
        // on shard routing, so it IS the index (what Dart gets from its
        // reserved `GDT` register).
        let b = fb
            .ins()
            .iconst(types::I64, moving_slab::region_slot_table_addr());
        (b, payload)
    } else {
        let shard = fb.ins().band_imm(payload, (crate::slab::N_SHARDS - 1) as i64);
        let idx = fb.ins().ushr_imm(payload, crate::slab::SHARD_BITS as i64);
        let boff = fb.ins().imul_imm(shard, 8);
        let baddr = fb.ins().iadd(slot_tables, boff);
        (fb.ins().load(types::I64, t, baddr, 0), idx)
    };
    let soff = fb.ins().imul_imm(idx, 8);
    let saddr = fb.ins().iadd(stab, soff);
    // THE indirection: the slot holds the block's current address, so a
    // relocation rewrites this one word and no live handle changes.
    fb.ins().load(types::I64, t, saddr, 0)
}

/// The card mark, emitted once per FIELD STORE — HotSpot's shape, which is the
/// one every published barrier cost refers to. See `slab/cards.rs` for why the
/// index is masked rather than heap-base-relative (same instruction count).
fn card_mark(fb: &mut FunctionBuilder, card_table: Value, block: Value) {
    let card = fb.ins().ushr_imm(block, cards::CARD_SHIFT);
    let card = fb.ins().band_imm(card, cards::CARD_MASK);
    let addr = fb.ins().iadd(card_table, card);
    let one = fb.ins().iconst(types::I8, 1);
    fb.ins().store(MemFlags::trusted(), one, addr, 0);
}

/// The boxed word field `j` holds this iteration: `i + j` as an inline double.
/// Distinct per field on purpose — a row that stored the same value into every
/// slot, or read one slot F times, would still fail the checksum.
fn field_value(fb: &mut FunctionBuilder, i_f64: Value, j: i64) -> Value {
    let d = fb.ins().f64const(j as f64);
    let v = fb.ins().fadd(i_f64, d);
    emit_box_double(fb, v)
}

fn emit_loop(fb: &mut FunctionBuilder, im: &Imports, w: W) {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let iters = fb.block_params(entry)[0];
    let hdr = fb.block_params(entry)[1];
    // Param 3 carries the shape id, exactly as kernel B uses it — there is no
    // object ring to index here, so the `mask` slot is free.
    let shape = fb.block_params(entry)[2];

    let t = MemFlags::trusted();
    // hdr = [slot_table_addr, card_table_addr]. Both are loop-invariant and the
    // egraph will hoist them; see the README caveat about hoisting across
    // safepoints, which this kernel does not have either.
    let slot_tables = fb.ins().load(types::I64, t, hdr, 0);
    let card_table = fb.ins().load(types::I64, t, hdr, 8);

    let header = fb.create_block();
    fb.append_block_param(header, types::I64);
    fb.append_block_param(header, types::F64);
    let body = fb.create_block();
    let exit = fb.create_block();
    fb.append_block_param(exit, types::F64);

    let zero = fb.ins().iconst(types::I64, 0);
    let s0 = fb.ins().f64const(0.0);
    fb.ins().jump(header, &[zero.into(), s0.into()]);

    fb.switch_to_block(header);
    let i = fb.block_params(header)[0];
    let s = fb.block_params(header)[1];
    let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
    fb.ins().brif(go, body, &[], exit, &[s.into()]);

    fb.switch_to_block(body);
    fb.seal_block(body);
    let i_f64 = fb.ins().fcvt_from_sint(types::F64, i);

    let mut acc = s;
    match w {
        W::LockedPush => {
            let p = call1(fb, im["probe_alloc_locked_empty"], &[shape]);
            // One locked push per field — `obj.rs:97-116`'s emission.
            for j in 0..FIELDS {
                let word = field_value(fb, i_f64, j);
                let _ = call1(fb, im["probe_vec_push_locked"], &[p, word]);
            }
            // Read back through the same total, locked accessor the engine uses.
            for j in 0..FIELDS {
                let idx = fb.ins().iconst(types::I64, 1 + j);
                let r = call1(fb, im["probe_vec_get_locked"], &[p, idx]);
                let f = emit_unbox_double(fb, r);
                acc = fb.ins().fadd(acc, f);
            }
        }
        W::FilledAtAlloc => {
            // The allocator writes the fields in its own single pass, so the
            // emitted code has NO store loop at all — only the argument set-up
            // a real `new P(a, b, c, d)` already performs.
            let mut args = vec![shape];
            for j in 0..FIELDS {
                let word = field_value(fb, i_f64, j);
                args.push(word);
            }
            let p = call1(fb, im["probe_block_alloc_filled"], &args);
            let block = block_from_handle(fb, slot_tables, p, false);
            for j in 0..FIELDS {
                let r = fb.ins().load(types::I64, t, block, field_offset(j));
                let f = emit_unbox_double(fb, r);
                acc = fb.ins().fadd(acc, f);
            }
        }
        _ => {
            // The three block rows differ ONLY in how the store site obtains the
            // block address; the store and read-back instructions are identical.
            let (block, barrier) = match w {
                W::BlockDirect => {
                    // The allocator's return value IS the base — no address
                    // computation at the store site.
                    (call1(fb, im["probe_block_alloc_addr"], &[shape]), false)
                }
                W::RegionFromHandle => {
                    let p = call1(fb, im["probe_region_alloc"], &[shape]);
                    (block_from_handle(fb, slot_tables, p, true), false)
                }
                W::BlockFromHandleBarrier => {
                    let p = call1(fb, im["probe_block_alloc"], &[shape]);
                    (block_from_handle(fb, slot_tables, p, false), true)
                }
                // W::BlockFromHandle
                _ => {
                    let p = call1(fb, im["probe_block_alloc"], &[shape]);
                    (block_from_handle(fb, slot_tables, p, false), false)
                }
            };
            // The address is computed ONCE for the whole constructor body, not
            // once per store: a real lowering materializes `this` and reuses it,
            // and the egraph would CSE it here regardless.
            for j in 0..FIELDS {
                let word = field_value(fb, i_f64, j);
                fb.ins().store(t, word, block, field_offset(j));
                if barrier {
                    card_mark(fb, card_table, block);
                }
            }
            for j in 0..FIELDS {
                let r = fb.ins().load(types::I64, t, block, field_offset(j));
                let f = emit_unbox_double(fb, r);
                acc = fb.ins().fadd(acc, f);
            }
        }
    }

    let i_next = fb.ins().iadd_imm(i, 1);
    fb.ins().jump(header, &[i_next.into(), acc.into()]);
    fb.seal_block(header);

    fb.switch_to_block(exit);
    fb.seal_block(exit);
    let out = fb.block_params(exit)[0];
    let raw = emit_box_double(fb, out);
    fb.ins().return_(&[raw]);
}
