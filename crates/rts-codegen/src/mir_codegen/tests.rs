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

#[test]
fn jit_switch_routes_to_correct_case() {
    // function pick(n:i64) -> i64 {
    //     switch (n) {
    //         case 1: return 100;
    //         case 2: return 200;
    //         default: return 0;
    //     }
    // }
    let mut mir = MirFunc::new("pick", CallConvHint::Tail, HirType::I64);
    let n = mir.new_value(HirType::I64);
    mir.params.push((n, HirType::I64));

    let entry = mir.new_block();
    let case1 = mir.new_block();
    let case2 = mir.new_block();
    let default_b = mir.new_block();
    mir.blocks[entry as usize].params.push((n, HirType::I64));

    let r1 = mir.new_value(HirType::I64);
    let r2 = mir.new_value(HirType::I64);
    let r0 = mir.new_value(HirType::I64);

    mir.blocks[entry as usize].term = Terminator::Switch {
        index: n,
        default: default_b,
        cases: vec![(1, case1), (2, case2)],
    };

    mir.blocks[case1 as usize].insts.push(Inst::IConst {
        dst: r1,
        ty: HirType::I64,
        val: 100,
    });
    mir.blocks[case1 as usize].term = Terminator::Return(vec![r1]);

    mir.blocks[case2 as usize].insts.push(Inst::IConst {
        dst: r2,
        ty: HirType::I64,
        val: 200,
    });
    mir.blocks[case2 as usize].term = Terminator::Return(vec![r2]);

    mir.blocks[default_b as usize].insts.push(Inst::IConst {
        dst: r0,
        ty: HirType::I64,
        val: 0,
    });
    mir.blocks[default_b as usize].term = Terminator::Return(vec![r0]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(1), 100);
    assert_eq!(f(2), 200);
    assert_eq!(f(0), 0);
    assert_eq!(f(99), 0);
    assert_eq!(f(-1), 0);
}

#[test]
fn jit_loop_with_block_param_phi() {
    // function sum_to(n:i64) -> i64 {
    //     let i = 0; let s = 0;
    //     while (i < n) { s = s + i; i = i + 1; }
    //     return s;
    // }
    // Modeled as:
    //   block0(n): jump block1(0, 0)
    //   block1(i, s):
    //     cmp = i < n
    //     brif cmp, block2(i, s), block3(s)
    //   block2(i, s):
    //     s2 = s + i
    //     i2 = i + 1
    //     jump block1(i2, s2)
    //   block3(s):
    //     return s
    let mut mir = MirFunc::new("sum_to", CallConvHint::Tail, HirType::I64);
    let n = mir.new_value(HirType::I64);
    mir.params.push((n, HirType::I64));

    let entry = mir.new_block();
    let header = mir.new_block();
    let body = mir.new_block();
    let exit = mir.new_block();

    let i_h = mir.new_value(HirType::I64);
    let s_h = mir.new_value(HirType::I64);
    let i_b = mir.new_value(HirType::I64);
    let s_b = mir.new_value(HirType::I64);
    let s_e = mir.new_value(HirType::I64);

    mir.blocks[entry as usize].params.push((n, HirType::I64));
    mir.blocks[header as usize].params.push((i_h, HirType::I64));
    mir.blocks[header as usize].params.push((s_h, HirType::I64));
    mir.blocks[body as usize].params.push((i_b, HirType::I64));
    mir.blocks[body as usize].params.push((s_b, HirType::I64));
    mir.blocks[exit as usize].params.push((s_e, HirType::I64));

    // entry: jump block1(0, 0)
    let zero_i = mir.new_value(HirType::I64);
    let zero_s = mir.new_value(HirType::I64);
    mir.blocks[entry as usize].insts.push(Inst::IConst {
        dst: zero_i,
        ty: HirType::I64,
        val: 0,
    });
    mir.blocks[entry as usize].insts.push(Inst::IConst {
        dst: zero_s,
        ty: HirType::I64,
        val: 0,
    });
    mir.blocks[entry as usize].term = Terminator::Jump {
        target: header,
        args: vec![zero_i, zero_s],
    };

    // header: cmp = i_h < n; brif cmp, body(i_h, s_h), exit(s_h)
    let cmp = mir.new_value(HirType::Bool);
    mir.blocks[header as usize].insts.push(Inst::ICmp {
        dst: cmp,
        cond: IntCond::Slt,
        lhs: i_h,
        rhs: n,
    });
    mir.blocks[header as usize].term = Terminator::Brif {
        cond: cmp,
        then_block: body,
        then_args: vec![i_h, s_h],
        else_block: exit,
        else_args: vec![s_h],
    };

    // body: s2 = s_b + i_b; i2 = i_b + 1; jump header(i2, s2)
    let s2 = mir.new_value(HirType::I64);
    let i2 = mir.new_value(HirType::I64);
    mir.blocks[body as usize].insts.push(Inst::IAdd {
        dst: s2,
        lhs: s_b,
        rhs: i_b,
    });
    mir.blocks[body as usize].insts.push(Inst::IAddImm {
        dst: i2,
        lhs: i_b,
        imm: 1,
    });
    mir.blocks[body as usize].term = Terminator::Jump {
        target: header,
        args: vec![i2, s2],
    };

    // exit: return s_e
    mir.blocks[exit as usize].term = Terminator::Return(vec![s_e]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 0);    // empty loop
    assert_eq!(f(1), 0);    // 0
    assert_eq!(f(5), 10);   // 0+1+2+3+4
    assert_eq!(f(10), 45);  // 0+1+...+9
    assert_eq!(f(100), 4950);
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

// ---------- integration: parse real TS source through the full pipeline ----------

/// Helper: parse `source`, lower each user fn through HIR → MIR + passes,
/// run verify on each, return the MirFunc list.
fn pipeline_parse_ts(source: &str) -> Vec<rts_mir::ir::MirFunc> {
    let program = rts_parser::parse_source(source).expect("parse");
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut mirs = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mut mir_fn = rts_mir::lower::lower_func(&hir_fn);
            rts_mir::passes::optimize(&mut mir_fn);
            rts_mir::passes::narrow(&mut mir_fn);
            rts_mir::passes::verify(&mir_fn).expect("MIR verify");
            mirs.push(mir_fn);
        }
    }
    mirs
}

#[test]
fn pipeline_parses_simple_function() {
    let mirs = pipeline_parse_ts("function add(a: number, b: number): number { return a + b; }");
    assert_eq!(mirs.len(), 1);
    assert_eq!(mirs[0].name, "add");
    // entry block should have 2 params
    assert_eq!(mirs[0].blocks[0].params.len(), 2);
}

#[test]
fn pipeline_parses_if_else_function() {
    let src = r#"
        function pick(c: number, a: number, b: number): number {
            if (c) {
                return a;
            } else {
                return b;
            }
        }
    "#;
    let mirs = pipeline_parse_ts(src);
    assert_eq!(mirs.len(), 1);
    // entry + then + else + join (or fewer if optimize collapses) — must have ≥ 3 blocks
    assert!(mirs[0].blocks.len() >= 3);
    // entry must Brif
    assert!(matches!(
        mirs[0].blocks[0].term,
        rts_mir::ir::Terminator::Brif { .. }
    ));
}

#[test]
fn pipeline_parses_while_loop() {
    let src = r#"
        function loopy(n: number): number {
            let i = 0;
            while (i < n) {
                i = i + 1;
            }
            return i;
        }
    "#;
    let mirs = pipeline_parse_ts(src);
    assert_eq!(mirs.len(), 1);
    // Should have entry + header + body + exit (or more)
    assert!(mirs[0].blocks.len() >= 4);
}

#[test]
fn pipeline_parses_multiple_functions() {
    let src = r#"
        function inc(x: number): number { return x + 1; }
        function dec(x: number): number { return x - 1; }
        function dbl(x: number): number { return x * 2; }
    "#;
    let mirs = pipeline_parse_ts(src);
    assert_eq!(mirs.len(), 3);
    assert_eq!(mirs[0].name, "inc");
    assert_eq!(mirs[1].name, "dec");
    assert_eq!(mirs[2].name, "dbl");
}

#[test]
fn pipeline_with_for_loop() {
    let src = r#"
        function sumTo(n: number): number {
            let s = 0;
            for (let i = 0; i < n; i = i + 1) {
                s = s + i;
            }
            return s;
        }
    "#;
    let mirs = pipeline_parse_ts(src);
    assert_eq!(mirs.len(), 1);
    // for: entry + header + body + update + exit ≥ 5 blocks
    assert!(mirs[0].blocks.len() >= 5);
}

#[test]
fn pipeline_with_ternary_emits_select() {
    let src = "function pick(c: number, a: number, b: number): number { return c ? a : b; }";
    let mirs = pipeline_parse_ts(src);
    assert_eq!(mirs.len(), 1);
    let bb = &mirs[0].blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(i, rts_mir::ir::Inst::Select { .. })));
}

#[test]
fn pipeline_with_explicit_int_types() {
    // Narrow type annotations should propagate to MIR.
    let src = "function tally(a: i32, b: i32): i32 { return a + b; }";
    let mirs = pipeline_parse_ts(src);
    assert_eq!(mirs.len(), 1);
    // Ret type in MIR should be I32
    assert_eq!(mirs[0].ret, rts_hir::ir::HirType::I32);
}

// ---------- user-fn calls: TS source with two fns where one calls the other ----------

/// Like compile_ts_via_mir but supports cross-fn calls via two-pass driver.
/// Returns the JIT module + a map of fn_name → FuncId.
fn compile_ts_multi_via_mir(src: &str) -> (JITModule, std::collections::HashMap<String, cranelift_module::FuncId>) {
    let program = rts_parser::parse_source(src).expect("parse");
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut module = make_jit();

    // Pass 1: HIR + MIR + lower MIR pre-pass to gather every MirFunc and
    // declare them all in the module so cross-fn calls resolve.
    let mut mirs: Vec<rts_mir::ir::MirFunc> = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mut mir_fn = rts_mir::lower::lower_func_with_scope(&hir_fn, &hir_scope);
            mir_fn.conv = if cfg!(windows) {
                CallConvHint::WindowsFastcall
            } else {
                CallConvHint::SystemV
            };
            rts_mir::passes::optimize(&mut mir_fn);
            rts_mir::passes::verify(&mir_fn).expect("verify");
            mirs.push(mir_fn);
        }
    }

    let mut decls: std::collections::HashMap<String, cranelift_module::FuncId> = std::collections::HashMap::new();
    for mir in &mirs {
        let id = super::lower::declare_mir_func(&mut module, mir).expect("declare");
        decls.insert(mir.name.clone(), id);
    }

    // Pass 2: lower each body with the full decls map so CallUser resolves.
    for mir in &mirs {
        super::lower::lower_mir_func_with_decls(&mut module, mir, &decls).expect("lower");
    }

    module.finalize_definitions().expect("finalize");
    (module, decls)
}

#[test]
fn smoke_ts_to_native_via_mir_calls_other_user_fn() {
    let src = r#"
        function inner(x: i64): i64 { return x + 10; }
        function outer(a: i64, b: i64): i64 { return inner(a) + b; }
    "#;
    let (module, decls) = compile_ts_multi_via_mir(src);
    let outer_id = decls["outer"];
    let ptr = module.get_finalized_function(outer_id);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    // outer(7, 5) = inner(7) + 5 = 17 + 5 = 22
    assert_eq!(f(7, 5), 22);
    assert_eq!(f(0, 0), 10);
    assert_eq!(f(-3, 100), 107);
}

#[test]
fn smoke_ts_to_native_via_mir_recursive_fn() {
    // Tail-recursive sum via accumulator. With CallConv = host fastcall
    // (not Tail), this becomes a normal call chain — Cranelift won't
    // tail-call-optimise it. For modest n it still works fine.
    let src = r#"
        function sum_acc(n: i64, acc: i64): i64 {
            if (n <= 0) {
                return acc;
            } else {
                return sum_acc(n - 1, acc + n);
            }
        }
    "#;
    let (module, decls) = compile_ts_multi_via_mir(src);
    let id = decls["sum_acc"];
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0, 0), 0);
    assert_eq!(f(1, 0), 1);     // 1
    assert_eq!(f(5, 0), 15);    // 1+2+3+4+5
    assert_eq!(f(10, 0), 55);   // sum 1..10
    // Larger n risks stack overflow without TCO; keep modest.
    assert_eq!(f(100, 0), 5050);
}

#[test]
fn smoke_ts_to_native_via_mir_chained_calls() {
    let src = r#"
        function add1(x: i64): i64 { return x + 1; }
        function add2(x: i64): i64 { return add1(add1(x)); }
        function add3(x: i64): i64 { return add1(add2(x)); }
    "#;
    let (module, decls) = compile_ts_multi_via_mir(src);
    let id = decls["add3"];
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 3);
    assert_eq!(f(10), 13);
    assert_eq!(f(-5), -2);
}

// ---------- the full enchilada: TS source → MIR → native code → execute ----------

/// Compile a single user fn from TS source through the entire MIR pipeline
/// and return a JIT module + the FuncId of the first user fn found.
fn compile_ts_via_mir(src: &str) -> (JITModule, cranelift_module::FuncId) {
    let program = rts_parser::parse_source(src).expect("parse");
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut module = make_jit();

    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mut mir_fn = rts_mir::lower::lower_func(&hir_fn);
            rts_mir::passes::optimize(&mut mir_fn);
            rts_mir::passes::verify(&mir_fn).expect("verify");

            // Override conv to host default for JIT.
            mir_fn.conv = if cfg!(windows) {
                CallConvHint::WindowsFastcall
            } else {
                CallConvHint::SystemV
            };

            let id = super::lower::lower_mir_func(&mut module, &mir_fn).expect("lower mir");
            module.finalize_definitions().expect("finalize");
            return (module, id);
        }
    }
    panic!("no user fn in source");
}

#[test]
fn smoke_ts_to_native_via_mir_inc() {
    let (module, id) = compile_ts_via_mir(
        "function inc(a: i64): i64 { return a + 1; }",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 1);
    assert_eq!(f(41), 42);
    assert_eq!(f(-2), -1);
}

#[test]
fn smoke_ts_to_native_via_mir_max() {
    let (module, id) = compile_ts_via_mir(
        "function max(a: i64, b: i64): i64 { if (a > b) { return a; } else { return b; } }",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(7, 3), 7);
    assert_eq!(f(2, 9), 9);
    assert_eq!(f(5, 5), 5);
    assert_eq!(f(-1, -10), -1);
}

#[test]
fn smoke_ts_to_native_via_mir_arithmetic() {
    let (module, id) = compile_ts_via_mir(
        "function poly(x: i64): i64 { return x * x + x * 2 + 1; }",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    // (x+1)^2 = x^2 + 2x + 1
    assert_eq!(f(0), 1);
    assert_eq!(f(1), 4);
    assert_eq!(f(3), 16);
    assert_eq!(f(10), 121);
}

#[test]
fn smoke_ts_to_native_via_mir_bitwise() {
    let (module, id) = compile_ts_via_mir(
        "function mask(x: i64): i64 { return x & 0xff; }",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0xdeadbeef), 0xef);
    assert_eq!(f(0x100), 0);
    assert_eq!(f(0x42), 0x42);
}

#[test]
fn smoke_ts_to_native_via_mir_ternary() {
    let (module, id) = compile_ts_via_mir(
        "function abs(x: i64): i64 { return x < 0 ? -x : x; }",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 0);
    assert_eq!(f(5), 5);
    assert_eq!(f(-7), 7);
    assert_eq!(f(-100), 100);
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
