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
    make_jit_with_externs(&[])
}

/// Build a JIT module and register `(symbol, fn_ptr)` pairs so calls to
/// extern symbols can be resolved at link time inside the JIT.
fn make_jit_with_externs(externs: &[(&str, *const u8)]) -> JITModule {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for &(sym, ptr) in externs {
        builder.symbol(sym, ptr);
    }
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

    // Pre-pass: register every fn signature in the HIR scope so mutual
    // recursion (`isEven` calling `isOdd` declared later) sees the right
    // ret type during body lowering.
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let ret = fdecl
                .return_type
                .as_deref()
                .map(rts_hir::lower::parse_type_annotation)
                .unwrap_or(rts_hir::ir::HirType::Unknown);
            if !matches!(ret, rts_hir::ir::HirType::Unknown) {
                hir_scope.register_return_type(&fdecl.name, ret);
            }
            let param_tys: Vec<rts_hir::ir::HirType> = fdecl
                .parameters
                .iter()
                .map(|p| {
                    p.type_annotation
                        .as_deref()
                        .map(rts_hir::lower::parse_type_annotation)
                        .unwrap_or(rts_hir::ir::HirType::Unknown)
                })
                .collect();
            hir_scope.register_param_types(&fdecl.name, param_tys);
        }
    }

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

// ---------- extern calls: TS source calling rts namespace methods ----------

#[test]
fn smoke_ts_to_native_via_mir_calls_math_sqrt() {
    // Build MIR by hand calling __RTS_FN_NS_MATH_SQRT directly. This skips
    // the HIR layer (which would need a parser run) and validates that the
    // mir_codegen lower path resolves extern symbols through the JIT
    // builder's symbol table.
    let mut mir = MirFunc::new("call_sqrt", CallConvHint::Tail, HirType::F64);
    let x = mir.new_value(HirType::F64);
    let r = mir.new_value(HirType::F64);
    mir.params.push((x, HirType::F64));
    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((x, HirType::F64));
    mir.blocks[blk as usize].insts.push(rts_mir::ir::Inst::CallExtern {
        dst: Some(r),
        sym: "__RTS_FN_NS_MATH_SQRT".to_string(),
        args: vec![x],
        ret_ty: HirType::F64,
        param_tys: vec![HirType::F64],
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    // JIT with the runtime symbol registered.
    let mut module = make_jit_with_externs(&[(
        "__RTS_FN_NS_MATH_SQRT",
        rts_runtime::namespaces::math::basic::__RTS_FN_NS_MATH_SQRT as *const u8,
    )]);
    let mir = host_conv(mir);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(ptr) };
    assert!((f(4.0) - 2.0).abs() < 1e-12);
    assert!((f(9.0) - 3.0).abs() < 1e-12);
    assert!((f(2.0) - 2f64.sqrt()).abs() < 1e-12);
}

#[test]
fn smoke_extern_resolver_handles_math() {
    // Verify the extern_resolver_default closure exposes math.sqrt.
    let resolver = super::extern_resolver_default();
    let resolved = resolver("math", "sqrt").expect("math.sqrt should resolve");
    let (sym, params, ret) = resolved;
    assert_eq!(sym, "__RTS_FN_NS_MATH_SQRT");
    assert_eq!(params, vec![HirType::F64]);
    assert_eq!(ret, HirType::F64);
}

#[test]
fn smoke_extern_resolver_skips_unknown_namespace() {
    let resolver = super::extern_resolver_default();
    assert!(resolver("not_a_namespace", "sqrt").is_none());
    assert!(resolver("math", "not_a_method").is_none());
}

#[test]
fn smoke_pipeline_resolves_math_sqrt_through_hir() {
    // function root(x: f64): f64 { return math.sqrt(x); }
    let src = "function root(x: f64): f64 { return math.sqrt(x); }";
    let program = rts_parser::parse_source(src).expect("parse");
    let resolver = super::extern_resolver_default();

    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut found_call_extern = false;
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full(&hir_fn, &hir_scope, Some(&resolver));
            for block in &mir_fn.blocks {
                for inst in &block.insts {
                    if let rts_mir::ir::Inst::CallExtern { sym, .. } = inst {
                        if sym == "__RTS_FN_NS_MATH_SQRT" {
                            found_call_extern = true;
                        }
                    }
                }
            }
        }
    }
    assert!(found_call_extern, "expected MIR to contain CallExtern math.sqrt");
}

// ---------- string literals via StrLit + CallExtern with StrPtr ----------

#[test]
fn smoke_strlit_calls_io_stdout_write() {
    // Build MIR by hand: function shout() -> i64 {
    //     let written = io.stdout_write("ABC");
    //     return written;
    // }
    // (returns 3 = number of bytes written)
    let mut mir = MirFunc::new("shout", CallConvHint::Tail, HirType::I64);
    let blk = mir.new_block();
    let dst_ptr = mir.new_value(HirType::I64);
    let dst_len = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);

    mir.blocks[blk as usize].insts.push(rts_mir::ir::Inst::StrLit {
        dst_ptr,
        dst_len,
        value: "ABC".to_string(),
    });
    mir.blocks[blk as usize].insts.push(rts_mir::ir::Inst::CallExtern {
        dst: Some(r),
        sym: "__RTS_FN_NS_IO_STDOUT_WRITE".to_string(),
        args: vec![dst_ptr, dst_len],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64, HirType::I64],
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mut module = make_jit_with_externs(&[(
        "__RTS_FN_NS_IO_STDOUT_WRITE",
        rts_runtime::namespaces::io::stdout::__RTS_FN_NS_IO_STDOUT_WRITE as *const u8,
    )]);
    let mir = host_conv(mir);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    let written = f();
    assert_eq!(written, 3, "io.stdout_write returns bytes written");
}

#[test]
fn smoke_strlit_pipeline_resolves_io_print() {
    // function greet(): void { io.print("ola"); }
    let src = r#"function greet(): void { io.print("ola"); }"#;
    let program = rts_parser::parse_source(src).expect("parse");
    let resolver = super::extern_resolver_default();

    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut found_strlit = false;
    let mut found_call_print = false;
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full(&hir_fn, &hir_scope, Some(&resolver));
            for block in &mir_fn.blocks {
                for inst in &block.insts {
                    match inst {
                        rts_mir::ir::Inst::StrLit { value, .. } if value == "ola" => {
                            found_strlit = true;
                        }
                        rts_mir::ir::Inst::CallExtern { sym, args, .. }
                            if sym == "__RTS_FN_NS_IO_PRINT" =>
                        {
                            // Should have been expanded into 2 args (ptr + len).
                            assert_eq!(args.len(), 2, "StrPtr arg should expand to (ptr, len)");
                            found_call_print = true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(found_strlit, "expected MIR to contain StrLit \"ola\"");
    assert!(found_call_print, "expected CallExtern __RTS_FN_NS_IO_PRINT");
}

#[test]
fn smoke_strlit_resolver_now_accepts_io_print() {
    // After the StrPtr expansion lands the resolver should expose io.print.
    let resolver = super::extern_resolver_default();
    let resolved = resolver("io", "print").expect("io.print should resolve now");
    let (sym, params, ret) = resolved;
    assert_eq!(sym, "__RTS_FN_NS_IO_PRINT");
    assert_eq!(params, vec![HirType::Str]);
    assert_eq!(ret, HirType::Void);
}

// ---------- namespace constants via Member access ----------

#[test]
fn smoke_member_math_pi_returns_constant() {
    // function pi(): f64 { return math.PI; }
    let src = "function pi(): f64 { return math.PI; }";
    let program = rts_parser::parse_source(src).expect("parse");
    let resolver = super::extern_resolver_default();

    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut module = make_jit_with_externs(&[(
        "__RTS_FN_NS_MATH_PI",
        rts_runtime::namespaces::math::consts::__RTS_FN_NS_MATH_PI as *const u8,
    )]);
    let mut id_opt = None;
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mut mir_fn = rts_mir::lower::lower_func_full(&hir_fn, &hir_scope, Some(&resolver));
            mir_fn.conv = if cfg!(windows) {
                CallConvHint::WindowsFastcall
            } else {
                CallConvHint::SystemV
            };
            rts_mir::passes::optimize(&mut mir_fn);
            rts_mir::passes::verify(&mir_fn).expect("verify");
            id_opt = Some(super::lower::lower_mir_func(&mut module, &mir_fn).expect("lower"));
        }
    }
    module.finalize_definitions().expect("finalize");
    let id = id_opt.expect("fn");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(ptr) };
    assert!((f() - std::f64::consts::PI).abs() < 1e-12);
}

#[test]
fn smoke_member_pipeline_lowers_math_e_to_callextern() {
    let src = "function eulers(): f64 { return math.E; }";
    let program = rts_parser::parse_source(src).expect("parse");
    let resolver = super::extern_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut found_e = false;
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full(&hir_fn, &hir_scope, Some(&resolver));
            for block in &mir_fn.blocks {
                for inst in &block.insts {
                    if let rts_mir::ir::Inst::CallExtern { sym, args, .. } = inst {
                        if sym == "__RTS_FN_NS_MATH_E" {
                            assert!(args.is_empty(), "math.E should have zero args");
                            found_e = true;
                        }
                    }
                }
            }
        }
    }
    assert!(found_e, "expected CallExtern math.E in MIR");
}

#[test]
fn smoke_member_unknown_namespace_falls_through() {
    // `bogus.thing` should emit a placeholder zero, not a CallExtern.
    let src = "function f(): f64 { return bogus.thing; }";
    let program = rts_parser::parse_source(src).expect("parse");
    let resolver = super::extern_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full(&hir_fn, &hir_scope, Some(&resolver));
            for block in &mir_fn.blocks {
                for inst in &block.insts {
                    if let rts_mir::ir::Inst::CallExtern { sym, .. } = inst {
                        panic!("unknown namespace shouldn't produce CallExtern, saw {sym}");
                    }
                }
            }
        }
    }
}

// ---------- intrinsic inlining ----------

fn lower_with_intrinsics(src: &str) -> Vec<rts_mir::ir::MirFunc> {
    let program = rts_parser::parse_source(src).expect("parse");
    let extern_r = super::extern_resolver_default();
    let intr_r = super::intrinsic_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut out = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full_with_intrinsics(
                &hir_fn,
                &hir_scope,
                Some(&extern_r),
                Some(&intr_r),
            );
            out.push(mir_fn);
        }
    }
    out
}

#[test]
fn intrinsic_sqrt_emits_inst_sqrt_not_callextern() {
    let mirs = lower_with_intrinsics("function f(x: f64): f64 { return math.sqrt(x); }");
    let mir = &mirs[0];
    let mut sqrts = 0;
    let mut callextern_sqrt = 0;
    for block in &mir.blocks {
        for inst in &block.insts {
            match inst {
                rts_mir::ir::Inst::Sqrt { .. } => sqrts += 1,
                rts_mir::ir::Inst::CallExtern { sym, .. }
                    if sym.contains("MATH_SQRT") =>
                {
                    callextern_sqrt += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!(sqrts, 1, "expected 1 Inst::Sqrt");
    assert_eq!(callextern_sqrt, 0, "no CallExtern for sqrt when intrinsic active");
}

#[test]
fn intrinsic_min_emits_fmin() {
    let mirs = lower_with_intrinsics(
        "function f(a: f64, b: f64): f64 { return math.min(a, b); }",
    );
    let mir = &mirs[0];
    assert!(mir.blocks.iter().flat_map(|b| &b.insts).any(|i| matches!(
        i,
        rts_mir::ir::Inst::FMin { .. }
    )));
}

#[test]
fn intrinsic_abs_i64_emits_select_with_neg() {
    let mirs = lower_with_intrinsics(
        "function f(a: i64): i64 { return math.abs_i64(a); }",
    );
    let mir = &mirs[0];
    let has_neg = mir.blocks.iter().flat_map(|b| &b.insts).any(|i| matches!(
        i,
        rts_mir::ir::Inst::INeg { .. }
    ));
    let has_select = mir.blocks.iter().flat_map(|b| &b.insts).any(|i| matches!(
        i,
        rts_mir::ir::Inst::Select { .. }
    ));
    assert!(has_neg, "AbsI64 should emit INeg");
    assert!(has_select, "AbsI64 should emit Select");
}

#[test]
fn smoke_intrinsic_sqrt_jit_executes_without_extern() {
    // No __RTS_FN_NS_MATH_SQRT registered as extern — proves the JIT
    // doesn't need it because the intrinsic emitted Cranelift sqrt directly.
    let mirs = lower_with_intrinsics("function f(x: f64): f64 { return math.sqrt(x); }");
    let mut module = make_jit(); // no externs registered
    let mut mir = mirs.into_iter().next().unwrap();
    mir.conv = if cfg!(windows) {
        CallConvHint::WindowsFastcall
    } else {
        CallConvHint::SystemV
    };
    rts_mir::passes::optimize(&mut mir);
    rts_mir::passes::verify(&mir).expect("verify");
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(ptr) };
    assert!((f(16.0) - 4.0).abs() < 1e-12);
    assert!((f(2.0) - 2f64.sqrt()).abs() < 1e-12);
}

// ---------- coverage measurement: how much MIR can handle of real programs ----------

/// Result of trying to lower a single user function through MIR.
#[derive(Debug, Default)]
struct CoverageReport {
    fn_name: String,
    blocks: usize,
    inst_count: usize,
    trap_count: usize,        // Terminator::Trap (unsupported stmt)
    placeholder_zeros: usize, // Heuristic: IConst zero following an unsupported expr
    has_unsupported_stmt: bool,
    has_call_user: bool,
    has_call_extern: bool,
    has_intrinsic: bool,
}

fn analyze_mir(mir: &rts_mir::ir::MirFunc) -> CoverageReport {
    use rts_mir::ir::{Inst, Terminator, TrapHint};
    let mut r = CoverageReport {
        fn_name: mir.name.clone(),
        blocks: mir.blocks.len(),
        ..Default::default()
    };
    for block in &mir.blocks {
        r.inst_count += block.insts.len();
        if matches!(block.term, Terminator::Trap { code: TrapHint::User(_) }) {
            r.trap_count += 1;
            r.has_unsupported_stmt = true;
        }
        for inst in &block.insts {
            match inst {
                Inst::CallUser { .. } => r.has_call_user = true,
                Inst::CallExtern { .. } => r.has_call_extern = true,
                Inst::Sqrt { .. }
                | Inst::FAbs { .. }
                | Inst::FMin { .. }
                | Inst::FMax { .. } => r.has_intrinsic = true,
                Inst::IConst { val: 0, .. } => r.placeholder_zeros += 1,
                _ => {}
            }
        }
    }
    r
}

/// Resolve a path relative to the workspace root (parent of `crates/`).
/// Tests run with cwd = `crates/rts-codegen` so we walk up two levels.
fn workspace_path(rel: &str) -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().parent().unwrap().join(rel)
}

fn lower_file_through_mir(path: &str) -> Vec<CoverageReport> {
    let resolved = workspace_path(path);
    let source = std::fs::read_to_string(&resolved)
        .unwrap_or_else(|e| panic!("read {}: {e}", resolved.display()));
    let program = match rts_parser::parse_source(&source) {
        Ok(p) => p,
        Err(_) => return vec![], // parse error: skip, not interesting for MIR coverage
    };
    let extern_r = super::extern_resolver_default();
    let intr_r = super::intrinsic_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut reports = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full_with_intrinsics(
                &hir_fn,
                &hir_scope,
                Some(&extern_r),
                Some(&intr_r),
            );
            // Skip the verify step on purpose — we want to inspect the IR
            // even when it's incomplete (with traps for unsupported shapes).
            reports.push(analyze_mir(&mir_fn));
        }
    }
    reports
}

#[test]
fn coverage_monte_carlo_pi_user_fns() {
    // Only the user-fn `toFloat` lives in this file (top-level statements
    // would also need lowering but we focus on functions for now).
    let reports = lower_file_through_mir("bench/monte_carlo_pi.ts");
    if reports.is_empty() {
        // File path resolves relative to the crate root; if not found,
        // skip silently rather than fail (CI invariants).
        return;
    }
    let to_float = reports.iter().find(|r| r.fn_name == "toFloat");
    if let Some(r) = to_float {
        // Should successfully compile through MIR with sqrt as intrinsic.
        assert!(!r.has_unsupported_stmt, "toFloat should not trap");
        assert!(r.has_intrinsic, "toFloat should use sqrt intrinsic");
    }
}

#[test]
fn coverage_aggregate_across_bench_files() {
    // Walk a curated list of bench TS files and tally MIR coverage.
    // Output goes to stdout via `cargo test -- --nocapture` so a developer
    // can see exactly which fns trap and which are clean.
    let bench_files = [
        "bench/bun_simple.ts",
        "bench/events_concurrent.ts",
        "bench/events_crossover.ts",
        "bench/events_emit.ts",
        "bench/monte_carlo_pi.ts",
        "bench/monte_carlo_pi_threaded.ts",
        "bench/pi_bigfloat.ts",
        "bench/rts_simple.ts",
    ];

    let mut total_fns = 0;
    let mut clean_fns = 0;
    let mut total_traps = 0;

    for path in bench_files {
        let reports = lower_file_through_mir(path);
        let clean = reports.iter().filter(|r| !r.has_unsupported_stmt).count();
        let traps: usize = reports.iter().map(|r| r.trap_count).sum();
        total_fns += reports.len();
        clean_fns += clean;
        total_traps += traps;
        println!(
            "[{}] {}/{} clean ({} traps total)",
            path,
            clean,
            reports.len(),
            traps
        );
        for r in &reports {
            let status = if r.has_unsupported_stmt { "TRAP" } else { "ok" };
            println!(
                "    [{}] {} (blocks={}, insts={}, call_user={}, call_extern={}, intrinsic={})",
                status, r.fn_name, r.blocks, r.inst_count,
                r.has_call_user, r.has_call_extern, r.has_intrinsic
            );
        }
    }

    println!(
        "===== AGGREGATE: {clean_fns}/{total_fns} clean, {total_traps} traps total ====="
    );

    assert!(total_fns > 0, "at least one bench file should have user fns");
    // For tracking purposes only — this asserts a baseline so regressions
    // pop a test failure. Bump up as coverage improves.
    let pct = (clean_fns * 100) / total_fns.max(1);
    assert!(
        pct >= 10,
        "expected ≥10% MIR coverage across bench fns, got {pct}% ({clean_fns}/{total_fns})"
    );
}

#[test]
fn coverage_simple_arithmetic_fn_is_clean() {
    // Sanity: a hand-written arithmetic fn should be 100% clean.
    let mut tmp = std::env::temp_dir();
    tmp.push("rts_mir_cov_smoke.ts");
    std::fs::write(
        &tmp,
        r#"
            function poly(x: f64): f64 {
                return x * x + 2.0 * x + 1.0;
            }
            function step(n: i64): i64 {
                if (n <= 0) {
                    return 0;
                } else {
                    return n + step(n - 1);
                }
            }
        "#,
    )
    .unwrap();
    // Pass an absolute path to avoid the workspace_path prefix.
    let absolute = tmp.to_str().unwrap();
    let source = std::fs::read_to_string(absolute).unwrap();
    let program = rts_parser::parse_source(&source).expect("parse");
    let extern_r = super::extern_resolver_default();
    let intr_r = super::intrinsic_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut reports = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full_with_intrinsics(
                &hir_fn,
                &hir_scope,
                Some(&extern_r),
                Some(&intr_r),
            );
            reports.push(analyze_mir(&mir_fn));
        }
    }
    assert_eq!(reports.len(), 2);
    assert!(!reports[0].has_unsupported_stmt, "poly should be clean");
    assert!(!reports[1].has_unsupported_stmt, "step should be clean");
    assert!(reports[1].has_call_user, "step should have CallUser (recursion)");
}

// ---------- hardening: shapes complexas via MIR ↔ native ↔ execute ----------

#[test]
fn jit_polynomial_with_intrinsic() {
    // function poly(x: f64): f64 { return math.sqrt(x*x + 1.0); }
    let src = "function poly(x: f64): f64 { return math.sqrt(x*x + 1.0); }";
    let program = rts_parser::parse_source(src).expect("parse");
    let extern_r = super::extern_resolver_default();
    let intr_r = super::intrinsic_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut module = make_jit();
    let mut id_opt = None;
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mut mir = rts_mir::lower::lower_func_full_with_intrinsics(
                &hir_fn,
                &hir_scope,
                Some(&extern_r),
                Some(&intr_r),
            );
            mir.conv = if cfg!(windows) {
                CallConvHint::WindowsFastcall
            } else {
                CallConvHint::SystemV
            };
            rts_mir::passes::optimize(&mut mir);
            rts_mir::passes::verify(&mir).expect("verify");
            id_opt = Some(super::lower::lower_mir_func(&mut module, &mir).expect("lower"));
        }
    }
    module.finalize_definitions().expect("finalize");
    let id = id_opt.expect("fn");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(ptr) };
    // poly(0) = sqrt(0+1) = 1
    assert!((f(0.0) - 1.0).abs() < 1e-12);
    // poly(3) = sqrt(9+1) = sqrt(10)
    assert!((f(3.0) - 10f64.sqrt()).abs() < 1e-12);
    // poly(-2) = sqrt(4+1) = sqrt(5)
    assert!((f(-2.0) - 5f64.sqrt()).abs() < 1e-12);
}

// NOTE: shapes that need block-param phis for loop-carry mutation
// (`i = i + 1` in a while body) aren't yet modeled by the MIR lowering.
// The current lower drops the assignment silently — bail goes through
// `had_placeholders` only when caller checks the flag. JIT-executing
// such MIR enters an infinite loop (`i` stays at 0). Tests below
// stick to recursion + immutable bindings until block-param SSA lands.

#[test]
fn jit_deep_recursion_factorial() {
    // factorial defined recursively — exercises CallUser self-recursion.
    let src = r#"
        function fact(n: i64): i64 {
            if (n <= 1) { return 1; }
            return n * fact(n - 1);
        }
    "#;
    let (module, decls) = compile_ts_multi_via_mir(src);
    let id = decls["fact"];
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 1);
    assert_eq!(f(1), 1);
    assert_eq!(f(5), 120);
    assert_eq!(f(10), 3628800);
    assert_eq!(f(12), 479001600);
}

#[test]
fn jit_mutual_recursion_even_odd() {
    // is_even / is_odd — exercises CallUser cross-fn calls.
    let src = r#"
        function isEven(n: i64): i64 {
            if (n == 0) { return 1; }
            return isOdd(n - 1);
        }
        function isOdd(n: i64): i64 {
            if (n == 0) { return 0; }
            return isEven(n - 1);
        }
    "#;
    let (module, decls) = compile_ts_multi_via_mir(src);
    let even = decls["isEven"];
    let odd = decls["isOdd"];
    let f_even: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(module.get_finalized_function(even)) };
    let f_odd: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(module.get_finalized_function(odd)) };
    assert_eq!(f_even(0), 1);
    assert_eq!(f_even(10), 1);
    assert_eq!(f_even(7), 0);
    assert_eq!(f_odd(0), 0);
    assert_eq!(f_odd(7), 1);
    assert_eq!(f_odd(10), 0);
}

// ---------- arrays via collections.vec_* ----------

fn lower_with_intrinsics_and_externs(src: &str) -> Vec<rts_mir::ir::MirFunc> {
    let program = rts_parser::parse_source(src).expect("parse");
    let extern_r = super::extern_resolver_default();
    let intr_r = super::intrinsic_resolver_default();
    let mut hir_scope = rts_hir::scope::Scope::new();
    let mut out = Vec::new();
    for item in &program.items {
        if let rts_ast::ast::Item::Function(fdecl) = item {
            let hir_fn = rts_hir::lower::lower_func(fdecl, &mut hir_scope);
            let mir_fn = rts_mir::lower::lower_func_full_with_intrinsics(
                &hir_fn,
                &hir_scope,
                Some(&extern_r),
                Some(&intr_r),
            );
            out.push(mir_fn);
        }
    }
    out
}

#[test]
fn jit_array_literal_and_index() {
    // function pick(): i64 {
    //     const arr: i64[] = [10, 20, 30];
    //     return arr[1];
    // }
    let src = r#"
        function pick(): i64 {
            const arr: i64[] = [10, 20, 30];
            return arr[1];
        }
    "#;
    let mirs = lower_with_intrinsics_and_externs(src);
    assert_eq!(mirs.len(), 1);
    let mut mir = mirs.into_iter().next().unwrap();
    mir.conv = if cfg!(windows) {
        CallConvHint::WindowsFastcall
    } else {
        CallConvHint::SystemV
    };
    rts_mir::passes::optimize(&mut mir);
    rts_mir::passes::verify(&mir).expect("verify");
    assert!(!mir.had_placeholders, "array+index should not trigger placeholders");

    // Resolve runtime symbols via raw fn pointers.
    let mut module = make_jit_with_externs(&[
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8,
        ),
    ]);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 20);
}

#[test]
fn jit_array_length() {
    // function n(): i64 {
    //     const arr: i64[] = [1, 2, 3, 4, 5];
    //     return arr.length;
    // }
    let src = r#"
        function n(): i64 {
            const arr: i64[] = [1, 2, 3, 4, 5];
            return arr.length;
        }
    "#;
    let mirs = lower_with_intrinsics_and_externs(src);
    let mut mir = mirs.into_iter().next().unwrap();
    mir.conv = if cfg!(windows) {
        CallConvHint::WindowsFastcall
    } else {
        CallConvHint::SystemV
    };
    rts_mir::passes::optimize(&mut mir);
    rts_mir::passes::verify(&mir).expect("verify");
    assert!(!mir.had_placeholders);

    let mut module = make_jit_with_externs(&[
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN as *const u8,
        ),
    ]);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 5);
}

#[test]
fn jit_array_sum_via_loop() {
    // function sum(): i64 {
    //     const arr: i64[] = [10, 20, 30];
    //     let s: i64 = 0;
    //     let i: i64 = 0;
    //     while (i < 3) { s = s + arr[i]; i = i + 1; }
    //     return s;
    // }
    let src = r#"
        function sumArr(): i64 {
            const arr: i64[] = [10, 20, 30];
            let s: i64 = 0;
            let i: i64 = 0;
            while (i < 3) {
                s = s + arr[i];
                i = i + 1;
            }
            return s;
        }
    "#;
    let mirs = lower_with_intrinsics_and_externs(src);
    let mut mir = mirs.into_iter().next().unwrap();
    mir.conv = if cfg!(windows) {
        CallConvHint::WindowsFastcall
    } else {
        CallConvHint::SystemV
    };
    rts_mir::passes::optimize(&mut mir);
    rts_mir::passes::verify(&mir).expect("verify");
    assert!(!mir.had_placeholders);

    let mut module = make_jit_with_externs(&[
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8,
        ),
    ]);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 60);
}

// ---------- GC stack maps ----------

#[test]
fn gc_array_literal_emits_declare_gc_value() {
    // Pos-pass deveria inserir Inst::DeclareGcValue após o vec_new
    // (cujo ret é Handle). Verifica estruturalmente.
    let src = "function n(): i64 { const arr: i64[] = [1,2,3]; return arr.length; }";
    let mirs = lower_with_intrinsics_and_externs(src);
    let mir = &mirs[0];
    let has_gc_decl = mir.blocks.iter().flat_map(|b| &b.insts).any(|i| matches!(
        i,
        rts_mir::ir::Inst::DeclareGcValue { .. }
    ));
    assert!(has_gc_decl, "expected DeclareGcValue after vec_new");
}

#[test]
fn gc_array_literal_jit_compiles_with_stack_maps() {
    // Compila uma fn que usa vec_new (handle) + vec_get e verifica que o
    // JIT aceita o IR com declare_value_needs_stack_map dentro. Sem este
    // suporte, o lower antigo gerava erro do verifier.
    let src = "function pick(): i64 { const arr: i64[] = [10,20,30]; return arr[1]; }";
    let mirs = lower_with_intrinsics_and_externs(src);
    let mut mir = mirs.into_iter().next().unwrap();
    mir.conv = if cfg!(windows) {
        CallConvHint::WindowsFastcall
    } else {
        CallConvHint::SystemV
    };
    rts_mir::passes::optimize(&mut mir);
    rts_mir::passes::verify(&mir).expect("verify");

    // Importante: optimize NÃO pode remover o DeclareGcValue.
    let still_has_gc = mir.blocks.iter().flat_map(|b| &b.insts).any(|i| matches!(
        i,
        rts_mir::ir::Inst::DeclareGcValue { .. }
    ));
    assert!(still_has_gc, "DeclareGcValue should survive optimize");

    let mut module = make_jit_with_externs(&[
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8,
        ),
    ]);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 20);
}

// ---------- arr[i] = v assignment ----------

#[test]
fn jit_array_index_assignment() {
    // function setup(): i64 {
    //     const arr: i64[] = [10, 20, 30];
    //     arr[1] = 99;
    //     return arr[1];
    // }
    let src = r#"
        function setup(): i64 {
            const arr: i64[] = [10, 20, 30];
            arr[1] = 99;
            return arr[1];
        }
    "#;
    let mirs = lower_with_intrinsics_and_externs(src);
    let mut mir = mirs.into_iter().next().unwrap();
    mir.conv = if cfg!(windows) {
        CallConvHint::WindowsFastcall
    } else {
        CallConvHint::SystemV
    };
    rts_mir::passes::optimize(&mut mir);
    rts_mir::passes::verify(&mir).expect("verify");
    assert!(!mir.had_placeholders, "arr[i]=v should not trigger placeholders");

    let mut module = make_jit_with_externs(&[
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8,
        ),
        (
            "__RTS_FN_NS_COLLECTIONS_VEC_SET",
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_SET as *const u8,
        ),
    ]);
    let id = super::lower::lower_mir_func(&mut module, &mir).expect("lower");
    module.finalize_definitions().expect("finalize");
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 99, "arr[1] após assignment deve ser 99");
}

// ---------- inline integrado: caller+callee pelo MIR_CACHE ----------

/// Compila um source TS através do pipeline real (compile_program) e
/// devolve um JITModule + map de FuncIds. Usado nos testes que precisam
/// validar que o routing automatico (inline + cache) funciona ponta a
/// ponta sem instanciar manualmente cada MirFunc.
fn compile_source_via_compile_program(
    src: &str,
) -> (JITModule, std::collections::HashMap<String, cranelift_module::FuncId>) {
    use cranelift_codegen::settings::{self, Configurable};

    let mut program = rts_parser::parse_source(src).expect("parse");

    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    // Registra symbols runtime mais comuns (math.PI etc.). Para os
    // testes simples que usam só aritmetica, normalmente nao precisamos.
    let _ = builder.symbol(
        "__RTS_FN_NS_MATH_PI",
        rts_runtime::namespaces::math::consts::__RTS_FN_NS_MATH_PI as *const u8,
    );
    let mut module = JITModule::new(builder);

    let mut extern_cache: std::collections::HashMap<String, cranelift_module::FuncId> =
        std::collections::HashMap::new();
    let mut data_counter: u32 = 0;
    crate::codegen::lower::compile_program(
        &mut program,
        &mut module,
        &mut extern_cache,
        &mut data_counter,
    )
    .expect("compile_program");
    module.finalize_definitions().expect("finalize");

    let mut decls: std::collections::HashMap<String, cranelift_module::FuncId> =
        std::collections::HashMap::new();
    for (name, id) in &extern_cache {
        if let Some(stripped) = name.strip_prefix("__user_") {
            decls.insert(stripped.to_string(), *id);
        }
    }
    (module, decls)
}

#[test]
fn inline_routing_caller_callee_e2e() {
    // function inc(x: i64): i64 { return x + 1; }
    // function dbl(x: i64): i64 { return x * 2; }
    // function compute(): i64 { return dbl(inc(20)); }
    //
    // Com routing default: inc cached, dbl cached, compute lowered
    // depois aplica inline em ambos calls. JIT executa o equivalente
    // a `(20 + 1) * 2 == 42` sem chamar inc/dbl em runtime.
    let src = r#"
        function inc(x: i64): i64 { return x + 1; }
        function dbl(x: i64): i64 { return x * 2; }
        function compute(): i64 { return dbl(inc(20)); }
    "#;
    let (module, decls) = compile_source_via_compile_program(src);
    let id = decls["compute"];
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 42, "compute() == dbl(inc(20)) == 42");
}

// ---------- inline pass + JIT ----------

#[test]
fn jit_inline_helper_executes_correctly() {
    // function inc(x: i64): i64 { return x + 1; }
    // function caller(): i64 { return inc(41); }
    // Após inline: caller fica `return 41 + 1`, sem CallUser.
    use std::collections::HashMap;

    // Build callee MIR
    let mut callee = rts_mir::ir::MirFunc::new(
        "inc",
        CallConvHint::Tail,
        HirType::I64,
    );
    let p = callee.new_value(HirType::I64);
    let r = callee.new_value(HirType::I64);
    callee.params.push((p, HirType::I64));
    let blk = callee.new_block();
    callee.blocks[blk as usize].params.push((p, HirType::I64));
    callee.blocks[blk as usize].insts.push(Inst::IAddImm {
        dst: r,
        lhs: p,
        imm: 1,
    });
    callee.blocks[blk as usize].term = Terminator::Return(vec![r]);

    // Build caller MIR
    let mut caller = rts_mir::ir::MirFunc::new(
        "caller",
        CallConvHint::Tail,
        HirType::I64,
    );
    let a = caller.new_value(HirType::I64);
    let b = caller.new_value(HirType::I64);
    let blk_c = caller.new_block();
    caller.blocks[blk_c as usize].insts.push(Inst::IConst {
        dst: a,
        ty: HirType::I64,
        val: 41,
    });
    caller.blocks[blk_c as usize].insts.push(Inst::CallUser {
        dst: Some(b),
        name: "inc".into(),
        args: vec![a],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64],
    });
    caller.blocks[blk_c as usize].term = Terminator::Return(vec![b]);

    // Aplicar inline pass
    let mut callees: HashMap<String, rts_mir::ir::MirFunc> = HashMap::new();
    callees.insert("inc".into(), callee);
    rts_mir::passes::inline::inline(&mut caller, &callees);

    // Verifica que CallUser sumiu
    let has_call = caller.blocks[0]
        .insts
        .iter()
        .any(|i| matches!(i, Inst::CallUser { .. }));
    assert!(!has_call, "inline deveria ter eliminado CallUser");

    // JIT compile e executa
    let caller = host_conv(caller);
    let (module, id) = lower_and_finalize(caller);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 42, "inline preserva semântica: 41 + 1 == 42");
}

// ---------- atomics ----------

#[test]
fn jit_atomic_load_store_roundtrip() {
    // function f(p: i64): i64 {
    //     atomic_store(p, 42);
    //     return atomic_load(p);
    // }
    // Construímos o MirFunc na mão pra testar Inst::AtomicLoad/AtomicStore
    // sem depender de gating por nome no HIR.
    use rts_mir::ir::{MemOrder, RmwOp};

    let mut mir = MirFunc::new("f", CallConvHint::Tail, HirType::I64);
    let p = mir.new_value(HirType::I64);
    let val42 = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((p, HirType::I64));

    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((p, HirType::I64));
    mir.blocks[blk as usize].insts.push(Inst::IConst {
        dst: val42,
        ty: HirType::I64,
        val: 42,
    });
    mir.blocks[blk as usize].insts.push(Inst::AtomicStore {
        val: val42,
        ptr: p,
        order: MemOrder::SeqCst,
    });
    mir.blocks[blk as usize].insts.push(Inst::AtomicLoad {
        dst: r,
        ptr: p,
        order: MemOrder::SeqCst,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(*mut i64) -> i64 = unsafe { std::mem::transmute(ptr) };

    let mut storage: i64 = 0;
    let result = f(&mut storage as *mut i64);
    assert_eq!(result, 42);
    assert_eq!(storage, 42);

    // Suprime warning de variável não usada no escopo de teste.
    let _ = (RmwOp::Add,);
}

#[test]
fn jit_atomic_rmw_add_increments() {
    // function inc(p: i64): i64 {
    //     return atomic_rmw_add(p, 5);  // returns OLD value
    // }
    use rts_mir::ir::{MemOrder, RmwOp};

    let mut mir = MirFunc::new("inc", CallConvHint::Tail, HirType::I64);
    let p = mir.new_value(HirType::I64);
    let v = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((p, HirType::I64));

    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((p, HirType::I64));
    mir.blocks[blk as usize].insts.push(Inst::IConst {
        dst: v,
        ty: HirType::I64,
        val: 5,
    });
    mir.blocks[blk as usize].insts.push(Inst::AtomicRmw {
        dst: r,
        op: RmwOp::Add,
        ptr: p,
        val: v,
        order: MemOrder::SeqCst,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(*mut i64) -> i64 = unsafe { std::mem::transmute(ptr) };

    let mut storage: i64 = 100;
    let old = f(&mut storage as *mut i64);
    assert_eq!(old, 100, "atomic_rmw_add returns OLD value");
    assert_eq!(storage, 105, "memory updated to 100 + 5");
}

#[test]
fn jit_atomic_cas_succeeds_when_match() {
    // function cas(p: i64, expected: i64, replacement: i64): i64 {
    //     return atomic_cas(p, expected, replacement);  // returns OLD
    // }
    use rts_mir::ir::MemOrder;

    let mut mir = MirFunc::new("cas", CallConvHint::Tail, HirType::I64);
    let p = mir.new_value(HirType::I64);
    let exp = mir.new_value(HirType::I64);
    let rep = mir.new_value(HirType::I64);
    let r = mir.new_value(HirType::I64);
    mir.params.push((p, HirType::I64));
    mir.params.push((exp, HirType::I64));
    mir.params.push((rep, HirType::I64));

    let blk = mir.new_block();
    mir.blocks[blk as usize].params.push((p, HirType::I64));
    mir.blocks[blk as usize].params.push((exp, HirType::I64));
    mir.blocks[blk as usize].params.push((rep, HirType::I64));
    mir.blocks[blk as usize].insts.push(Inst::AtomicCas {
        dst: r,
        ptr: p,
        expected: exp,
        replacement: rep,
        order: MemOrder::SeqCst,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(*mut i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(ptr) };

    let mut storage: i64 = 7;
    let old = f(&mut storage as *mut i64, 7, 42);
    assert_eq!(old, 7, "cas returns OLD value");
    assert_eq!(storage, 42, "cas swaps when expected matches");

    // Falha: expected != current
    let mut storage2: i64 = 99;
    let old2 = f(&mut storage2 as *mut i64, 50, 1);
    assert_eq!(old2, 99);
    assert_eq!(storage2, 99, "cas leaves memory untouched on mismatch");
}

#[test]
fn jit_fence_emits_no_op_terminator() {
    // function f(): i64 { fence(); return 7; }
    use rts_mir::ir::MemOrder;

    let mut mir = MirFunc::new("f", CallConvHint::Tail, HirType::I64);
    let r = mir.new_value(HirType::I64);
    let blk = mir.new_block();
    mir.blocks[blk as usize].insts.push(Inst::Fence {
        order: MemOrder::SeqCst,
    });
    mir.blocks[blk as usize].insts.push(Inst::IConst {
        dst: r,
        ty: HirType::I64,
        val: 7,
    });
    mir.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mir = host_conv(mir);
    let (module, id) = lower_and_finalize(mir);
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(), 7);
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

// ---------- loops com mutacao de variaveis locais (block params/phi) ----------

#[test]
fn jit_while_with_mutation_counts_to_n() {
    let (module, id) = compile_ts_via_mir(
        "function count(n: i64): i64 {\n    let i: i64 = 0;\n    while (i < n) { i = i + 1; }\n    return i;\n}",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 0);
    assert_eq!(f(5), 5);
    assert_eq!(f(100), 100);
}

#[test]
fn jit_while_sums_squares() {
    let (module, id) = compile_ts_via_mir(
        "function sumSquares(n: i64): i64 {\n    let s: i64 = 0;\n    let i: i64 = 1;\n    while (i <= n) { s = s + i * i; i = i + 1; }\n    return s;\n}",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(3), 14);
    assert_eq!(f(5), 55);
}

#[test]
fn jit_for_classic_with_update() {
    let (module, id) = compile_ts_via_mir(
        "function product(n: i64): i64 {\n    let p: i64 = 1;\n    for (let i: i64 = 1; i <= n; i = i + 1) { p = p * i; }\n    return p;\n}",
    );
    let ptr = module.get_finalized_function(id);
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr) };
    assert_eq!(f(0), 1);
    assert_eq!(f(5), 120);
}
