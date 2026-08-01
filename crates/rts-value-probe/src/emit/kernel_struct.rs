//! Kernel STRUCT — does a STATIC STRUCT LAYOUT actually beat the shaped object?
//!
//! This tests the §1c "S1+S2" proposal directly: `class P { x: number; y: number }`
//! compiled as a struct (compile-time offsets, fields stored at their declared
//! type, no shape word, no tag) versus what the engine does today.
//!
//! The suspicion this kernel exists to confirm or kill: **for a `number` field a
//! NaN-boxed double IS the raw `f64` bits**, so "unboxing the field" may be a
//! no-op at rest, and S2's entire win over a direct-addressed shaped object may
//! be just the shape word plus the guard. If so, S2 is a much smaller change
//! than §1c implies and the plan must say so.
//!
//! Layouts under test, all in the same flat arena so addressing is identical:
//!
//! ```text
//! shaped : [shape_id][x:PolyValue][y:PolyValue]   3 words, slot 0 is the shape
//! struct : [x:f64][y:f64]                          2 words, no header at all
//! guarded: [class_id][x:f64][y:f64]                3 words, header is a class id
//! ```

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};

use super::{Compiled, LoopVals, call1, emit_unbox_double, loop_kernel};

const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const ADP_MUL: (&str, usize) = ("probe_adp_mul", 2);
const ADP_ADD: (&str, usize) = ("probe_adp_add", 2);

/// Shaped object: slot 0 = shape id, fields at 1 + index.
const SHAPED_X: i32 = 8;
const SHAPED_Y: i32 = 16;
/// Struct: no header, fields at 0 and 8.
const STRUCT_X: i32 = 0;
const STRUCT_Y: i32 = 8;
/// Guarded struct: class id at 0, fields after it.
const GUARDED_X: i32 = 8;
const GUARDED_Y: i32 = 16;

/// T0 — today: shaped object in the locked slab, generic arithmetic.
pub fn t0_today() -> Compiled {
    loop_kernel(
        "struct_t0_today",
        &[VEC_GET_LOCKED, ADP_MUL, ADP_ADD],
        types::I64,
        |fb, im, v| {
            let ix = fb.ins().iconst(types::I64, 1);
            let iy = fb.ins().iconst(types::I64, 2);
            let x = call1(fb, im["probe_vec_get_locked"], &[v.payload, ix]);
            let y = call1(fb, im["probe_vec_get_locked"], &[v.payload, iy]);
            let m = call1(fb, im["probe_adp_mul"], &[x, y]);
            call1(fb, im["probe_adp_add"], &[v.s, m])
        },
    )
}

/// T1 — Tier 3's outcome: SHAPED object, but direct-addressed and with inline
/// arithmetic. Fields are still PolyValue words, unboxed with a `bitcast`.
pub fn t1_shaped_direct() -> Compiled {
    loop_kernel("struct_t1_shaped", &[], types::F64, |fb, _im, v| {
        let obj = obj_addr(fb, v.arena_base, v.payload);
        let trusted = MemFlags::trusted();
        let x = fb.ins().load(types::I64, trusted, obj, SHAPED_X);
        let y = fb.ins().load(types::I64, trusted, obj, SHAPED_Y);
        let xf = emit_unbox_double(fb, x);
        let yf = emit_unbox_double(fb, y);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}

/// T2 — the S1+S2 proposal: STATIC STRUCT. No shape word, no tag; the field is
/// loaded **typed `f64` directly**, which is the whole claim.
pub fn t2_static_struct() -> Compiled {
    loop_kernel("struct_t2_struct", &[], types::F64, |fb, _im, v| {
        let obj = obj_addr(fb, v.arena_base, v.payload);
        let trusted = MemFlags::trusted();
        let xf = fb.ins().load(types::F64, trusted, obj, STRUCT_X);
        let yf = fb.ins().load(types::F64, trusted, obj, STRUCT_Y);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}

/// T2g — the HONEST version of T2. JS semantics do not let a declared layout be
/// trusted unconditionally (`p` may be an `any` that holds something else, the
/// program may have added a property, `delete` may have fired), so a real
/// implementation still checks a class id before using the fixed offsets. This
/// is the cost of keeping the language's semantics on top of the struct.
pub fn t2g_struct_guarded(class_id: i64) -> Compiled {
    loop_kernel("struct_t2g_guarded", &[], types::F64, move |fb, _im, v| {
        let obj = obj_addr(fb, v.arena_base, v.payload);
        let trusted = MemFlags::trusted();
        let cid = fb.ins().load(types::I64, trusted, obj, 0);
        let want = fb.ins().iconst(types::I64, class_id);
        let ok = fb.ins().icmp(IntCC::Equal, cid, want);

        let fast = fb.create_block();
        let miss = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(ok, fast, &[], miss, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let xf = fb.ins().load(types::F64, trusted, obj, GUARDED_X);
        let yf = fb.ins().load(types::F64, trusted, obj, GUARDED_Y);
        let m = fb.ins().fmul(xf, yf);
        let s1 = fb.ins().fadd(v.s, m);
        fb.ins().jump(cont, &[s1.into()]);

        fb.switch_to_block(miss);
        fb.seal_block(miss);
        let nan = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(v.s, nan);
        fb.ins().jump(cont, &[s2.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    })
}

/// T3 — scalar replacement: the object never existed. The ceiling.
pub fn t3_scalarized() -> Compiled {
    loop_kernel("struct_t3_scalar", &[], types::F64, |fb, _im, v| {
        let xf = fb.ins().fcvt_from_sint(types::F64, v.idx);
        let one = fb.ins().f64const(1.0);
        let yf = fb.ins().fadd(xf, one);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}

/// `arena_base + payload*8` — identical addressing in every direct variant, so
/// the comparison isolates the LAYOUT, not how the object is found.
fn obj_addr(fb: &mut cranelift_frontend::FunctionBuilder, base: Value, payload: Value) -> Value {
    let off = fb.ins().imul_imm(payload, 8);
    fb.ins().iadd(base, off)
}

#[allow(dead_code)]
const _: Option<LoopVals> = None;
