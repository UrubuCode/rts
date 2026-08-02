//! Kernel M — the METHOD + `this` ladder.
//!
//! Measured in the real engine first (release, 3M iters, `p` a `class P {x,y}`):
//!
//! ```text
//! free fn call        6 ms →  2.0 ns   a direct user-fn call is already cheap
//! p.x                39 ms → 13.0 ns   ONE field read
//! p.getx()           43 ms → 14.3 ns   the method adds ~1.3 ns over the field
//! p.sum()  (x + y)   79 ms → 26.3 ns   exactly 2 field reads
//! ```
//!
//! and the emitted IR for the method body is
//!
//! ```text
//! v3 = band v0, 0xffff_ffff_ffff        ; untag `this`
//! v4 = call fn0(v3, 1)                  ; __rtsn_vec_get_by_payload -> shard Mutex
//! ```
//!
//! So the hypothesis this kernel tests is NOT "dispatch is slow". It is: **the
//! method call and the `this` tagging are already near-free; everything a class
//! costs is the per-`this.field` opaque call + shard lock.** Each row removes
//! exactly ONE thing from the row above it, and the CALL to the method is a real
//! Cranelift call to a second compiled function (not an inlined closure) for
//! every row except M4/M5, where inlining IS the lever under test.
//!
//! Workload per iteration: `s = s + p.sum()` where `sum() { return this.x + this.y }`
//! and `p = objs[i & mask]`, so nothing is loop-invariant.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, FuncRef, InstBuilder, MemFlags, Signature, Value, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use super::{Compiled, KernelFn, call1, emit_box_double, emit_unbox_double};

const SLOT_X: i64 = 1;
const SLOT_Y: i64 = 2;

/// How the method receives `this`.
#[derive(Clone, Copy, PartialEq)]
enum This {
    /// A tagged PolyValue word — the method must `band` off the 48-bit payload
    /// on entry, exactly like `__rtsn_method_P_getx` does today.
    Tagged,
    /// The raw 48-bit payload, already proven to be an object (`Repr::Ref`).
    Payload,
    /// The object's ADDRESS, in a register. No slab indirection at all.
    Addr,
}

/// How the method reads a field of `this`.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Locked,
    Unlocked,
    /// `load [this + 8*(1+slot)]` — pure IR.
    Load,
}

/// How the method's result is combined — the operator ladder, kept in lockstep
/// with kernel A so the two tables are comparable.
#[derive(Clone, Copy, PartialEq)]
enum Arith {
    Generic,
    Inline,
}

#[derive(Clone, Copy)]
struct Variant {
    this: This,
    field: Field,
    arith: Arith,
    /// The method body is emitted at the call site instead of being called.
    inlined: bool,
    /// The inlined body guards on the shape word (slot 0) before trusting the
    /// fixed offsets — the honest cost of a correct IC hit.
    guard: bool,
    /// The object is gone entirely (escape analysis); fields come from `idx`.
    scalarized: bool,
}

const BASE: Variant = Variant {
    this: This::Tagged,
    field: Field::Locked,
    arith: Arith::Generic,
    inlined: false,
    guard: false,
    scalarized: false,
};

pub fn m0_today() -> Compiled {
    build("m0_today", BASE)
}

pub fn m1_inline_arith() -> Compiled {
    build(
        "m1_inline_arith",
        Variant {
            arith: Arith::Inline,
            ..BASE
        },
    )
}

pub fn m2_untagged_this() -> Compiled {
    build(
        "m2_untagged_this",
        Variant {
            this: This::Payload,
            arith: Arith::Inline,
            ..BASE
        },
    )
}

pub fn m3_no_lock() -> Compiled {
    build(
        "m3_no_lock",
        Variant {
            this: This::Payload,
            field: Field::Unlocked,
            arith: Arith::Inline,
            ..BASE
        },
    )
}

pub fn m4_this_in_register() -> Compiled {
    build(
        "m4_this_in_register",
        Variant {
            this: This::Addr,
            field: Field::Load,
            arith: Arith::Inline,
            ..BASE
        },
    )
}

pub fn m5_inlined_guarded(_shape: i64) -> Compiled {
    build(
        "m5_inlined_guarded",
        Variant {
            this: This::Addr,
            field: Field::Load,
            arith: Arith::Inline,
            inlined: true,
            guard: true,
            ..BASE
        },
    )
}

pub fn m6_scalarized() -> Compiled {
    build(
        "m6_scalarized",
        Variant {
            this: This::Addr,
            field: Field::Load,
            arith: Arith::Inline,
            inlined: true,
            guard: false,
            scalarized: true,
        },
    )
}

/// The shape id every object carries — must match what the driver allocates.
pub const SHAPE_ID: i64 = 7;

// ---------------------------------------------------------------------------
// Module construction: TWO functions, so the method call is a REAL call.
// ---------------------------------------------------------------------------

fn build(name: &str, v: Variant) -> Compiled {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    flags.set("enable_verifier", "false").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa builder")
        .finish(settings::Flags::new(flags))
        .expect("finish host isa");

    let mut jb = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for (sym, ptr) in crate::rt::symbols() {
        jb.symbol(sym, ptr);
    }
    let mut module = JITModule::new(jb);
    let cc = module.isa().default_call_conv();

    // Runtime imports the method body may need.
    let needed: &[(&str, usize)] = &[
        ("probe_vec_get_locked", 2),
        ("probe_vec_get_unlocked", 2),
        ("probe_adp_add", 2),
    ];
    let mut import_ids = Vec::new();
    for (sym, arity) in needed {
        let mut sig = Signature::new(cc);
        for _ in 0..*arity {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let id = module
            .declare_function(sym, Linkage::Import, &sig)
            .expect("declare import");
        import_ids.push((*sym, id));
    }

    // The method: `(this) -> value`. Same one-i64-in/one-i64-out shape the real
    // `__rtsn_method_P_sum` has.
    let mut msig = Signature::new(cc);
    msig.params.push(AbiParam::new(types::I64));
    msig.returns.push(AbiParam::new(types::I64));
    let method_id = module
        .declare_function(&format!("{name}_method"), Linkage::Local, &msig)
        .expect("declare method");

    let mut mctx = module.make_context();
    mctx.func.signature = msig;
    {
        let mut fbc = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut mctx.func, &mut fbc);
        let imports: super::Imports = import_ids
            .iter()
            .map(|(s, id)| (*s, module.declare_func_in_func(*id, fb.func)))
            .collect();
        emit_method(&mut fb, &imports, v);
        fb.finalize();
    }
    module.define_function(method_id, &mut mctx).expect("method");
    module.clear_context(&mut mctx);

    // The caller loop: `(iters, hdr_ptr, mask) -> word`.
    let mut ksig = Signature::new(cc);
    for _ in 0..3 {
        ksig.params.push(AbiParam::new(types::I64));
    }
    ksig.returns.push(AbiParam::new(types::I64));
    let kernel_id = module
        .declare_function(name, Linkage::Local, &ksig)
        .expect("declare kernel");

    let mut kctx = module.make_context();
    kctx.func.signature = ksig;
    {
        let mut fbc = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut kctx.func, &mut fbc);
        let imports: super::Imports = import_ids
            .iter()
            .map(|(s, id)| (*s, module.declare_func_in_func(*id, fb.func)))
            .collect();
        let mref = module.declare_func_in_func(method_id, fb.func);
        emit_caller(&mut fb, &imports, mref, v);
        fb.finalize();
    }
    module.define_function(kernel_id, &mut kctx).expect("kernel");
    module.clear_context(&mut kctx);
    module.finalize_definitions().expect("finalize");

    let code = module.get_finalized_function(kernel_id);
    // SAFETY: signature matches `KernelFn`; `_module` keeps the code mapped.
    let f: KernelFn = unsafe { std::mem::transmute(code) };
    Compiled { _module: module, f }
}

// ---------------------------------------------------------------------------
// The method body — shared by the real-call form and the inlined form.
// ---------------------------------------------------------------------------

/// Reads `this.x` and `this.y` and returns their sum. `recv` is whatever `This`
/// says it is; `arena_base` is only needed for `This::Addr` construction at the
/// call site (the method itself already receives an address in that mode).
fn emit_body(fb: &mut FunctionBuilder, im: &super::Imports, v: Variant, recv: Value) -> Value {
    let this = match v.this {
        // Today: the method unpacks the 48-bit payload out of the tagged word.
        This::Tagged => {
            let mask = fb.ins().iconst(types::I64, 0xffff_ffff_ffff);
            fb.ins().band(recv, mask)
        }
        This::Payload | This::Addr => recv,
    };

    let (x, y) = match v.field {
        Field::Locked => {
            let sx = fb.ins().iconst(types::I64, SLOT_X);
            let x = call1(fb, im["probe_vec_get_locked"], &[this, sx]);
            let sy = fb.ins().iconst(types::I64, SLOT_Y);
            let y = call1(fb, im["probe_vec_get_locked"], &[this, sy]);
            (x, y)
        }
        Field::Unlocked => {
            let sx = fb.ins().iconst(types::I64, SLOT_X);
            let x = call1(fb, im["probe_vec_get_unlocked"], &[this, sx]);
            let sy = fb.ins().iconst(types::I64, SLOT_Y);
            let y = call1(fb, im["probe_vec_get_unlocked"], &[this, sy]);
            (x, y)
        }
        Field::Load => {
            let t = MemFlags::trusted();
            let x = fb.ins().load(types::I64, t, this, (8 * SLOT_X) as i32);
            let y = fb.ins().load(types::I64, t, this, (8 * SLOT_Y) as i32);
            (x, y)
        }
    };

    match v.arith {
        Arith::Generic => call1(fb, im["probe_adp_add"], &[x, y]),
        Arith::Inline => {
            let xf = emit_unbox_double(fb, x);
            let yf = emit_unbox_double(fb, y);
            let sum = fb.ins().fadd(xf, yf);
            emit_box_double(fb, sum)
        }
    }
}

fn emit_method(fb: &mut FunctionBuilder, im: &super::Imports, v: Variant) {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let recv = fb.block_params(entry)[0];
    let out = emit_body(fb, im, v, recv);
    fb.ins().return_(&[out]);
}

// ---------------------------------------------------------------------------
// The caller loop.
// ---------------------------------------------------------------------------

fn emit_caller(fb: &mut FunctionBuilder, im: &super::Imports, method: FuncRef, v: Variant) {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let iters = fb.block_params(entry)[0];
    let hdr = fb.block_params(entry)[1];
    let mask = fb.block_params(entry)[2];

    let t = MemFlags::trusted();
    let payload_arr = fb.ins().load(types::I64, t, hdr, 0);
    let arena_base = fb.ins().load(types::I64, t, hdr, 8);

    // The accumulator is F64 whenever the arithmetic is inline (proven Repr),
    // and a Tagged i64 in the M0 row — same convention kernel A uses.
    let s_ty = if v.arith == Arith::Generic {
        types::I64
    } else {
        types::F64
    };

    let header = fb.create_block();
    fb.append_block_param(header, types::I64);
    fb.append_block_param(header, s_ty);
    let body = fb.create_block();
    let exit = fb.create_block();
    fb.append_block_param(exit, s_ty);

    let zero = fb.ins().iconst(types::I64, 0);
    let s0 = if s_ty == types::F64 {
        fb.ins().f64const(0.0)
    } else {
        fb.ins().iconst(types::I64, 0)
    };
    fb.ins().jump(header, &[zero.into(), s0.into()]);

    fb.switch_to_block(header);
    let i = fb.block_params(header)[0];
    let s = fb.block_params(header)[1];
    let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
    fb.ins().brif(go, body, &[], exit, &[s.into()]);

    fb.switch_to_block(body);
    fb.seal_block(body);
    let idx = fb.ins().band(i, mask);
    let off = fb.ins().imul_imm(idx, 8);
    let addr = fb.ins().iadd(payload_arr, off);
    let payload = fb.ins().load(types::I64, t, addr, 0);

    // What the call site hands the method as `this`.
    let recv = match v.this {
        This::Tagged => {
            // Re-tag: the real site holds a PolyValue OBJECT word, so the tag
            // bits are set and the callee has to mask them off.
            let base = fb.ins().iconst(types::I64, crate::poly::BOX_BASE as i64);
            let tag = fb.ins().iconst(types::I64, (4i64) << 48); // OBJECT tag
            let t0 = fb.ins().bor(payload, base);
            fb.ins().bor(t0, tag)
        }
        This::Payload => payload,
        This::Addr => {
            let o = fb.ins().imul_imm(payload, 8);
            fb.ins().iadd(arena_base, o)
        }
    };

    let s_next = if v.scalarized {
        // Escape analysis: the object never existed. x = idx, y = idx + 1.
        let xf = fb.ins().fcvt_from_sint(types::F64, idx);
        let one = fb.ins().f64const(1.0);
        let yf = fb.ins().fadd(xf, one);
        let sum = fb.ins().fadd(xf, yf);
        fb.ins().fadd(s, sum)
    } else if v.inlined {
        // The method body emitted at the site, behind the IC shape guard.
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        if v.guard {
            let miss = fb.create_block();
            let fast = fb.create_block();
            let shape = fb.ins().load(types::I64, t, recv, 0);
            let want = fb.ins().iconst(types::I64, SHAPE_ID);
            let hit = fb.ins().icmp(IntCC::Equal, shape, want);
            fb.ins().brif(hit, fast, &[], miss, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let r = emit_body(fb, im, v, recv);
            let rf = emit_unbox_double(fb, r);
            let s1 = fb.ins().fadd(s, rf);
            fb.ins().jump(cont, &[s1.into()]);

            // Reachable, never taken by this workload — a guard, not a bet.
            fb.switch_to_block(miss);
            fb.seal_block(miss);
            let nanv = fb.ins().f64const(f64::NAN);
            let s2 = fb.ins().fadd(s, nanv);
            fb.ins().jump(cont, &[s2.into()]);
        } else {
            let r = emit_body(fb, im, v, recv);
            let rf = emit_unbox_double(fb, r);
            let s1 = fb.ins().fadd(s, rf);
            fb.ins().jump(cont, &[s1.into()]);
        }
        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    } else {
        let r = call1(fb, method, &[recv]);
        match v.arith {
            Arith::Generic => call1(fb, im["probe_adp_add"], &[s, r]),
            Arith::Inline => {
                let rf = emit_unbox_double(fb, r);
                fb.ins().fadd(s, rf)
            }
        }
    };

    let i_next = fb.ins().iadd_imm(i, 1);
    fb.ins().jump(header, &[i_next.into(), s_next.into()]);
    fb.seal_block(header);

    fb.switch_to_block(exit);
    fb.seal_block(exit);
    let out = fb.block_params(exit)[0];
    let raw = if s_ty == types::F64 {
        emit_box_double(fb, out)
    } else {
        out
    };
    fb.ins().return_(&[raw]);
}
