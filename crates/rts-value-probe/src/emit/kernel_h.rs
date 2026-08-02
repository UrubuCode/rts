//! Kernel H — TODAY's object management vs the STATIC-HERMES-shaped one,
//! adapted to the constraint RTS cannot lift.
//!
//! ## Why Hermes and not V8
//!
//! Static Hermes is the closest existing system to RTS: nominal typed classes,
//! field slot indices assigned at DECLARATION (not discovered by a hidden-class
//! transition), `PrLoad`/`PrStore` by constant index with no guard, ICs
//! quarantined to the untyped path only. RTS already does the hard half of this
//! — `obj.rs:608-636` resolves `this.x` to a compile-time constant slot. What it
//! then does with that constant is `call __rtsn_vec_get_by_payload`, which takes
//! a shard `Mutex` and dereferences a `Box<Vec<i64>>`.
//!
//! ## The constraint Hermes does not have
//!
//! Hermes emits `_sh_prload_direct(obj, idx)` — a load at a fixed offset from a
//! RAW POINTER. RTS has no raw pointer to an object and cannot get one: the
//! conservative GC scanner (`collector/scan.rs`) recognizes roots by decoding
//! `gen|slot|shard` handle words and validating them against the live
//! `HandleTable`, and the threading model (`rts-threading-model.md`) is defined
//! on "payload = slot index, never a pointer" so a region can re-home an entry
//! by updating its slot. A raw pointer in a register would be invisible to the
//! first and unmovable by the second.
//!
//! So the Hermes-shaped design for RTS is: **keep handle identity, make
//! handle→address pure IR**:
//!
//! ```text
//! shard = payload & 31
//! idx   = payload >> 5
//! base  = load [SHARD_BASES + shard*8]    ; loop-invariant — the egraph hoists it
//! addr  = base + idx * STRIDE
//! x     = load [addr + 8*(1 + slot)]      ; the Hermes `_sh_prload_direct`
//! ```
//!
//! One extra hoistable load and some ALU, against a call plus a mutex. Whether
//! that trade is real is the ONLY question this kernel answers.
//!
//! ## The ladder
//!
//! Workload identical to kernel M so the tables are comparable:
//! `s += p.x + p.y` over 1024 pre-built objects, `p = objs[i & mask]`.
//! Arithmetic is INLINE in every row — kernel A/M already priced the generic
//! operator, and leaving it in would just add a constant to every row and hide
//! the storage lever this kernel is about.
//!
//! Rows H0–H3 unbox each loaded field through a REAL GUARD (`is_double` fast
//! arm, `probe_to_number` call slow arm, reachable): a `Tagged` field slot
//! cannot be `bitcast` blindly. H4 drops the guard because that is exactly what
//! a per-field `Repr` would buy — and today `Repr` never reaches a heap slot
//! (`obj.rs:616` hands back `Repr::Tagged` unconditionally, and there is no
//! `field_numbers` set anywhere in `synth.rs`).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_box_double, emit_is_double, emit_unbox_double};
use crate::slab::blocks::{CHUNK_BITS, CHUNK_MASK, CHUNKS, inline_slab, packed_slab, region_slab};

const OVERFLOW_WORD: usize = inline_slab::OVERFLOW_WORD;
const STRIDE_BYTES: i64 = inline_slab::STRIDE_BYTES;

const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const VEC_GET_UNLOCKED: (&str, usize) = ("probe_vec_get_unlocked", 2);
const INLINE_GET: (&str, usize) = ("probe_inline_get", 2);
const TO_NUMBER: (&str, usize) = ("probe_to_number", 1);

const SLOT_X: i64 = 1;
const SLOT_Y: i64 = 2;

/// Where the field words come from.
#[derive(Clone, Copy, PartialEq)]
enum Src {
    /// TODAY: `call __rtsn_vec_get_by_payload` — shard `Mutex` + `Box<Vec>`.
    LockedCall,
    /// Same shape, lock removed. Isolates the lock.
    UnlockedCall,
    /// Inline-slot slab, still behind an opaque call. Isolates the `Box`.
    InlineCall,
    /// Pure IR: handle → address → load. Isolates the call.
    InlineLoad,
    /// Pure IR into the OVERFLOW bag: load the overflow pointer out of the
    /// object, then index it. Hermes's `_sh_prload_indirect`, and the fallback
    /// the census's HARD #4 (`(p as any).z = 1` is legal) requires.
    OverflowLoad,
    /// Pure IR, but through a CHUNK TABLE instead of one base per shard — the
    /// stable storage a real implementation needs, since a `Vec` realloc would
    /// invalidate every live object address mid-run. Prices the precondition
    /// `InlineLoad` quietly assumes.
    ChunkedLoad,
    /// Pure IR through ONE slot indirection: `block = load [slot_table[shard] +
    /// idx*8]`, then the field. The MOVABLE form — the only one compatible with
    /// regional promotion (`rts-threading-model.md`) and a copying nursery
    /// (`gc-generational-design.md`), because moving an object is one word
    /// written and no live handle changes.
    MovableLoad,
    /// The movable form with the slot table's base as an `iconst` (one region),
    /// so the shard-table load disappears and only THE indirection is left.
    MovableRegion,
    /// Pure IR with a stride of 4 words (32 B) instead of 8 (64 B). Same
    /// addressing, half the footprint — prices the CACHE cost of a fixed stride
    /// on a 2-field class.
    PackedLoad,
    /// Pure IR into a single thread-local REGION: no shard routing, base is an
    /// `iconst`. The ceiling of finishing the threading model's "shards are
    /// proto-regions".
    RegionLoad,
    /// The object never existed.
    Scalarized,
}

#[derive(Clone, Copy)]
struct V {
    src: Src,
    /// Unbox each field word through a reachable guard (a `Tagged` slot needs
    /// it). False = the field's `Repr` was proven Float64 at compile time.
    guarded: bool,
}

pub fn h0_today() -> Compiled {
    build("h0_today", V { src: Src::LockedCall, guarded: true })
}
pub fn h1_no_lock() -> Compiled {
    build("h1_no_lock", V { src: Src::UnlockedCall, guarded: true })
}
pub fn h2_inline_slots_call() -> Compiled {
    build("h2_inline_slots_call", V { src: Src::InlineCall, guarded: true })
}
pub fn h3_pure_ir_load() -> Compiled {
    build("h3_pure_ir_load", V { src: Src::InlineLoad, guarded: true })
}
pub fn h4_repr_proven() -> Compiled {
    build("h4_repr_proven", V { src: Src::InlineLoad, guarded: false })
}
pub fn h5_overflow_bag() -> Compiled {
    build("h5_overflow_bag", V { src: Src::OverflowLoad, guarded: false })
}
pub fn h6_scalarized() -> Compiled {
    build("h6_scalarized", V { src: Src::Scalarized, guarded: false })
}
pub fn h7_chunked() -> Compiled {
    build("h7_chunked", V { src: Src::ChunkedLoad, guarded: false })
}
pub fn h8_packed_stride() -> Compiled {
    build("h8_packed_stride", V { src: Src::PackedLoad, guarded: false })
}
pub fn h9_region_const_base() -> Compiled {
    build("h9_region_const_base", V { src: Src::RegionLoad, guarded: false })
}
pub fn h10_movable() -> Compiled {
    build("h10_movable", V { src: Src::MovableLoad, guarded: false })
}
pub fn h11_movable_region() -> Compiled {
    build("h11_movable_region", V { src: Src::MovableRegion, guarded: false })
}

// ---------------------------------------------------------------------------

fn build(name: &str, v: V) -> Compiled {
    let needed: &[(&'static str, usize)] = &[
        VEC_GET_LOCKED,
        VEC_GET_UNLOCKED,
        INLINE_GET,
        TO_NUMBER,
    ];
    compile(name, needed, move |fb, im| emit_loop(fb, im, v))
}

/// `handle payload -> object address`, in pure IR. This is the whole proposal.
fn handle_to_addr(
    fb: &mut FunctionBuilder,
    base_table: Value,
    payload: Value,
    stride: i64,
) -> Value {
    let t = MemFlags::trusted();
    let shard = fb.ins().band_imm(payload, (crate::slab::N_SHARDS - 1) as i64);
    let idx = fb.ins().ushr_imm(payload, crate::slab::SHARD_BITS as i64);
    let boff = fb.ins().imul_imm(shard, 8);
    let baddr = fb.ins().iadd(base_table, boff);
    let base = fb.ins().load(types::I64, t, baddr, 0);
    let off = fb.ins().imul_imm(idx, stride);
    fb.ins().iadd(base, off)
}

/// Same, but through the chunk table — the stable-storage version. The two-level
/// index `[shard][chunk]` collapses into ONE flat table load, so the extra cost
/// against [`handle_to_addr`] is a shift and a mask, not a second dependent
/// load. That is the whole point of measuring it.
fn handle_to_addr_chunked(
    fb: &mut FunctionBuilder,
    chunk_table: Value,
    payload: Value,
    stride: i64,
) -> Value {
    let t = MemFlags::trusted();
    let shard = fb.ins().band_imm(payload, (crate::slab::N_SHARDS - 1) as i64);
    let idx = fb.ins().ushr_imm(payload, crate::slab::SHARD_BITS as i64);
    let chunk = fb.ins().ushr_imm(idx, CHUNK_BITS as i64);
    let within = fb.ins().band_imm(idx, CHUNK_MASK);
    let row = fb.ins().imul_imm(shard, CHUNKS as i64);
    let entry = fb.ins().iadd(row, chunk);
    let boff = fb.ins().imul_imm(entry, 8);
    let baddr = fb.ins().iadd(chunk_table, boff);
    let base = fb.ins().load(types::I64, t, baddr, 0);
    let off = fb.ins().imul_imm(within, stride);
    fb.ins().iadd(base, off)
}

/// Unbox a field word to `f64`. Guarded = the honest cost for a `Tagged` slot.
fn unbox_field(fb: &mut FunctionBuilder, im: &Imports, w: Value, guarded: bool) -> Value {
    if !guarded {
        return emit_unbox_double(fb, w);
    }
    let fast = fb.create_block();
    let slow = fb.create_block();
    let join = fb.create_block();
    fb.append_block_param(join, types::F64);

    let isd = emit_is_double(fb, w);
    fb.ins().brif(isd, fast, &[], slow, &[]);

    fb.switch_to_block(fast);
    fb.seal_block(fast);
    let f = emit_unbox_double(fb, w);
    fb.ins().jump(join, &[f.into()]);

    // Reachable: a tagged int32 / singleton / handle word lands here.
    fb.switch_to_block(slow);
    fb.seal_block(slow);
    let bits = call1(fb, im["probe_to_number"], &[w]);
    let f2 = emit_unbox_double(fb, bits);
    fb.ins().jump(join, &[f2.into()]);

    fb.switch_to_block(join);
    fb.seal_block(join);
    fb.block_params(join)[0]
}

fn emit_loop(fb: &mut FunctionBuilder, im: &Imports, v: V) {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let iters = fb.block_params(entry)[0];
    let hdr = fb.block_params(entry)[1];
    let mask = fb.block_params(entry)[2];

    let t = MemFlags::trusted();
    // hdr = [payload_array_ptr, shard_base_table_addr] — every row pays the
    // same dependent load of the payload word, exactly like kernels A and M.
    let payload_arr = fb.ins().load(types::I64, t, hdr, 0);
    let base_table = fb.ins().load(types::I64, t, hdr, 8);

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
    let idx = fb.ins().band(i, mask);
    let poff = fb.ins().imul_imm(idx, 8);
    let paddr = fb.ins().iadd(payload_arr, poff);
    let payload = fb.ins().load(types::I64, t, paddr, 0);

    let s_next = if v.src == Src::Scalarized {
        // x = idx, y = idx + 1 — the values the objects hold, so the checksum
        // must match every other row. No payload load, no object.
        let xf = fb.ins().fcvt_from_sint(types::F64, idx);
        let one = fb.ins().f64const(1.0);
        let yf = fb.ins().fadd(xf, one);
        let sum = fb.ins().fadd(xf, yf);
        fb.ins().fadd(s, sum)
    } else {
        let (wx, wy) = match v.src {
            Src::LockedCall | Src::UnlockedCall | Src::InlineCall => {
                let sym = match v.src {
                    Src::LockedCall => "probe_vec_get_locked",
                    Src::UnlockedCall => "probe_vec_get_unlocked",
                    _ => "probe_inline_get",
                };
                let sx = fb.ins().iconst(types::I64, SLOT_X);
                let x = call1(fb, im[sym], &[payload, sx]);
                let sy = fb.ins().iconst(types::I64, SLOT_Y);
                let y = call1(fb, im[sym], &[payload, sy]);
                (x, y)
            }
            Src::InlineLoad => {
                let obj = handle_to_addr(fb, base_table, payload, STRIDE_BYTES);
                let x = fb.ins().load(types::I64, t, obj, (8 * SLOT_X) as i32);
                let y = fb.ins().load(types::I64, t, obj, (8 * SLOT_Y) as i32);
                (x, y)
            }
            Src::ChunkedLoad => {
                let obj = handle_to_addr_chunked(fb, base_table, payload, STRIDE_BYTES);
                let x = fb.ins().load(types::I64, t, obj, (8 * SLOT_X) as i32);
                let y = fb.ins().load(types::I64, t, obj, (8 * SLOT_Y) as i32);
                (x, y)
            }
            Src::MovableLoad | Src::MovableRegion => {
                // One region ⇒ the slot table's base is an immediate AND the
                // payload spends no bits on shard routing, so it IS the index.
                // Sharded ⇒ decode shard/idx and load the shard's table.
                let (stab, idx) = if v.src == Src::MovableRegion {
                    let b = fb.ins().iconst(
                        types::I64,
                        crate::slab::blocks::moving_slab::region_slot_table_addr(),
                    );
                    (b, payload)
                } else {
                    let shard = fb.ins().band_imm(payload, (crate::slab::N_SHARDS - 1) as i64);
                    let idx = fb.ins().ushr_imm(payload, crate::slab::SHARD_BITS as i64);
                    let boff = fb.ins().imul_imm(shard, 8);
                    let baddr = fb.ins().iadd(base_table, boff);
                    (fb.ins().load(types::I64, t, baddr, 0), idx)
                };
                let soff = fb.ins().imul_imm(idx, 8);
                let saddr = fb.ins().iadd(stab, soff);
                // THE indirection: the slot holds the block's current address,
                // so relocating the object rewrites this one word.
                let block = fb.ins().load(types::I64, t, saddr, 0);
                let x = fb.ins().load(types::I64, t, block, (8 * SLOT_X) as i32);
                let y = fb.ins().load(types::I64, t, block, (8 * SLOT_Y) as i32);
                (x, y)
            }
            Src::PackedLoad => {
                let obj = handle_to_addr(fb, base_table, payload, packed_slab::STRIDE_BYTES);
                let x = fb.ins().load(types::I64, t, obj, (8 * SLOT_X) as i32);
                let y = fb.ins().load(types::I64, t, obj, (8 * SLOT_Y) as i32);
                (x, y)
            }
            Src::RegionLoad => {
                // No shard routing and no table load: the base is an immediate,
                // which is what a reserved region register would give.
                let base = fb.ins().iconst(types::I64, region_slab::base_addr());
                let off = fb.ins().imul_imm(payload, region_slab::STRIDE_BYTES);
                let obj = fb.ins().iadd(base, off);
                let x = fb.ins().load(types::I64, t, obj, (8 * SLOT_X) as i32);
                let y = fb.ins().load(types::I64, t, obj, (8 * SLOT_Y) as i32);
                (x, y)
            }
            Src::OverflowLoad => {
                let obj = handle_to_addr(fb, base_table, payload, STRIDE_BYTES);
                let bag = fb
                    .ins()
                    .load(types::I64, t, obj, (8 * OVERFLOW_WORD as i64) as i32);
                let x = fb.ins().load(types::I64, t, bag, 0);
                let y = fb.ins().load(types::I64, t, bag, 8);
                (x, y)
            }
            Src::Scalarized => unreachable!(),
        };
        let xf = unbox_field(fb, im, wx, v.guarded);
        let yf = unbox_field(fb, im, wy, v.guarded);
        let sum = fb.ins().fadd(xf, yf);
        fb.ins().fadd(s, sum)
    };

    let i_next = fb.ins().iadd_imm(i, 1);
    fb.ins().jump(header, &[i_next.into(), s_next.into()]);
    fb.seal_block(header);

    fb.switch_to_block(exit);
    fb.seal_block(exit);
    let out = fb.block_params(exit)[0];
    let raw = emit_box_double(fb, out);
    fb.ins().return_(&[raw]);
}
