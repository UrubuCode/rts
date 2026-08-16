//! Asking whether a generic value is a particular singleton.
//!
//! The claim is that this needs no call and reads nothing: a singleton has one
//! encoding, so the question is a comparison against a constant word. The last
//! test compiles it and runs it, because a verifier accepting something is not
//! evidence that it computes the right answer.

use rts_cranelift::ir::{
    BuildError, FuncBuilder, FuncRegistry, Function, Inst, Signature, ValueId,
};
use rts_cranelift::repr::Repr;
use rts_cranelift::tags::{TAG_BOOL, TagRegistry, encode};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::verify::{VerifyError, verify};

fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

fn two_singletons() -> (rts_cranelift::tags::SingletonId, rts_cranelift::tags::SingletonId) {
    let mut tags = TagRegistry::new();
    let declared = tags.declare_singletons(2).expect("two fit");
    (declared[0], declared[1])
}

#[test]
fn asking_whether_a_generic_value_is_a_singleton_emits_no_call() {
    let (first, _) = two_singletons();
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let answer = b.is_singleton(value, first).expect("a generic operand");
    let widened = b.widen(answer);
    b.ret(&[widened]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
    let calls = func
        .blocks()
        .flat_map(|(_, block)| block.insts.iter())
        .filter_map(|id| func.inst(*id))
        .filter(|data| matches!(data.inst, Inst::Call { .. }))
        .count();
    assert_eq!(
        calls, 0,
        "the encoding answers this; reaching a runtime for it is the cost this \
         instruction exists to remove"
    );
}

#[test]
fn asking_whether_a_proven_value_is_a_singleton_is_refused() {
    let (first, _) = two_singletons();
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::Tagged]);
    let proven = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    assert_eq!(
        b.is_singleton(proven, first),
        Err(BuildError::WrongDomain {
            operation: "is_singleton",
            found: Repr::F64,
        }),
        "nothing proven is a singleton, so the answer would be a constant false \
         — and a client asking has lost track of what it holds"
    );
}

#[test]
fn the_verifier_refuses_a_proven_operand_the_builder_never_made() {
    let (first, _) = two_singletons();
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::Tagged]);
    let proven = param(&func, 0);

    // Built by hand, past the builder, which is the case rule 7 asks the
    // verifier to catch: the refusal has to survive a representation that did
    // not come through the constructor.
    let entry = func.entry;
    let results = func.push_inst(
        entry,
        Inst::IsSingleton {
            value: proven,
            singleton: first,
        },
        &[Repr::Bool],
    );
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(results[0]);
    b.ret(&[widened]);

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::WrongDomain { .. })),
        "the verifier is the half that catches what did not come from the builder"
    );
}

#[test]
fn two_singletons_are_told_apart_by_the_compiled_code() {
    use cranelift_module::Linkage;
    use cranelift_module::Module;
    use rts_cranelift::target::{MachineModule, executable_memory};

    let (first, second) = two_singletons();
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let answer = b.is_singleton(value, first).expect("a generic operand");
    let widened = b.widen(answer);
    b.ret(&[widened]);

    let mut jit = executable_memory().expect("this machine can host its own code");
    let machine_id = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, "is_first", Linkage::Export, &funcs)
            .expect("declaring succeeds");
        module
            .define(id, &func, &funcs, &types)
            .expect("defining succeeds");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalizing succeeds");
    let address = jit.get_finalized_function(machine_id);
    let call: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(address) };
    std::mem::forget(jit);

    let yes = encode(TAG_BOOL, 1);
    let no = encode(TAG_BOOL, 0);

    assert_eq!(call(first.word()), yes, "the singleton it was asked about");
    assert_eq!(
        call(second.word()),
        no,
        "a DIFFERENT singleton: same tag, different payload, and telling those \
         apart is the whole job"
    );
    assert_eq!(
        call(1.5f64.to_bits()),
        no,
        "an ordinary double is not in the encoded quadrant at all"
    );
    assert_eq!(
        call(f64::NAN.to_bits()),
        no,
        "a NaN is the value that would collide if the encoding did not reserve \
         its own quadrant, so it is the one worth asking about"
    );
}
