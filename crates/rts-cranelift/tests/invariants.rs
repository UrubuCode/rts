//! The invariants, exercised end to end.
//!
//! These are the claims the machine layer makes about what it will and will not
//! represent. They run with no client present, which is the property that makes
//! a slow or wrong program attributable to the layer above rather than to this
//! one.

use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::ir::{
    BuildError, ConstDecl, FuncBuilder, Function, NumOp, Region, ScalarBits, Signature, Terminator,
};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::verify::{VerifyError, is_valid, verify};

/// A function taking the given parameters and returning the given values.
fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    })
}

/// The value bound to the entry block's parameter at `index`.
fn param(func: &Function, index: usize) -> rts_cranelift::ir::ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

#[test]
fn a_well_formed_function_verifies_clean() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let sum = b
        .arith(NumOp::Add, x, y)
        .expect("both operands are proven f64");
    b.ret(&[sum]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
}

#[test]
fn proven_arithmetic_refuses_a_generic_operand() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::Tagged], &[Repr::F64]);
    let (x, unknown) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    assert_eq!(
        b.arith(NumOp::Add, x, unknown),
        Err(BuildError::GenericOperand { operation: "arith" }),
        "there is no addition that accepts both; that branch is the point"
    );
}

#[test]
fn proven_arithmetic_refuses_operands_that_disagree() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32, Repr::F64], &[Repr::I32]);
    let (i, f) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    assert_eq!(
        b.arith(NumOp::Add, i, f),
        Err(BuildError::MixedOperands {
            operation: "arith",
            left: Repr::I32,
            right: Repr::F64
        })
    );
}

#[test]
fn a_merge_to_the_generic_form_widens_automatically() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let merge = b.create_block();
    b.add_block_param(merge, Repr::Tagged);
    b.jump(merge, &[x])
        .expect("widening is inserted, not demanded of the caller");

    b.switch_to(merge);
    let merged = func.block(merge).expect("merge exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, merge);
    b.ret(&[merged]);

    assert!(is_valid(&func, &types, &FuncRegistry::new()));
}

#[test]
fn a_merge_that_would_narrow_is_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::I32]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let target = b.create_block();
    b.add_block_param(target, Repr::I32);

    assert_eq!(
        b.jump(target, &[unknown]),
        Err(BuildError::ImplicitNarrowing {
            target,
            position: 0,
            expected: Repr::I32,
            found: Repr::Tagged,
        }),
        "narrowing can fail at run time, so it is only reachable through a guard"
    );
}

#[test]
fn a_guard_hands_the_narrowed_value_to_its_success_block() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::I32);
    b.guard(unknown, Repr::I32, (ok, &[]), (fail, &[]))
        .expect("well-formed guard");

    // The success path may now do proven arithmetic on a value that arrived generic.
    let narrowed = func.block(ok).expect("ok exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, ok);
    let doubled = b
        .arith(NumOp::Add, narrowed, narrowed)
        .expect("narrowed to i32");
    let widened = b.widen(doubled);
    b.ret(&[widened]);

    let mut b = FuncBuilder::new(&mut func, &types, fail);
    b.ret(&[unknown]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
}

#[test]
fn a_guard_whose_success_block_ignores_the_narrowed_value_is_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();

    assert_eq!(
        b.guard(unknown, Repr::I32, (ok, &[]), (fail, &[])),
        Err(BuildError::GuardTargetMissingValue { target: ok }),
        "without the parameter, the narrowed value would escape the path that proved it"
    );
}

#[test]
fn a_field_access_takes_its_representation_from_the_layout() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::F64, Repr::F64]);

    let mut func = function(&[], &[Repr::F64]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let obj = b.alloc(point, Region::Local);
    let x = b.field_load(obj, point, 0).expect("field 0 exists");
    let y = b.field_load(obj, point, 1).expect("field 1 exists");
    let sum = b
        .arith(NumOp::Add, x, y)
        .expect("both are f64 by declaration, not by assertion");
    b.ret(&[sum]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
}

#[test]
fn a_field_that_does_not_exist_is_refused() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::F64, Repr::F64]);

    let mut func = function(&[], &[Repr::F64]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let obj = b.alloc(point, Region::Local);

    assert_eq!(
        b.field_load(obj, point, 2),
        Err(BuildError::NoSuchField {
            ty: point,
            field: 2
        })
    );
}

#[test]
fn an_allocation_is_a_reference_to_its_own_type() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[], &[Repr::Ref(RefKind::Aggregate(ty))]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let obj = b.alloc(ty, Region::Shared);
    b.ret(&[obj]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
}

#[test]
fn a_constant_materializes_with_its_declared_representation() {
    let types = TypeRegistry::new();
    let mut func = function(&[], &[Repr::F64]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let decl = b.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(1.5f64.to_bits()),
    });
    let value = b.use_const(decl);
    b.ret(&[value]);

    assert_eq!(func.repr_of(value), Repr::F64);
    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
}

#[test]
fn the_verifier_catches_a_block_that_never_ends() {
    let types = TypeRegistry::new();
    let mut func = function(&[], &[]);
    let orphan = func.push_block();

    let errors = verify(&func, &types, &FuncRegistry::new());
    assert!(errors.contains(&VerifyError::UnterminatedBlock(func.entry)));
    assert!(errors.contains(&VerifyError::UnterminatedBlock(orphan)));
}

#[test]
fn the_verifier_catches_a_return_that_contradicts_the_signature() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    func.set_terminator(entry, Terminator::Return(vec![x]));

    assert_eq!(
        verify(&func, &types, &FuncRegistry::new()),
        vec![VerifyError::ReturnRepr {
            from: entry,
            position: 0,
            expected: Repr::F64,
            found: Repr::I32,
        }]
    );
}

#[test]
fn the_verifier_reports_every_violation_rather_than_the_first() {
    let types = TypeRegistry::new();
    let mut func = function(&[], &[Repr::F64]);
    let orphan = func.push_block();
    func.set_terminator(func.entry, Terminator::Return(vec![]));

    let errors = verify(&func, &types, &FuncRegistry::new());
    assert!(
        errors.len() >= 2,
        "a program with two mistakes takes one pass to diagnose"
    );
    assert!(errors.contains(&VerifyError::UnterminatedBlock(orphan)));
}

#[test]
fn the_verifier_rejects_a_program_built_against_another_registry() {
    let mut theirs = TypeRegistry::new();
    let foreign = theirs.declare(&[Repr::I64]);
    let ours = TypeRegistry::new();

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &theirs, entry);
    b.alloc(foreign, Region::Local);
    b.ret(&[]);

    // Built consistently, it is fine; verified against a registry that never
    // issued the identifier, it is not — plausible offsets read from the wrong
    // layouts is exactly the failure worth catching here.
    assert_eq!(verify(&func, &theirs, &FuncRegistry::new()), vec![]);
    assert!(!is_valid(&func, &ours, &FuncRegistry::new()));
}

#[test]
fn a_one_operand_float_operation_refuses_an_integer() {
    // The domain is stated once, at the builder, where the mistake is written.
    // `sqrtsd` over an integer register is not a slower square root, it is a
    // different number.
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::F64]);
    let integer = param(&func, 0);
    let entry = func.entry;
    let mut builder = FuncBuilder::new(&mut func, &types, entry);
    let refused = builder.float_unary(rts_cranelift::ir::FloatOp::Sqrt, integer);
    assert!(matches!(
        refused,
        Err(BuildError::WrongDomain {
            operation: "float_unary",
            found: Repr::I32
        })
    ));
}

#[test]
fn reading_a_double_as_an_int32_and_back_stays_in_its_domain() {
    // The pair that makes bitwise work reachable: out of the float domain and
    // back into it, each step refusing the other's representation. Without the
    // return trip a bitwise result widens as a tagged INTEGER and every numeric
    // guard downstream stops recognising it.
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let double = param(&func, 0);
    let entry = func.entry;
    let mut builder = FuncBuilder::new(&mut func, &types, entry);
    let integer = builder.to_int32(double).expect("a double converts");
    assert_eq!(builder.repr_of(integer), Repr::I32);
    assert!(builder.to_int32(integer).is_err(), "already an integer");
    let back = builder.to_f64(integer).expect("an int32 converts back");
    assert_eq!(builder.repr_of(back), Repr::F64);
    assert!(builder.to_f64(back).is_err(), "already a double");
}
