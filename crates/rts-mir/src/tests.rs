//! Smoke tests for MIR construction primitives.
//!
//! Lower de HIR e passes virão em sub-etapas seguintes; aqui só validamos
//! os ctors e os helpers de construção.

use crate::ir::*;
use rts_hir::HirType;

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
