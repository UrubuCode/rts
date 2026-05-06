//! End-to-end tests for the MIR → Cranelift lowering.
//!
//! Each test builds a `MirFunc` by hand (or via `rts_mir::lower::lower_func`),
//! lowers it to a `JITModule`, finalizes, then transmutes the function pointer
//! and runs the compiled code, asserting on the return value.
//!
//! These tests exercise the actual Cranelift backend so they catch real
//! mismatches between MIR semantics and Cranelift IR semantics — bugs that
//! pure structural verification (`rts_mir::passes::verify`) cannot detect.

use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::{JITBuilder, JITModule};
use rts_hir::ir::{HirBinOp, HirExpr, HirExprKind, HirFunc, HirLit, HirParam, HirStmt, HirType};
use rts_mir::ir::*;

use super::lower::lower_mir_func;

/// Build a JIT module configured for the host. Fast call conv is the
/// default (Windows fastcall on Windows, SystemV on Linux). We override
/// MIR's CallConvHint::Tail to use the host default to avoid the tail
/// call frame-pointer requirement here.
fn make_jit() -> JITModule {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();
    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    JITModule::new(builder)
}

/// Override conv to host default before lowering — JIT-friendly.
fn host_conv(mut mir: MirFunc) -> MirFunc {
    mir.conv = CallConvHint::SystemV;
    // On Windows the default is WindowsFastcall; pick whichever the host uses.
    if cfg!(windows) {
        mir.conv = CallConvHint::WindowsFastcall;
    }
    mir
}

fn lower_and_finalize(mir: MirFunc) -> (JITModule, cranelift_module::FuncId) {
    let mut module = make_jit();
    let id = lower_mir_func(&mut module, &mir).expect("lower_mir_func");
    module.finalize_definitions().expect("finalize");
    (module, id)
}

// ---------- direct MIR construction ----------

#[test]
fn jit_const_int_returns_42() {
    let mut mir = MirFunc::new("answer", CallConvHint::Tail, HirType::I64);
    let v = mir.new_value(HirType::I64);
    let blk = mir.new_block();
    mir.blocks[blk as usize].insts.push(Inst::IConst {
        dst: v,
        ty: HirType::I64,
        val: 42,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![v]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 42);
}

#[test]
fn jit_add_two_params() {
    // function add(a:i64, b:i64) -> i64 { return a + b; }
    let mut mir = MirFunc::new("add", CallConvHint::Tail, HirType::I64);
    let a = mir.new_value(HirType::I64);
    let b = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((a, HirType::I64));
    mir.params.push((b, HirType::I64));
    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((a, HirType::I64));
    mir.blocks[blk as usize].params.push((b, HirType::I64));
    mir.blocks[blk as usize]
        .insts
        .push(Inst::IAdd { dst: r, lhs: a, rhs: b });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(7, 35), 42);
    assert_eq!(f(-10, 10), 0);
}

#[test]
fn jit_iadd_imm() {
    // f(x) = x + 1
    let mut mir = MirFunc::new("inc", CallConvHint::Tail, HirType::I64);
    let x = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((x, HirType::I64));
    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((x, HirType::I64));
    mir.blocks[blk as usize].insts.push(Inst::IAddImm {
        dst: r,
        lhs: x,
        imm: 1,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(41), 42);
    assert_eq!(f(-1), 0);
}

#[test]
fn jit_imul_pow2_via_ishl() {
    // f(x) = x << 3  (= x * 8)
    let mut mir = MirFunc::new("mul8", CallConvHint::Tail, HirType::I64);
    let x = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((x, HirType::I64));
    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((x, HirType::I64));
    mir.blocks[blk as usize].insts.push(Inst::IShlImm {
        dst: r,
        lhs: x,
        imm: 3,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(5), 40);
    assert_eq!(f(7), 56);
}

#[test]
fn jit_fadd() {
    // f(a, b) = a + b (f64)
    let mut mir = MirFunc::new("fa", CallConvHint::Tail, HirType::F64);
    let a = mir.new_value(HirType::F64);
    let b = mir.new_value(HirType::F64);
    let r = mir.new_value(HirType::F64);
    mir.params.push((a, HirType::F64));
    mir.params.push((b, HirType::F64));
    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((a, HirType::F64));
    mir.blocks[blk as usize].params.push((b, HirType::F64));
    mir.blocks[blk as usize]
        .insts
        .push(Inst::FAdd { dst: r, lhs: a, rhs: b });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(ptr) };
    assert!((f(1.5, 2.5) - 4.0).abs() < 1e-12);
}

#[test]
fn jit_branch_returns_max() {
    // function pick_max(a:i64, b:i64) {
    //     if (a > b) return a; else return b;
    // }
    let mut mir = MirFunc::new("max", CallConvHint::Tail, HirType::I64);
    let a = mir.new_value(HirType::I64);
    let b = mir.new_value(HirType::I64);
    let cmp = mir.new_value(HirType::Bool);
    mir.params.push((a, HirType::I64));
    mir.params.push((b, HirType::I64));

    let entry = mir.new_block();
    let then_b = mir.new_block();
    let else_b = mir.new_block();
    mir.blocks[entry as usize].params.push((a, HirType::I64));
    mir.blocks[entry as usize].params.push((b, HirType::I64));

    mir.blocks[entry as usize].insts.push(Inst::ICmp {
        dst: cmp,
        cond: IntCond::Sgt,
        lhs: a,
        rhs: b,
    });
    mir.blocks[entry as usize].term = Terminator::Brif {
        cond: cmp,
        then_block: then_b,
        then_args: vec![],
        else_block: else_b,
        else_args: vec![],
    };
    mir.blocks[then_b as usize].term = Terminator::Return(vec![a]);
    mir.blocks[else_b as usize].term = Terminator::Return(vec![b]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(7, 3), 7);
    assert_eq!(f(2, 9), 9);
    assert_eq!(f(5, 5), 5);
}

#[test]
fn jit_select_emits_branchless() {
    // function pick(c:bool, a:i64, b:i64) -> i64 { return c ? a : b; }
    let mut mir = MirFunc::new("sel", CallConvHint::Tail, HirType::I64);
    let c = mir.new_value(HirType::Bool);
    let a = mir.new_value(HirType::I64);
    let b = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((c, HirType::Bool));
    mir.params.push((a, HirType::I64));
    mir.params.push((b, HirType::I64));

    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((c, HirType::Bool));
    mir.blocks[blk as usize].params.push((a, HirType::I64));
    mir.blocks[blk as usize].params.push((b, HirType::I64));
    mir.blocks[blk as usize].insts.push(Inst::Select {
        dst: r,
        cond: c,
        on_true: a,
        on_false: b,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(1, 100, 200), 100);
    assert_eq!(f(0, 100, 200), 200);
}

// ---------- end-to-end via rts_mir::lower ----------

fn build_hir_func(
    name: &str,
    params: Vec<(&str, HirType)>,
    ret: HirType,
    body: Vec<HirStmt>,
) -> HirFunc {
    HirFunc {
        name: name.into(),
        params: params
            .into_iter()
            .map(|(n, t)| HirParam {
                name: n.into(),
                ty: t,
                variadic: false,
                has_default: false,
            })
            .collect(),
        ret,
        body,
        is_async: false,
        is_arrow: false,
    }
}

#[test]
fn jit_end_to_end_hir_to_cl_via_mir() {
    // function inc(a:i64) -> i64 { return a + 1; }
    let body = vec![HirStmt::Return(Some(HirExpr::new(
        HirExprKind::Bin {
            op: HirBinOp::Add,
            lhs: Box::new(HirExpr::new(
                HirExprKind::Ident("a".into()),
                HirType::I64,
            )),
            rhs: Box::new(HirExpr::new(
                HirExprKind::Lit(HirLit::Int(1)),
                HirType::I64,
            )),
        },
        HirType::I64,
    )))];
    let hir = build_hir_func("inc", vec![("a", HirType::I64)], HirType::I64, body);

    let mut mir = rts_mir::lower::lower_func(&hir);
    rts_mir::passes::optimize(&mut mir);

    // Validate the IR structurally too.
    rts_mir::passes::verify(&mir).expect("MIR should verify");

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 1);
    assert_eq!(f(41), 42);
    assert_eq!(f(-2), -1);
}
