//! Kernel A — the objbench inner loop, no allocation:
//! `for i in 0..n { s = s + p.x * p.y }` over an array of pre-built objects
//! (`p = objs[i & mask]`, so nothing is loop-invariant and no load can be
//! hoisted out — the failure mode that would make the no-call variants lie).
//!
//! Six variants, each removing exactly ONE thing from the one before it, so the
//! delta between two adjacent rows is that one lever and nothing else.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{
    Compiled, Imports, call1, compile, emit_box_double, emit_is_double, emit_unbox_double,
    loop_kernel,
};

const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const VEC_GET_UNLOCKED: (&str, usize) = ("probe_vec_get_unlocked", 2);
const ADP_MUL: (&str, usize) = ("probe_adp_mul", 2);
const ADP_ADD: (&str, usize) = ("probe_adp_add", 2);

/// Object layout: `[shape_id, x, y]` — slot 0 is the shape word, exactly like
/// `obj.rs:97-116`, so a field lives at `1 + slot_index`.
const SLOT_X: i64 = 1;
const SLOT_Y: i64 = 2;

/// Read a field through the LOCKED trampoline — one shard `Mutex` per touch.
fn field_locked(fb: &mut FunctionBuilder, im: &Imports, payload: Value, slot: i64) -> Value {
    let idx = fb.ins().iconst(types::I64, slot);
    call1(fb, im["probe_vec_get_locked"], &[payload, idx])
}

fn field_unlocked(fb: &mut FunctionBuilder, im: &Imports, payload: Value, slot: i64) -> Value {
    let idx = fb.ins().iconst(types::I64, slot);
    call1(fb, im["probe_vec_get_unlocked"], &[payload, idx])
}

// ---------------------------------------------------------------------------
// A0 — TODAY. Locked call per field, generic call per operator.
// ---------------------------------------------------------------------------

pub fn a0_current() -> Compiled {
    loop_kernel(
        "a0_current",
        &[VEC_GET_LOCKED, ADP_MUL, ADP_ADD],
        types::I64,
        |fb, im, v| {
            let x = field_locked(fb, im, v.payload, SLOT_X);
            let y = field_locked(fb, im, v.payload, SLOT_Y);
            let m = call1(fb, im["probe_adp_mul"], &[x, y]);
            call1(fb, im["probe_adp_add"], &[v.s, m])
        },
    )
}

// ---------------------------------------------------------------------------
// A1 — L0+L1: the shape carries a per-slot Repr, so a proven-double field read
// stays Float64 and the arithmetic is INLINE. Field access unchanged (locked).
// ---------------------------------------------------------------------------

pub fn a1_proven_repr() -> Compiled {
    loop_kernel(
        "a1_proven_repr",
        &[VEC_GET_LOCKED],
        types::F64,
        |fb, im, v| {
            let x = field_locked(fb, im, v.payload, SLOT_X);
            let y = field_locked(fb, im, v.payload, SLOT_Y);
            let xf = emit_unbox_double(fb, x);
            let yf = emit_unbox_double(fb, y);
            let m = fb.ins().fmul(xf, yf);
            fb.ins().fadd(v.s, m)
        },
    )
}

// ---------------------------------------------------------------------------
// A2 — L2 alone: keep the Tagged accumulator and the generic operator, but put
// the inline tag-check fast path in front of it (design doc §7, not implemented
// today — `binop.rs:596` calls unconditionally).
// ---------------------------------------------------------------------------

pub fn a2_inline_guard() -> Compiled {
    loop_kernel(
        "a2_inline_guard",
        &[VEC_GET_LOCKED, ADP_MUL, ADP_ADD],
        types::I64,
        |fb, im, v| {
            let x = field_locked(fb, im, v.payload, SLOT_X);
            let y = field_locked(fb, im, v.payload, SLOT_Y);
            let m = guarded_binop(fb, im["probe_adp_mul"], x, y, true);
            guarded_binop(fb, im["probe_adp_add"], v.s, m, false)
        },
    )
}

/// `if is_double(a) && is_double(b) { inline op } else { call generic }`.
/// The slow arm is emitted and reachable — this is a guard, not a bet.
fn guarded_binop(
    fb: &mut FunctionBuilder,
    generic: cranelift_codegen::ir::FuncRef,
    a: Value,
    b: Value,
    is_mul: bool,
) -> Value {
    let fast = fb.create_block();
    let slow = fb.create_block();
    let join = fb.create_block();
    fb.append_block_param(join, types::I64);

    let da = emit_is_double(fb, a);
    let db = emit_is_double(fb, b);
    let both = fb.ins().band(da, db);
    fb.ins().brif(both, fast, &[], slow, &[]);

    fb.switch_to_block(fast);
    fb.seal_block(fast);
    let af = emit_unbox_double(fb, a);
    let bf = emit_unbox_double(fb, b);
    let r = if is_mul {
        fb.ins().fmul(af, bf)
    } else {
        fb.ins().fadd(af, bf)
    };
    let rw = emit_box_double(fb, r);
    fb.ins().jump(join, &[rw.into()]);

    fb.switch_to_block(slow);
    fb.seal_block(slow);
    let rs = call1(fb, generic, &[a, b]);
    fb.ins().jump(join, &[rs.into()]);

    fb.switch_to_block(join);
    fb.seal_block(join);
    fb.block_params(join)[0]
}

// ---------------------------------------------------------------------------
// A3 — L3a: A1 with the shard Mutex removed from the field read. Still a call.
// The A1→A3 delta is the LOCK; the A3→A4 delta is the CALL + indirection.
// ---------------------------------------------------------------------------

pub fn a3_unlocked_call() -> Compiled {
    loop_kernel(
        "a3_unlocked_call",
        &[VEC_GET_UNLOCKED],
        types::F64,
        |fb, im, v| {
            let x = field_unlocked(fb, im, v.payload, SLOT_X);
            let y = field_unlocked(fb, im, v.payload, SLOT_Y);
            let xf = emit_unbox_double(fb, x);
            let yf = emit_unbox_double(fb, y);
            let m = fb.ins().fmul(xf, yf);
            fb.ins().fadd(v.s, m)
        },
    )
}

// ---------------------------------------------------------------------------
// A4 — L3b: the payload is an ARENA OFFSET, so the field address is
// `base + (payload + 1 + slot)*8` — pure IR, no call, no lock. This is what
// "shape-id compare + fixed-offset load" actually costs when it IS a load.
// ---------------------------------------------------------------------------

pub fn a4_raw_load() -> Compiled {
    loop_kernel("a4_raw_load", &[], types::F64, |fb, _im, v| {
        let trusted = MemFlags::trusted();
        let base_off = fb.ins().imul_imm(v.payload, 8);
        let obj = fb.ins().iadd(v.arena_base, base_off);
        let x = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_X) as i32);
        let y = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_Y) as i32);
        let xf = emit_unbox_double(fb, x);
        let yf = emit_unbox_double(fb, y);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}

// ---------------------------------------------------------------------------
// A4g — A4 plus the shape guard a real inline cache must emit: compare slot 0
// against the expected shape id before trusting the fixed offsets. This is the
// honest cost of a correct IC hit, not the no-guard ceiling.
// ---------------------------------------------------------------------------

pub fn a4g_raw_load_guarded(shape_id: i64) -> Compiled {
    let name = "a4g_raw_load_guarded";
    compile(name, &[], move |fb, _im| {
        // Reuse the same skeleton by hand: the guard needs a branch inside the
        // body, which the closure form supports, so just delegate.
        build_guarded_raw(fb, shape_id);
    })
}

fn build_guarded_raw(fb: &mut FunctionBuilder, shape_id: i64) {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let iters = fb.block_params(entry)[0];
    let hdr_ptr = fb.block_params(entry)[1];
    let mask = fb.block_params(entry)[2];
    let trusted = MemFlags::trusted();
    let payload_arr = fb.ins().load(types::I64, trusted, hdr_ptr, 0);
    let arena_base = fb.ins().load(types::I64, trusted, hdr_ptr, 8);

    let header = fb.create_block();
    fb.append_block_param(header, types::I64);
    fb.append_block_param(header, types::F64);
    let lbody = fb.create_block();
    let miss = fb.create_block();
    let cont = fb.create_block();
    fb.append_block_param(cont, types::F64);
    let exit = fb.create_block();
    fb.append_block_param(exit, types::F64);

    let zero = fb.ins().iconst(types::I64, 0);
    let s0 = fb.ins().f64const(0.0);
    fb.ins().jump(header, &[zero.into(), s0.into()]);

    fb.switch_to_block(header);
    let i = fb.block_params(header)[0];
    let s = fb.block_params(header)[1];
    let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
    fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

    fb.switch_to_block(lbody);
    fb.seal_block(lbody);
    let idx = fb.ins().band(i, mask);
    let off = fb.ins().imul_imm(idx, 8);
    let addr = fb.ins().iadd(payload_arr, off);
    let payload = fb.ins().load(types::I64, trusted, addr, 0);
    let base_off = fb.ins().imul_imm(payload, 8);
    let obj = fb.ins().iadd(arena_base, base_off);
    // The IC guard: slot 0 must be the shape this site was specialized for.
    let shape = fb.ins().load(types::I64, trusted, obj, 0);
    let want = fb.ins().iconst(types::I64, shape_id);
    let hit = fb.ins().icmp(IntCC::Equal, shape, want);
    let fast = fb.create_block();
    fb.ins().brif(hit, fast, &[], miss, &[]);

    fb.switch_to_block(fast);
    fb.seal_block(fast);
    let x = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_X) as i32);
    let y = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_Y) as i32);
    let xf = emit_unbox_double(fb, x);
    let yf = emit_unbox_double(fb, y);
    let m = fb.ins().fmul(xf, yf);
    let s1 = fb.ins().fadd(s, m);
    fb.ins().jump(cont, &[s1.into()]);

    // Miss arm: reachable, so the branch is real. Never taken by this workload.
    fb.switch_to_block(miss);
    fb.seal_block(miss);
    let nanv = fb.ins().f64const(f64::NAN);
    let s2 = fb.ins().fadd(s, nanv);
    fb.ins().jump(cont, &[s2.into()]);

    fb.switch_to_block(cont);
    fb.seal_block(cont);
    let s_next = fb.block_params(cont)[0];
    let i_next = fb.ins().iadd_imm(i, 1);
    fb.ins().jump(header, &[i_next.into(), s_next.into()]);
    fb.seal_block(header);

    fb.switch_to_block(exit);
    fb.seal_block(exit);
    let out = fb.block_params(exit)[0];
    let raw = emit_box_double(fb, out);
    fb.ins().return_(&[raw]);
}

// ---------------------------------------------------------------------------
// A5 — L4: escape analysis removed the object entirely. The floor for this loop.
// ---------------------------------------------------------------------------

pub fn a5_scalarized() -> Compiled {
    loop_kernel("a5_scalarized", &[], types::F64, |fb, _im, v| {
        // `x = idx`, `y = idx + 1` — the same values the objects hold (see
        // `main.rs::build_objects`), so this variant's checksum must match every
        // other variant's. It does no payload-array load at all: that IS what
        // eliminating the object buys.
        let xf = fb.ins().fcvt_from_sint(types::F64, v.idx);
        let one = fb.ins().f64const(1.0);
        let yf = fb.ins().fadd(xf, one);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}
