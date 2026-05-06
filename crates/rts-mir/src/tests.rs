//! Smoke tests for MIR construction primitives.
//!
//! Lower de HIR e passes virão em sub-etapas seguintes; aqui só validamos
//! os ctors e os helpers de construção.

use crate::ir::*;
use crate::lower::lower_func;
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
