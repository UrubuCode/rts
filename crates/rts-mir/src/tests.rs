//! Smoke tests for MIR construction primitives.
//!
//! Lower de HIR e passes virão em sub-etapas seguintes; aqui só validamos
//! os ctors e os helpers de construção.

use crate::ir::*;
use crate::lower::lower_func;
use crate::passes::{dce, fold, narrow, narrow_return, optimize, verify, VerifyError};
use rts_hir::ir::{HirBinOp, HirExpr, HirExprKind, HirFunc, HirLit, HirParam, HirStmt};
use rts_hir::HirType;

// ---------- HIR construction helpers ----------

fn lit_int(n: i64) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Int(n)), HirType::I64)
}
fn lit_bool(b: bool) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Bool(b)), HirType::Bool)
}
fn ident(name: &str, ty: HirType) -> HirExpr {
    HirExpr::new(HirExprKind::Ident(name.into()), ty)
}
fn bin(op: HirBinOp, lhs: HirExpr, rhs: HirExpr, ty: HirType) -> HirExpr {
    HirExpr::new(
        HirExprKind::Bin {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        ty,
    )
}
fn fn_decl(
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
                optional: false,
                default_expr: None,
            })
            .collect(),
        ret,
        body,
        is_async: false,
        is_arrow: false,
    }
}

#[test]
fn mirfunc_new_records_metadata() {
    let f = MirFunc::new("foo", CallConvHint::Tail, HirType::I64);
    assert_eq!(f.name, "foo");
    assert_eq!(f.conv, CallConvHint::Tail);
    assert_eq!(f.ret, HirType::I64);
    assert!(f.blocks.is_empty());
    assert!(f.values.is_empty());
    assert!(f.params.is_empty());
}

#[test]
fn new_value_returns_increasing_ids_and_records_type() {
    let mut f = MirFunc::new("g", CallConvHint::SystemV, HirType::Void);
    let v0 = f.new_value(HirType::I32);
    let v1 = f.new_value(HirType::F64);
    let v2 = f.new_value(HirType::Bool);
    assert_eq!(v0, 0);
    assert_eq!(v1, 1);
    assert_eq!(v2, 2);
    assert_eq!(*f.type_of(v0), HirType::I32);
    assert_eq!(*f.type_of(v1), HirType::F64);
    assert_eq!(*f.type_of(v2), HirType::Bool);
}

#[test]
fn new_block_starts_with_trap_terminator() {
    let mut f = MirFunc::new("h", CallConvHint::Tail, HirType::Void);
    let b = f.new_block();
    assert_eq!(b, 0);
    let bb = &f.blocks[b as usize];
    assert!(bb.params.is_empty());
    assert!(bb.insts.is_empty());
    assert!(matches!(bb.term, Terminator::Trap { .. }));
}

#[test]
fn build_simple_add_block() {
    let mut f = MirFunc::new("add", CallConvHint::Tail, HirType::I64);
    let p0 = f.new_value(HirType::I64);
    let p1 = f.new_value(HirType::I64);
    f.params.push((p0, HirType::I64));
    f.params.push((p1, HirType::I64));

    let b = f.new_block();
    f.blocks[b as usize].params.push((p0, HirType::I64));
    f.blocks[b as usize].params.push((p1, HirType::I64));

    let dst = f.new_value(HirType::I64);
    f.blocks[b as usize].insts.push(Inst::IAdd { dst, lhs: p0, rhs: p1 });
    f.blocks[b as usize].term = Terminator::Return(vec![dst]);

    let bb = &f.blocks[0];
    assert_eq!(bb.insts.len(), 1);
    assert!(matches!(bb.insts[0], Inst::IAdd { .. }));
    if let Terminator::Return(vals) = &bb.term {
        assert_eq!(vals, &vec![dst]);
    } else {
        panic!("expected Return terminator");
    }
}

#[test]
fn brif_terminator_carries_block_args() {
    let mut f = MirFunc::new("br", CallConvHint::Tail, HirType::I64);
    let cond = f.new_value(HirType::Bool);
    let then_b = f.new_block();
    let else_b = f.new_block();
    let head = f.new_block();

    let v = f.new_value(HirType::I64);
    f.blocks[head as usize].term = Terminator::Brif {
        cond,
        then_block: then_b,
        then_args: vec![v],
        else_block: else_b,
        else_args: vec![],
    };

    if let Terminator::Brif { then_args, else_args, .. } = &f.blocks[head as usize].term {
        assert_eq!(then_args.len(), 1);
        assert!(else_args.is_empty());
    } else {
        panic!("expected Brif");
    }
}

#[test]
fn switch_terminator_records_cases() {
    let mut f = MirFunc::new("sw", CallConvHint::Tail, HirType::Void);
    let idx = f.new_value(HirType::I64);
    let def_b = f.new_block();
    let case_b = f.new_block();
    let head = f.new_block();

    f.blocks[head as usize].term = Terminator::Switch {
        index: idx,
        default: def_b,
        cases: vec![(0, case_b), (1, case_b), (42, def_b)],
    };
    if let Terminator::Switch { cases, .. } = &f.blocks[head as usize].term {
        assert_eq!(cases.len(), 3);
        assert_eq!(cases[0].0, 0);
        assert_eq!(cases[2].0, 42);
    } else {
        panic!("expected Switch");
    }
}

#[test]
fn intcond_and_floatcond_distinct() {
    assert_ne!(IntCond::Slt, IntCond::Ult);
    assert_ne!(FloatCond::OLt, FloatCond::ULt);
}

// ---------- Lowering tests ----------

#[test]
fn lower_empty_void_fn_returns_void() {
    let f = fn_decl("noop", vec![], HirType::Void, vec![]);
    let mir = lower_func(&f);
    assert_eq!(mir.name, "noop");
    assert_eq!(mir.blocks.len(), 1);
    assert!(matches!(mir.blocks[0].term, Terminator::Return(ref v) if v.is_empty()));
}

#[test]
fn lower_return_int_literal() {
    let body = vec![HirStmt::Return(Some(lit_int(42)))];
    let f = fn_decl("answer", vec![], HirType::I64, body);
    let mir = lower_func(&f);

    // Single block with one IConst + Return
    assert_eq!(mir.blocks.len(), 1);
    let bb = &mir.blocks[0];
    assert!(matches!(
        bb.insts[0],
        Inst::IConst { val: 42, ty: HirType::I64, .. }
    ));
    if let Terminator::Return(vs) = &bb.term {
        assert_eq!(vs.len(), 1);
    } else {
        panic!("expected Return");
    }
}

#[test]
fn lower_add_params_emits_iadd() {
    // function add(a:i64, b:i64): i64 { return a + b; }
    let body = vec![HirStmt::Return(Some(bin(
        HirBinOp::Add,
        ident("a", HirType::I64),
        ident("b", HirType::I64),
        HirType::I64,
    )))];
    let f = fn_decl(
        "add",
        vec![("a", HirType::I64), ("b", HirType::I64)],
        HirType::I64,
        body,
    );
    let mir = lower_func(&f);

    assert_eq!(mir.params.len(), 2);
    let bb = &mir.blocks[0];
    // entry block carries 2 params
    assert_eq!(bb.params.len(), 2);
    // exactly one IAdd in the body
    let iadd_count = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::IAdd { .. }))
        .count();
    assert_eq!(iadd_count, 1);
    assert!(matches!(bb.term, Terminator::Return(_)));
}

#[test]
fn lower_float_add_emits_fadd() {
    let lhs = HirExpr::new(HirExprKind::Lit(HirLit::Float(1.5)), HirType::F64);
    let rhs = HirExpr::new(HirExprKind::Lit(HirLit::Float(2.5)), HirType::F64);
    let body = vec![HirStmt::Return(Some(bin(
        HirBinOp::Add, lhs, rhs, HirType::F64,
    )))];
    let f = fn_decl("fadd", vec![], HirType::F64, body);
    let mir = lower_func(&f);

    let bb = &mir.blocks[0];
    let fadd = bb.insts.iter().filter(|i| matches!(i, Inst::FAdd { .. })).count();
    assert_eq!(fadd, 1);
}

#[test]
fn lower_comparison_emits_icmp_with_slt() {
    // a < b
    let body = vec![HirStmt::Return(Some(bin(
        HirBinOp::Lt,
        ident("a", HirType::I64),
        ident("b", HirType::I64),
        HirType::Bool,
    )))];
    let f = fn_decl(
        "lt",
        vec![("a", HirType::I64), ("b", HirType::I64)],
        HirType::Bool,
        body,
    );
    let mir = lower_func(&f);
    let bb = &mir.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(
        i,
        Inst::ICmp {
            cond: IntCond::Slt,
            ..
        }
    )));
}

#[test]
fn lower_if_else_creates_three_extra_blocks() {
    // function pick(c:bool, a:i64, b:i64): i64 { if (c) return a; else return b; }
    let body = vec![HirStmt::If {
        cond: ident("c", HirType::Bool),
        then: vec![HirStmt::Return(Some(ident("a", HirType::I64)))],
        else_: Some(vec![HirStmt::Return(Some(ident("b", HirType::I64)))]),
    }];
    let f = fn_decl(
        "pick",
        vec![
            ("c", HirType::Bool),
            ("a", HirType::I64),
            ("b", HirType::I64),
        ],
        HirType::I64,
        body,
    );
    let mir = lower_func(&f);

    // entry + then + else + join
    assert_eq!(mir.blocks.len(), 4);
    // entry should branch with Brif
    assert!(matches!(mir.blocks[0].term, Terminator::Brif { .. }));
    // then and else both terminate via Return (they don't fall through to join)
    assert!(matches!(mir.blocks[1].term, Terminator::Return(_)));
    assert!(matches!(mir.blocks[2].term, Terminator::Return(_)));
}

#[test]
fn lower_let_binding_then_use() {
    // function f(): i64 { let x = 10; return x + 1; }
    let body = vec![
        HirStmt::Let {
            name: "x".into(),
            ty: HirType::I64,
            init: Some(lit_int(10)),
        },
        HirStmt::Return(Some(bin(
            HirBinOp::Add,
            ident("x", HirType::I64),
            lit_int(1),
            HirType::I64,
        ))),
    ];
    let f = fn_decl("f", vec![], HirType::I64, body);
    let mir = lower_func(&f);

    let bb = &mir.blocks[0];
    // 2 IConst (10 and 1) + 1 IAdd
    let iconsts = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::IConst { .. }))
        .count();
    let iadds = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::IAdd { .. }))
        .count();
    assert_eq!(iconsts, 2);
    assert_eq!(iadds, 1);
}

#[test]
fn lower_while_creates_header_body_exit() {
    // while (c) { x = x + 1; } — uses pre-existing locals
    let body = vec![
        HirStmt::Let {
            name: "x".into(),
            ty: HirType::I64,
            init: Some(lit_int(0)),
        },
        HirStmt::While {
            cond: lit_bool(true),
            body: vec![HirStmt::Expr(bin(
                HirBinOp::Add,
                ident("x", HirType::I64),
                lit_int(1),
                HirType::I64,
            ))],
        },
        HirStmt::Return(Some(ident("x", HirType::I64))),
    ];
    let f = fn_decl("loop_fn", vec![], HirType::I64, body);
    let mir = lower_func(&f);

    // entry + header + body + exit + (no extra) = 4 blocks
    assert_eq!(mir.blocks.len(), 4);
    // entry jumps to header
    assert!(matches!(mir.blocks[0].term, Terminator::Jump { .. }));
    // header brifs
    assert!(matches!(mir.blocks[1].term, Terminator::Brif { .. }));
    // body jumps back to header (forming the loop)
    assert!(matches!(mir.blocks[2].term, Terminator::Jump { .. }));
    // exit returns
    assert!(matches!(mir.blocks[3].term, Terminator::Return(_)));
}

#[test]
fn lower_unsupported_stmt_traps() {
    // throw is not yet supported
    let body = vec![HirStmt::Throw(lit_int(0))];
    let f = fn_decl("bad", vec![], HirType::Void, body);
    let mir = lower_func(&f);
    assert!(matches!(
        mir.blocks[0].term,
        Terminator::Trap { code: TrapHint::User(_) }
    ));
}

#[test]
fn lower_negation_emits_ineg() {
    let body = vec![HirStmt::Return(Some(HirExpr::new(
        HirExprKind::Unary {
            op: rts_hir::ir::HirUnOp::Neg,
            operand: Box::new(ident("a", HirType::I64)),
        },
        HirType::I64,
    )))];
    let f = fn_decl("neg", vec![("a", HirType::I64)], HirType::I64, body);
    let mir = lower_func(&f);
    let bb = &mir.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(i, Inst::INeg { .. })));
}

// ---------- Pass tests ----------

fn build_const_add(a: i64, b: i64) -> MirFunc {
    let mut f = MirFunc::new("c_add", CallConvHint::Tail, HirType::I64);
    let v0 = f.new_value(HirType::I64);
    let v1 = f.new_value(HirType::I64);
    let v2 = f.new_value(HirType::I64);
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IConst { dst: v0, ty: HirType::I64, val: a });
    bb.insts.push(Inst::IConst { dst: v1, ty: HirType::I64, val: b });
    bb.insts.push(Inst::IAdd { dst: v2, lhs: v0, rhs: v1 });
    bb.term = Terminator::Return(vec![v2]);
    f
}

#[test]
fn fold_iadd_consts() {
    let mut f = build_const_add(40, 2);
    fold(&mut f);
    let bb = &f.blocks[0];
    // The IAdd should have been folded into an IConst { val: 42 }
    let folded = bb.insts.iter().any(|i| matches!(
        i,
        Inst::IConst { val: 42, .. }
    ));
    assert!(folded, "expected folded IConst 42, got {:?}", bb.insts);
}

#[test]
fn fold_imul_pow2_to_ishlimm() {
    let mut f = MirFunc::new("m", CallConvHint::Tail, HirType::I64);
    let x = f.new_value(HirType::I64);
    let c = f.new_value(HirType::I64);
    let r = f.new_value(HirType::I64);
    f.params.push((x, HirType::I64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((x, HirType::I64));
    bb.insts.push(Inst::IConst { dst: c, ty: HirType::I64, val: 8 });
    bb.insts.push(Inst::IMul { dst: r, lhs: x, rhs: c });
    bb.term = Terminator::Return(vec![r]);

    fold(&mut f);
    let bb = &f.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(
        i,
        Inst::IShlImm { imm: 3, .. }
    )), "expected IShlImm 3, got {:?}", bb.insts);
}

#[test]
fn fold_urem_pow2_to_band() {
    let mut f = MirFunc::new("u", CallConvHint::Tail, HirType::I64);
    let x = f.new_value(HirType::I64);
    let c = f.new_value(HirType::I64);
    let r = f.new_value(HirType::I64);
    f.params.push((x, HirType::I64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((x, HirType::I64));
    bb.insts.push(Inst::IConst { dst: c, ty: HirType::I64, val: 16 });
    bb.insts.push(Inst::URem { dst: r, lhs: x, rhs: c });
    bb.term = Terminator::Return(vec![r]);

    fold(&mut f);
    let bb = &f.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(
        i,
        Inst::BAndImm { imm: 15, .. }
    )));
}

#[test]
fn fold_band_with_zero_to_iconst_zero() {
    let mut f = MirFunc::new("z", CallConvHint::Tail, HirType::I64);
    let x = f.new_value(HirType::I64);
    let c = f.new_value(HirType::I64);
    let r = f.new_value(HirType::I64);
    f.params.push((x, HirType::I64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((x, HirType::I64));
    bb.insts.push(Inst::IConst { dst: c, ty: HirType::I64, val: 0 });
    bb.insts.push(Inst::BAnd { dst: r, lhs: x, rhs: c });
    bb.term = Terminator::Return(vec![r]);

    fold(&mut f);
    let bb = &f.blocks[0];
    let zero_consts = bb.insts.iter().filter(|i| matches!(
        i,
        Inst::IConst { val: 0, .. }
    )).count();
    // Original IConst 0 + folded BAnd → IConst 0 = 2 zero consts.
    assert_eq!(zero_consts, 2);
}

#[test]
fn fold_iadd_with_one_const_to_iaddimm() {
    let mut f = MirFunc::new("a1", CallConvHint::Tail, HirType::I64);
    let x = f.new_value(HirType::I64);
    let c = f.new_value(HirType::I64);
    let r = f.new_value(HirType::I64);
    f.params.push((x, HirType::I64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((x, HirType::I64));
    bb.insts.push(Inst::IConst { dst: c, ty: HirType::I64, val: 5 });
    bb.insts.push(Inst::IAdd { dst: r, lhs: x, rhs: c });
    bb.term = Terminator::Return(vec![r]);

    fold(&mut f);
    let bb = &f.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(
        i,
        Inst::IAddImm { imm: 5, .. }
    )));
}

#[test]
fn dce_removes_unused_iconst() {
    let mut f = MirFunc::new("d", CallConvHint::Tail, HirType::I64);
    let used = f.new_value(HirType::I64);
    let dead = f.new_value(HirType::I64);
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IConst { dst: used, ty: HirType::I64, val: 1 });
    bb.insts.push(Inst::IConst { dst: dead, ty: HirType::I64, val: 2 });
    bb.term = Terminator::Return(vec![used]);

    dce(&mut f);
    let bb = &f.blocks[0];
    assert_eq!(bb.insts.len(), 1, "dead IConst should be removed");
    assert!(matches!(bb.insts[0], Inst::IConst { val: 1, .. }));
}

#[test]
fn dce_keeps_store_even_with_unused_dst_chain() {
    let mut f = MirFunc::new("s", CallConvHint::Tail, HirType::Void);
    let p = f.new_value(HirType::I64);
    let v = f.new_value(HirType::I64);
    f.params.push((p, HirType::I64));
    f.params.push((v, HirType::I64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((p, HirType::I64));
    bb.params.push((v, HirType::I64));
    bb.insts.push(Inst::Store { val: v, ptr: p, offset: 0, flags: MemHint::Default });
    bb.term = Terminator::Return(vec![]);

    dce(&mut f);
    let bb = &f.blocks[0];
    assert_eq!(bb.insts.len(), 1);
    assert!(matches!(bb.insts[0], Inst::Store { .. }));
}

#[test]
fn dce_iterates_to_fixed_point() {
    // a -> b -> c chain where c is unused. DCE should remove c, then b, then a.
    let mut f = MirFunc::new("chain", CallConvHint::Tail, HirType::I64);
    let final_v = f.new_value(HirType::I64);
    let a = f.new_value(HirType::I64);
    let b = f.new_value(HirType::I64);
    let c = f.new_value(HirType::I64);
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IConst { dst: final_v, ty: HirType::I64, val: 99 });
    bb.insts.push(Inst::IConst { dst: a, ty: HirType::I64, val: 1 });
    bb.insts.push(Inst::IAddImm { dst: b, lhs: a, imm: 1 });
    bb.insts.push(Inst::IAddImm { dst: c, lhs: b, imm: 1 });
    bb.term = Terminator::Return(vec![final_v]);

    dce(&mut f);
    let bb = &f.blocks[0];
    // Only `final_v` IConst remains
    assert_eq!(bb.insts.len(), 1);
    assert!(matches!(bb.insts[0], Inst::IConst { val: 99, .. }));
}

#[test]
fn optimize_pipeline_runs_fold_then_dce() {
    // Build: x + 0  → after fold: BAndImm? no — IAddImm 0
    // Actually let's do: 2*8 = 16 (consts) + dead IConst → optimize collapses
    let mut f = MirFunc::new("p", CallConvHint::Tail, HirType::I64);
    let a = f.new_value(HirType::I64);
    let b = f.new_value(HirType::I64);
    let r = f.new_value(HirType::I64);
    let dead = f.new_value(HirType::I64);
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IConst { dst: a, ty: HirType::I64, val: 2 });
    bb.insts.push(Inst::IConst { dst: b, ty: HirType::I64, val: 8 });
    bb.insts.push(Inst::IMul { dst: r, lhs: a, rhs: b });
    bb.insts.push(Inst::IConst { dst: dead, ty: HirType::I64, val: 1234 });
    bb.term = Terminator::Return(vec![r]);

    optimize(&mut f);
    let bb = &f.blocks[0];
    // Only the folded IConst 16 should remain
    assert_eq!(bb.insts.len(), 1);
    assert!(matches!(bb.insts[0], Inst::IConst { val: 16, .. }));
}

// ---------- verify tests ----------

#[test]
fn verify_accepts_well_formed_function() {
    let f = build_const_add(1, 2);
    assert_eq!(verify(&f), Ok(()));
}

#[test]
fn verify_lowered_function() {
    // A real lowered fn from HIR — a smoke test that lower output passes verify.
    let body = vec![HirStmt::Return(Some(bin(
        HirBinOp::Add,
        ident("a", HirType::I64),
        lit_int(1),
        HirType::I64,
    )))];
    let f = fn_decl("inc", vec![("a", HirType::I64)], HirType::I64, body);
    let mir = lower_func(&f);
    assert_eq!(verify(&mir), Ok(()));
}

#[test]
fn verify_rejects_value_out_of_range() {
    let mut f = MirFunc::new("bad", CallConvHint::Tail, HirType::I64);
    let blk = f.new_block();
    f.blocks[blk as usize].insts.push(Inst::IConst {
        dst: 99, // not allocated
        ty: HirType::I64,
        val: 0,
    });
    f.blocks[blk as usize].term = Terminator::Return(vec![]);
    assert!(matches!(
        verify(&f),
        Err(VerifyError::ValueOutOfRange { value: 99, .. })
    ));
}

#[test]
fn verify_rejects_block_id_mismatch() {
    let mut f = MirFunc::new("bad", CallConvHint::Tail, HirType::Void);
    let _ = f.new_block();
    f.blocks[0].id = 7; // doesn't match position
    assert!(matches!(
        verify(&f),
        Err(VerifyError::BlockIdMismatch { idx: 0, declared: 7 })
    ));
}

#[test]
fn verify_rejects_brif_with_invalid_block() {
    let mut f = MirFunc::new("bad", CallConvHint::Tail, HirType::Void);
    let cond = f.new_value(HirType::Bool);
    let blk = f.new_block();
    f.blocks[blk as usize].insts.push(Inst::IConst {
        dst: cond,
        ty: HirType::Bool,
        val: 0,
    });
    f.blocks[blk as usize].term = Terminator::Brif {
        cond,
        then_block: 99,
        then_args: vec![],
        else_block: 100,
        else_args: vec![],
    };
    assert!(matches!(
        verify(&f),
        Err(VerifyError::BlockOutOfRange { .. })
    ));
}

#[test]
fn verify_passes_after_optimize() {
    // Run lower + optimize and verify still holds.
    let body = vec![HirStmt::Return(Some(bin(
        HirBinOp::Mul,
        lit_int(2),
        lit_int(8),
        HirType::I64,
    )))];
    let f = fn_decl("k", vec![], HirType::I64, body);
    let mut mir = lower_func(&f);
    optimize(&mut mir);
    assert_eq!(verify(&mir), Ok(()));
}

// ---------- narrow tests ----------

#[test]
fn narrow_i8_iadd_inserts_mask() {
    // dst: I8 = a + b
    let mut f = MirFunc::new("n8", CallConvHint::Tail, HirType::I8);
    let a = f.new_value(HirType::I8);
    let b = f.new_value(HirType::I8);
    let r = f.new_value(HirType::I8);
    f.params.push((a, HirType::I8));
    f.params.push((b, HirType::I8));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::I8));
    f.blocks[blk as usize].params.push((b, HirType::I8));
    f.blocks[blk as usize].insts.push(Inst::IAdd { dst: r, lhs: a, rhs: b });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);

    narrow(&mut f);
    let bb = &f.blocks[0];
    assert_eq!(bb.insts.len(), 2);
    assert!(matches!(bb.insts[0], Inst::IAdd { .. }));
    assert!(matches!(
        bb.insts[1],
        Inst::BAndImm { imm: 0xFF, .. }
    ));
}

#[test]
fn narrow_idempotent_no_double_mask() {
    let mut f = MirFunc::new("nidem", CallConvHint::Tail, HirType::I8);
    let a = f.new_value(HirType::I8);
    let b = f.new_value(HirType::I8);
    let r = f.new_value(HirType::I8);
    f.params.push((a, HirType::I8));
    f.params.push((b, HirType::I8));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::I8));
    f.blocks[blk as usize].params.push((b, HirType::I8));
    f.blocks[blk as usize].insts.push(Inst::IAdd { dst: r, lhs: a, rhs: b });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);

    narrow(&mut f);
    narrow(&mut f);
    narrow(&mut f);
    let bb = &f.blocks[0];
    // 3 passadas nao acumulam masks
    assert_eq!(bb.insts.len(), 2, "narrow deve ser idempotente");
}

#[test]
fn narrow_i16_imul_inserts_mask_0xffff() {
    let mut f = MirFunc::new("n16", CallConvHint::Tail, HirType::I16);
    let a = f.new_value(HirType::I16);
    let b = f.new_value(HirType::I16);
    let r = f.new_value(HirType::I16);
    f.params.push((a, HirType::I16));
    f.params.push((b, HirType::I16));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::I16));
    f.blocks[blk as usize].params.push((b, HirType::I16));
    f.blocks[blk as usize].insts.push(Inst::IMul { dst: r, lhs: a, rhs: b });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);

    narrow(&mut f);
    let bb = &f.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(
        i,
        Inst::BAndImm { imm: 0xFFFF, .. }
    )));
}

#[test]
fn narrow_i64_iadd_does_not_insert_mask() {
    let mut f = MirFunc::new("n64", CallConvHint::Tail, HirType::I64);
    let a = f.new_value(HirType::I64);
    let b = f.new_value(HirType::I64);
    let r = f.new_value(HirType::I64);
    f.params.push((a, HirType::I64));
    f.params.push((b, HirType::I64));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::I64));
    f.blocks[blk as usize].params.push((b, HirType::I64));
    f.blocks[blk as usize].insts.push(Inst::IAdd { dst: r, lhs: a, rhs: b });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);

    narrow(&mut f);
    let bb = &f.blocks[0];
    assert_eq!(bb.insts.len(), 1, "I64 ops should not be masked");
}

// ---------- expanded lowering tests ----------

#[test]
fn lower_for_classic_creates_init_header_body_update_exit() {
    // for (let i = 0; i < 10; i++) { ... }
    let body = vec![HirStmt::For {
        init: Some(Box::new(HirStmt::Let {
            name: "i".into(),
            ty: HirType::I64,
            init: Some(lit_int(0)),
        })),
        cond: Some(bin(
            HirBinOp::Lt,
            ident("i", HirType::I64),
            lit_int(10),
            HirType::Bool,
        )),
        update: Some(HirExpr::new(
            HirExprKind::PostInc(Box::new(ident("i", HirType::I64))),
            HirType::I64,
        )),
        body: vec![],
    }];
    let f = fn_decl("for_fn", vec![], HirType::Void, body);
    let mir = lower_func(&f);

    // entry + header + body + update + exit = 5 blocks
    assert_eq!(mir.blocks.len(), 5);
    assert!(matches!(mir.blocks[0].term, Terminator::Jump { .. })); // entry → header
    assert!(matches!(mir.blocks[1].term, Terminator::Brif { .. })); // header
    assert!(matches!(mir.blocks[2].term, Terminator::Jump { .. })); // body → update
    assert!(matches!(mir.blocks[3].term, Terminator::Jump { .. })); // update → header
    assert!(matches!(mir.blocks[4].term, Terminator::Return(_))); // exit
}

#[test]
fn lower_do_while_executes_body_first() {
    // do { } while (false)
    let body = vec![HirStmt::DoWhile {
        body: vec![],
        cond: lit_bool(false),
    }];
    let f = fn_decl("dw", vec![], HirType::Void, body);
    let mir = lower_func(&f);
    // entry + body + header + exit = 4
    assert_eq!(mir.blocks.len(), 4);
    // entry jumps to body (not header)
    if let Terminator::Jump { target, .. } = &mir.blocks[0].term {
        assert_eq!(*target, 1, "entry should jump to body block");
    } else {
        panic!("entry must Jump");
    }
    // body jumps to header
    assert!(matches!(mir.blocks[1].term, Terminator::Jump { .. }));
    // header brifs
    assert!(matches!(mir.blocks[2].term, Terminator::Brif { .. }));
}

#[test]
fn lower_break_in_while_jumps_to_exit() {
    // while (true) { break; }
    let body = vec![HirStmt::While {
        cond: lit_bool(true),
        body: vec![HirStmt::Break(None)],
    }];
    let f = fn_decl("br", vec![], HirType::Void, body);
    let mir = lower_func(&f);

    // body block (block index 2 in: entry/header/body/exit)
    let body_b = &mir.blocks[2];
    if let Terminator::Jump { target, .. } = &body_b.term {
        // should jump to the exit block (which exists at index 3)
        assert_eq!(*target, 3, "break should target exit block");
    } else {
        panic!("body must Jump (break)");
    }
}

#[test]
fn lower_continue_in_while_jumps_to_header() {
    // while (true) { continue; }
    let body = vec![HirStmt::While {
        cond: lit_bool(true),
        body: vec![HirStmt::Continue(None)],
    }];
    let f = fn_decl("cont", vec![], HirType::Void, body);
    let mir = lower_func(&f);

    // body should jump to header (block 1)
    let body_b = &mir.blocks[2];
    if let Terminator::Jump { target, .. } = &body_b.term {
        assert_eq!(*target, 1, "continue should target header block");
    } else {
        panic!("body must Jump (continue)");
    }
}

#[test]
fn lower_ternary_emits_select() {
    // function pick(c, a, b): i64 { return c ? a : b; }
    let tern = HirExpr::new(
        HirExprKind::Ternary {
            cond: Box::new(ident("c", HirType::Bool)),
            then: Box::new(ident("a", HirType::I64)),
            else_: Box::new(ident("b", HirType::I64)),
        },
        HirType::I64,
    );
    let body = vec![HirStmt::Return(Some(tern))];
    let f = fn_decl(
        "pick",
        vec![
            ("c", HirType::Bool),
            ("a", HirType::I64),
            ("b", HirType::I64),
        ],
        HirType::I64,
        body,
    );
    let mir = lower_func(&f);
    let bb = &mir.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(i, Inst::Select { .. })));
}

// ---------- switch / try / throw tests ----------

#[test]
fn lower_switch_with_two_cases_and_default() {
    use rts_hir::ir::HirSwitchCase;
    // switch (n) { case 1: case 2: ; default: ; }
    let body = vec![HirStmt::Switch {
        discriminant: ident("n", HirType::I64),
        cases: vec![
            HirSwitchCase {
                test: Some(lit_int(1)),
                body: vec![],
            },
            HirSwitchCase {
                test: Some(lit_int(2)),
                body: vec![],
            },
            HirSwitchCase {
                test: None, // default
                body: vec![],
            },
        ],
    }];
    let f = fn_decl("sw", vec![("n", HirType::I64)], HirType::Void, body);
    let mir = lower_func(&f);

    // entry + 3 case blocks + exit = 5 blocks
    assert_eq!(mir.blocks.len(), 5);
    // entry terminates with Switch
    if let Terminator::Switch { cases, default, .. } = &mir.blocks[0].term {
        assert_eq!(cases.len(), 2); // 2 non-default cases
        assert_eq!(cases[0].0, 1);
        assert_eq!(cases[1].0, 2);
        assert_eq!(*default, 3); // default points to the third case block
    } else {
        panic!("entry must Switch, got {:?}", mir.blocks[0].term);
    }
}

#[test]
fn lower_switch_without_default_routes_to_exit() {
    use rts_hir::ir::HirSwitchCase;
    let body = vec![HirStmt::Switch {
        discriminant: lit_int(0),
        cases: vec![HirSwitchCase {
            test: Some(lit_int(7)),
            body: vec![],
        }],
    }];
    let f = fn_decl("sw2", vec![], HirType::Void, body);
    let mir = lower_func(&f);

    if let Terminator::Switch { default, cases, .. } = &mir.blocks[0].term {
        assert_eq!(cases.len(), 1);
        // exit block is the last (case + exit = 2 extra blocks: indices 1 and 2)
        // default should point to the exit block (index 2)
        assert_eq!(*default, 2, "no-default switch should fall to exit");
    } else {
        panic!("expected Switch terminator");
    }
}

#[test]
fn lower_switch_falls_through_between_cases() {
    use rts_hir::ir::HirSwitchCase;
    // switch (x) { case 1: ; case 2: ; }  // no break → fallthrough
    let body = vec![HirStmt::Switch {
        discriminant: ident("x", HirType::I64),
        cases: vec![
            HirSwitchCase {
                test: Some(lit_int(1)),
                body: vec![],
            },
            HirSwitchCase {
                test: Some(lit_int(2)),
                body: vec![],
            },
        ],
    }];
    let f = fn_decl("ft", vec![("x", HirType::I64)], HirType::Void, body);
    let mir = lower_func(&f);

    // case0 (block 1) should Jump to case1 (block 2)
    if let Terminator::Jump { target, .. } = &mir.blocks[1].term {
        assert_eq!(*target, 2, "case 0 should fall through to case 1");
    } else {
        panic!("case 0 should Jump (fallthrough), got {:?}", mir.blocks[1].term);
    }
    // last case should Jump to exit (block 3)
    if let Terminator::Jump { target, .. } = &mir.blocks[2].term {
        assert_eq!(*target, 3, "last case should jump to exit");
    } else {
        panic!("last case should Jump to exit");
    }
}

#[test]
fn lower_switch_break_exits() {
    use rts_hir::ir::HirSwitchCase;
    // switch (x) { case 1: break; }
    let body = vec![HirStmt::Switch {
        discriminant: ident("x", HirType::I64),
        cases: vec![HirSwitchCase {
            test: Some(lit_int(1)),
            body: vec![HirStmt::Break(None)],
        }],
    }];
    let f = fn_decl("br", vec![("x", HirType::I64)], HirType::Void, body);
    let mir = lower_func(&f);

    // case 0 (block 1) should jump to exit (break, block 2)
    if let Terminator::Jump { target, .. } = &mir.blocks[1].term {
        assert_eq!(*target, 2, "break in switch should jump to exit");
    } else {
        panic!("expected Jump from case 0 (break)");
    }
}

#[test]
fn lower_throw_emits_trap() {
    let body = vec![HirStmt::Throw(lit_int(0))];
    let f = fn_decl("th", vec![], HirType::Void, body);
    let mir = lower_func(&f);
    assert!(matches!(
        mir.blocks[0].term,
        Terminator::Trap { code: TrapHint::User(_) }
    ));
}

#[test]
fn lower_try_phase1_lowers_body_only() {
    use rts_hir::ir::HirCatch;
    // try { return 1; } catch (e) { return 2; } finally { ; }
    // Phase 1: catch is ignored, finally runs only on normal completion
    // (which won't happen because body returns).
    let body = vec![HirStmt::Try {
        body: vec![HirStmt::Return(Some(lit_int(1)))],
        catch: Some(HirCatch {
            binding: Some("e".into()),
            body: vec![HirStmt::Return(Some(lit_int(2)))],
        }),
        finally: Some(vec![HirStmt::Expr(lit_int(99))]),
    }];
    let f = fn_decl("tt", vec![], HirType::I64, body);
    let mir = lower_func(&f);
    // Single block, terminated by Return 1
    assert_eq!(mir.blocks.len(), 1);
    assert!(matches!(mir.blocks[0].term, Terminator::Return(_)));
}

#[test]
fn lower_try_with_finally_runs_finally_after_body() {
    // try { } finally { let x = 99; }
    let body = vec![HirStmt::Try {
        body: vec![],
        catch: None,
        finally: Some(vec![HirStmt::Let {
            name: "x".into(),
            ty: HirType::I64,
            init: Some(lit_int(99)),
        }]),
    }];
    let f = fn_decl("fin", vec![], HirType::Void, body);
    let mir = lower_func(&f);
    // The IConst 99 from `let x = 99` should be in the entry block
    let bb = &mir.blocks[0];
    assert!(bb.insts.iter().any(|i| matches!(
        i,
        Inst::IConst { val: 99, .. }
    )));
}

// ---------- FMA fusion ----------

#[test]
fn fma_fuses_fmul_then_fadd() {
    use crate::passes::fma;

    // a * b + c → Fma(a, b, c)
    let mut f = MirFunc::new("compute", CallConvHint::Tail, HirType::F64);
    let a = f.new_value(HirType::F64);
    let b = f.new_value(HirType::F64);
    let c = f.new_value(HirType::F64);
    let prod = f.new_value(HirType::F64);
    let result = f.new_value(HirType::F64);
    f.params.push((a, HirType::F64));
    f.params.push((b, HirType::F64));
    f.params.push((c, HirType::F64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((a, HirType::F64));
    bb.params.push((b, HirType::F64));
    bb.params.push((c, HirType::F64));
    bb.insts.push(Inst::FMul { dst: prod, lhs: a, rhs: b });
    bb.insts.push(Inst::FAdd { dst: result, lhs: prod, rhs: c });
    bb.term = Terminator::Return(vec![result]);

    let changed = fma(&mut f);
    assert!(changed);
    let bb = &f.blocks[0];
    let has_fma = bb.insts.iter().any(|i| matches!(i, Inst::Fma { .. }));
    let has_fmul = bb.insts.iter().any(|i| matches!(i, Inst::FMul { .. }));
    let has_fadd = bb.insts.iter().any(|i| matches!(i, Inst::FAdd { .. }));
    assert!(has_fma);
    assert!(!has_fmul, "FMul deve ter sido removido após fusion");
    assert!(!has_fadd, "FAdd deve ter sido substituído por Fma");
}

#[test]
fn fma_skips_fmul_with_multiple_uses() {
    use crate::passes::fma;

    // prod = a * b
    // r1 = prod + c
    // r2 = prod + d  // segundo uso do prod → não pode fundir
    let mut f = MirFunc::new("dup", CallConvHint::Tail, HirType::F64);
    let a = f.new_value(HirType::F64);
    let b = f.new_value(HirType::F64);
    let c = f.new_value(HirType::F64);
    let d = f.new_value(HirType::F64);
    let prod = f.new_value(HirType::F64);
    let r1 = f.new_value(HirType::F64);
    let r2 = f.new_value(HirType::F64);
    let r3 = f.new_value(HirType::F64);
    f.params.push((a, HirType::F64));
    f.params.push((b, HirType::F64));
    f.params.push((c, HirType::F64));
    f.params.push((d, HirType::F64));
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.params.push((a, HirType::F64));
    bb.params.push((b, HirType::F64));
    bb.params.push((c, HirType::F64));
    bb.params.push((d, HirType::F64));
    bb.insts.push(Inst::FMul { dst: prod, lhs: a, rhs: b });
    bb.insts.push(Inst::FAdd { dst: r1, lhs: prod, rhs: c });
    bb.insts.push(Inst::FAdd { dst: r2, lhs: prod, rhs: d });
    bb.insts.push(Inst::FAdd { dst: r3, lhs: r1, rhs: r2 });
    bb.term = Terminator::Return(vec![r3]);

    fma(&mut f);
    let bb = &f.blocks[0];
    // FMul deveria sobreviver pq tem 2 uses
    let has_fmul = bb.insts.iter().any(|i| matches!(i, Inst::FMul { .. }));
    assert!(has_fmul, "FMul com múltiplos uses não pode virar FMA");
}

// ---------- CSE pass ----------

#[test]
fn cse_dedups_iconst() {
    use crate::passes::cse;

    let mut f = MirFunc::new("c", CallConvHint::Tail, HirType::I64);
    let v1 = f.new_value(HirType::I64);
    let v2 = f.new_value(HirType::I64);
    let v3 = f.new_value(HirType::I64);
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IConst { dst: v1, ty: HirType::I64, val: 5 });
    bb.insts.push(Inst::IConst { dst: v2, ty: HirType::I64, val: 5 });
    bb.insts.push(Inst::IAdd { dst: v3, lhs: v1, rhs: v2 });
    bb.term = Terminator::Return(vec![v3]);

    let changed = cse(&mut f);
    assert!(changed);

    // Após CSE, v2 vira IAddImm 0 v1 (alias) e o IAdd usa v1, v1.
    let bb = &f.blocks[0];
    if let Inst::IAdd { lhs, rhs, .. } = &bb.insts[2] {
        assert_eq!(*lhs, *rhs, "ambos lhs e rhs apontam pro mesmo IConst");
    } else {
        panic!("expected IAdd");
    }
}

#[test]
fn cse_dedups_iadd_same_operands() {
    use crate::passes::cse;

    let mut f = MirFunc::new("c", CallConvHint::Tail, HirType::I64);
    let a = f.new_value(HirType::I64);
    let r1 = f.new_value(HirType::I64);
    let r2 = f.new_value(HirType::I64);
    let r3 = f.new_value(HirType::I64);
    f.params.push((a, HirType::I64));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::I64));
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IAdd { dst: r1, lhs: a, rhs: a });
    bb.insts.push(Inst::IAdd { dst: r2, lhs: a, rhs: a });
    bb.insts.push(Inst::IAdd { dst: r3, lhs: r1, rhs: r2 });
    bb.term = Terminator::Return(vec![r3]);

    cse(&mut f);

    // r2 deve virar IAddImm 0 r1 (alias). O IAdd r3 usa r1 + r1 (depois
    // de remap r2 → r1).
    let bb = &f.blocks[0];
    if let Inst::IAdd { lhs, rhs, .. } = &bb.insts[2] {
        assert_eq!(*lhs, *rhs, "ambos operandos do IAdd r3 devem apontar pro mesmo r1");
    } else {
        panic!("expected IAdd at index 2");
    }
}

#[test]
fn cse_optimize_pipeline_eliminates_dups() {
    use crate::passes::optimize;

    let mut f = MirFunc::new("c", CallConvHint::Tail, HirType::I64);
    let v1 = f.new_value(HirType::I64);
    let v2 = f.new_value(HirType::I64);
    let v3 = f.new_value(HirType::I64);
    let blk = f.new_block();
    let bb = &mut f.blocks[blk as usize];
    bb.insts.push(Inst::IConst { dst: v1, ty: HirType::I64, val: 7 });
    bb.insts.push(Inst::IConst { dst: v2, ty: HirType::I64, val: 7 });
    bb.insts.push(Inst::IAdd { dst: v3, lhs: v1, rhs: v2 });
    bb.term = Terminator::Return(vec![v3]);

    optimize(&mut f);

    // optimize roda fold (constant folding 7+7=14) + cse + dce.
    // Após fold, IAdd vira IConst 14. CSE dedupa o IConst 7 duplicado.
    // DCE remove o IConst 7 não usado.
    let bb = &f.blocks[0];
    let iconsts = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::IConst { .. }))
        .count();
    // Após pipeline: pelo menos 1 IConst (o 14 final) e zero IAdd.
    assert!(iconsts >= 1);
    let iadds = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::IAdd { .. }))
        .count();
    assert_eq!(iadds, 0, "fold deveria ter colapsado o IAdd");
}

// ---------- inline pass ----------

#[test]
fn inline_replaces_callee_with_body() {
    use crate::passes::inline;
    use std::collections::HashMap;

    // callee: function inc(x: i64): i64 { return x + 1; }
    let mut callee = MirFunc::new("inc", CallConvHint::Tail, HirType::I64);
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

    // caller: function caller() -> i64 { let a = 5; let b = inc(a); return b; }
    let mut caller = MirFunc::new("caller", CallConvHint::Tail, HirType::I64);
    let a = caller.new_value(HirType::I64);
    let b = caller.new_value(HirType::I64);
    let blk_c = caller.new_block();
    caller.blocks[blk_c as usize].insts.push(Inst::IConst {
        dst: a,
        ty: HirType::I64,
        val: 5,
    });
    caller.blocks[blk_c as usize].insts.push(Inst::CallUser {
        dst: Some(b),
        name: "inc".into(),
        args: vec![a],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64],
    });
    caller.blocks[blk_c as usize].term = Terminator::Return(vec![b]);

    let mut callees: HashMap<String, MirFunc> = HashMap::new();
    callees.insert("inc".into(), callee);

    let changed = inline(&mut caller, &callees);
    assert!(changed, "inline deveria ter substituído CallUser");

    // Após inline: o body deveria ter IConst 5 + IAddImm 1 (e nao mais
    // CallUser).
    let bb = &caller.blocks[0];
    let has_call = bb.insts.iter().any(|i| matches!(i, Inst::CallUser { .. }));
    assert!(!has_call, "CallUser deveria ter sumido após inline");
    let has_addimm = bb.insts.iter().any(|i| matches!(
        i,
        Inst::IAddImm { imm: 1, .. }
    ));
    assert!(has_addimm, "corpo do callee (IAddImm 1) deveria estar inlined");
}

#[test]
fn inline_skips_recursive_callee() {
    use crate::passes::inline;
    use std::collections::HashMap;

    // callee recursivo: function f(x): i64 { return f(x); }
    let mut callee = MirFunc::new("f", CallConvHint::Tail, HirType::I64);
    let p = callee.new_value(HirType::I64);
    let r = callee.new_value(HirType::I64);
    callee.params.push((p, HirType::I64));
    let blk = callee.new_block();
    callee.blocks[blk as usize].params.push((p, HirType::I64));
    callee.blocks[blk as usize].insts.push(Inst::CallUser {
        dst: Some(r),
        name: "f".into(),
        args: vec![p],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64],
    });
    callee.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mut caller = MirFunc::new("caller", CallConvHint::Tail, HirType::I64);
    let a = caller.new_value(HirType::I64);
    let b = caller.new_value(HirType::I64);
    let blk_c = caller.new_block();
    caller.blocks[blk_c as usize].insts.push(Inst::IConst {
        dst: a,
        ty: HirType::I64,
        val: 5,
    });
    caller.blocks[blk_c as usize].insts.push(Inst::CallUser {
        dst: Some(b),
        name: "f".into(),
        args: vec![a],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64],
    });
    caller.blocks[blk_c as usize].term = Terminator::Return(vec![b]);

    let mut callees: HashMap<String, MirFunc> = HashMap::new();
    callees.insert("f".into(), callee);

    let changed = inline(&mut caller, &callees);
    assert!(!changed, "fn recursiva não deve ser inlinada");
    let has_call = caller.blocks[0].insts.iter().any(|i| matches!(
        i,
        Inst::CallUser { .. }
    ));
    assert!(has_call, "CallUser deveria sobreviver");
}

#[test]
fn inline_skips_callee_with_call_extern() {
    use crate::passes::inline;
    use std::collections::HashMap;

    // callee com CallExtern → bail
    let mut callee = MirFunc::new("c", CallConvHint::Tail, HirType::I64);
    let p = callee.new_value(HirType::I64);
    let r = callee.new_value(HirType::I64);
    callee.params.push((p, HirType::I64));
    let blk = callee.new_block();
    callee.blocks[blk as usize].params.push((p, HirType::I64));
    callee.blocks[blk as usize].insts.push(Inst::CallExtern {
        dst: Some(r),
        sym: "__RTS_FN_FOO".into(),
        args: vec![p],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64],
    });
    callee.blocks[blk as usize].term = Terminator::Return(vec![r]);

    let mut caller = MirFunc::new("caller", CallConvHint::Tail, HirType::I64);
    let a = caller.new_value(HirType::I64);
    let b = caller.new_value(HirType::I64);
    let blk_c = caller.new_block();
    caller.blocks[blk_c as usize].insts.push(Inst::IConst {
        dst: a,
        ty: HirType::I64,
        val: 1,
    });
    caller.blocks[blk_c as usize].insts.push(Inst::CallUser {
        dst: Some(b),
        name: "c".into(),
        args: vec![a],
        ret_ty: HirType::I64,
        param_tys: vec![HirType::I64],
    });
    caller.blocks[blk_c as usize].term = Terminator::Return(vec![b]);

    let mut callees: HashMap<String, MirFunc> = HashMap::new();
    callees.insert("c".into(), callee);
    let changed = inline(&mut caller, &callees);
    assert!(!changed);
}

// ---------- print tests ----------

#[test]
fn print_simple_function() {
    let f = build_const_add(40, 2);
    let out = format!("{}", f);
    assert!(out.contains("function c_add"));
    assert!(out.contains("block0:"));
    assert!(out.contains("iconst.i64 40"));
    assert!(out.contains("iconst.i64 2"));
    assert!(out.contains("iadd v0, v1"));
    assert!(out.contains("return v2"));
}

#[test]
fn print_after_fold_shows_iconst() {
    let mut f = build_const_add(40, 2);
    fold(&mut f);
    let out = format!("{}", f);
    // Folded into IConst 42
    assert!(out.contains("iconst.i64 42"));
}

#[test]
fn print_brif_terminator() {
    let mut f = MirFunc::new("br", CallConvHint::Tail, HirType::Void);
    let cond = f.new_value(HirType::Bool);
    let _t = f.new_block();
    let _e = f.new_block();
    let head = f.new_block();
    f.blocks[head as usize].insts.push(Inst::IConst {
        dst: cond,
        ty: HirType::Bool,
        val: 1,
    });
    f.blocks[head as usize].term = Terminator::Brif {
        cond,
        then_block: 0,
        then_args: vec![],
        else_block: 1,
        else_args: vec![],
    };
    let out = format!("{}", f);
    assert!(out.contains("brif v0, block0, block1"));
}

#[test]
fn lower_logical_not_emits_bxor_with_one() {
    let body = vec![HirStmt::Return(Some(HirExpr::new(
        HirExprKind::Unary {
            op: rts_hir::ir::HirUnOp::Not,
            operand: Box::new(ident("b", HirType::Bool)),
        },
        HirType::Bool,
    )))];
    let f = fn_decl("not_fn", vec![("b", HirType::Bool)], HirType::Bool, body);
    let mir = lower_func(&f);
    let bb = &mir.blocks[0];
    let bxors = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::BXor { .. }))
        .count();
    let one_consts = bb
        .insts
        .iter()
        .filter(|i| matches!(i, Inst::IConst { val: 1, ty: HirType::Bool, .. }))
        .count();
    assert_eq!(bxors, 1);
    assert_eq!(one_consts, 1);
}

#[test]
fn narrow_return_signed_inserts_sign_extend() {
    let mut f = MirFunc::new("retI8", CallConvHint::Tail, HirType::I8);
    let a = f.new_value(HirType::I8);
    let b = f.new_value(HirType::I8);
    let r = f.new_value(HirType::I8);
    f.params.push((a, HirType::I8));
    f.params.push((b, HirType::I8));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::I8));
    f.blocks[blk as usize].params.push((b, HirType::I8));
    f.blocks[blk as usize].insts.push(Inst::IAdd { dst: r, lhs: a, rhs: b });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);

    assert!(narrow_return(&mut f));
    let bb = &f.blocks[0];
    // IAdd + IShlImm 56 + SShrImm 56
    assert!(bb.insts.iter().any(|i| matches!(i, Inst::IShlImm { imm: 56, .. })));
    assert!(bb.insts.iter().any(|i| matches!(i, Inst::SShrImm { imm: 56, .. })));

    // Idempotente: chamar de novo nao adiciona novamente
    let n_before = bb.insts.len();
    narrow_return(&mut f);
    assert_eq!(f.blocks[0].insts.len(), n_before, "narrow_return idempotente");
}

#[test]
fn narrow_return_unsigned_no_sign_extend() {
    let mut f = MirFunc::new("retU8", CallConvHint::Tail, HirType::U8);
    let a = f.new_value(HirType::U8);
    let b = f.new_value(HirType::U8);
    let r = f.new_value(HirType::U8);
    f.params.push((a, HirType::U8));
    f.params.push((b, HirType::U8));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::U8));
    f.blocks[blk as usize].params.push((b, HirType::U8));
    f.blocks[blk as usize].insts.push(Inst::IAdd { dst: r, lhs: a, rhs: b });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);

    assert!(!narrow_return(&mut f), "U8 nao deve sign-extend");
    assert_eq!(f.blocks[0].insts.len(), 1);
}

// ---------- SIMD V128 smoke ----------

#[test]
fn simd_splat_extract_roundtrip() {
    use rts_hir::ir::VecKind;
    let mut f = MirFunc::new("simd_splat", CallConvHint::Tail, HirType::I32);
    let s = f.new_value(HirType::I32);
    let v = f.new_value(HirType::V128(VecKind::I32x4));
    let r = f.new_value(HirType::I32);
    f.params.push((s, HirType::I32));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((s, HirType::I32));
    f.blocks[blk as usize].insts.push(Inst::VSplat { dst: v, src: s, kind: VecKind::I32x4 });
    f.blocks[blk as usize].insts.push(Inst::VExtractLane {
        dst: r, src: v, lane: 2, kind: VecKind::I32x4,
    });
    f.blocks[blk as usize].term = Terminator::Return(vec![r]);
    verify(&f).expect("verify");
}

#[test]
fn simd_vadd_dce_keeps_used_values() {
    use rts_hir::ir::VecKind;
    let mut f = MirFunc::new("simd_add", CallConvHint::Tail, HirType::V128(VecKind::F32x4));
    let a = f.new_value(HirType::F32);
    let b = f.new_value(HirType::F32);
    let va = f.new_value(HirType::V128(VecKind::F32x4));
    let vb = f.new_value(HirType::V128(VecKind::F32x4));
    let vr = f.new_value(HirType::V128(VecKind::F32x4));
    f.params.push((a, HirType::F32));
    f.params.push((b, HirType::F32));
    let blk = f.new_block();
    f.blocks[blk as usize].params.push((a, HirType::F32));
    f.blocks[blk as usize].params.push((b, HirType::F32));
    f.blocks[blk as usize].insts.push(Inst::VSplat { dst: va, src: a, kind: VecKind::F32x4 });
    f.blocks[blk as usize].insts.push(Inst::VSplat { dst: vb, src: b, kind: VecKind::F32x4 });
    f.blocks[blk as usize].insts.push(Inst::VAdd { dst: vr, lhs: va, rhs: vb, kind: VecKind::F32x4 });
    f.blocks[blk as usize].term = Terminator::Return(vec![vr]);

    let before = f.blocks[0].insts.len();
    dce(&mut f);
    assert_eq!(f.blocks[0].insts.len(), before, "DCE não deve remover SIMD em uso");
}
