//! End-to-end do builder: registra uma superfície e checa a resolução que o
//! codegen faria.

use rts_engine::{AbiType, Engine, MemberFlags, MemberKind, VarKind, sig};

// Implementações nativas de brinquedo (o que `rts-std` escreveria).
extern "C" fn toy_print(_ptr: *const u8, _len: i64) {}
extern "C" fn toy_max() -> i64 {
    4096
}
extern "C" fn toy_seed_get() -> i64 {
    7
}
extern "C" fn toy_seed_set(_v: i64) {}
extern "C" fn toy_index_of(_recv: u64, _x: i64) -> i64 {
    -1
}
extern "C" fn toy_index_of_from(_recv: u64, _x: i64, _from: i64) -> i64 {
    -1
}
extern "C" fn toy_is_nan(_x: f64) -> i64 {
    0
}

fn build() -> Engine {
    let mut e = Engine::new();

    // módulo: import { print, MAX, seed } from "rts:toy"
    e.ns("toy")
        .doc("toy module")
        .function("print", toy_print as *const u8, sig!(StrPtr => Void))
        .constant("MAX", toy_max as *const u8, AbiType::I64)
        .variable(
            "seed",
            VarKind::Mutable,
            toy_seed_get as *const u8,
            Some(toy_seed_set as *const u8),
            AbiType::I64,
        )
        .done();

    // classe: new Arr(); arr.indexOf(x) / arr.indexOf(x, from)  (overload por aridade)
    e.class("Arr")
        .method(
            "indexOf",
            toy_index_of as *const u8,
            sig!(Handle, I64 => I64),
        )
        .method(
            "indexOf",
            toy_index_of_from as *const u8,
            sig!(Handle, I64, I64 => I64),
        )
        .done();

    // global bare: isNaN(x)
    e.global()
        .function("isNaN", toy_is_nan as *const u8, sig!(F64 => Bool));

    e
}

#[test]
fn module_resolves_and_carries_fn_ptr() {
    let e = build();
    let r = e.registry();

    // bare "toy" resolve via scheme rts default
    let m = r.module("toy").expect("module toy");
    assert_eq!(m.scheme, "rts");
    let print = m.members.iter().find(|x| x.name == "print").unwrap();
    assert!(matches!(print.kind, MemberKind::Function));
    assert_eq!(print.symbol, "__RTS_FN_NS_TOY_PRINT");
    assert_eq!(print.fn_ptr.addr(), toy_print as *const u8 as usize);

    // o símbolo está no jit_symbols (substitui o add_fn!)
    assert_eq!(
        r.jit_symbol("__RTS_FN_NS_TOY_PRINT").unwrap().addr(),
        toy_print as *const u8 as usize
    );
}

#[test]
fn variable_emits_getter_and_setter_with_flags() {
    let e = build();
    let m = e.registry().module("rts:toy").unwrap();

    let get = m
        .members
        .iter()
        .find(|x| x.name == "seed" && matches!(x.kind, MemberKind::VarGetter))
        .unwrap();
    assert!(get.flags.contains(MemberFlags::MUTABLE));
    assert_eq!(get.symbol, "__RTS_FN_NS_TOY_SEED_GET");

    let set = m
        .members
        .iter()
        .find(|x| matches!(x.kind, MemberKind::VarSetter))
        .unwrap();
    assert_eq!(set.name, "seed");
    assert_eq!(set.symbol, "__RTS_FN_NS_TOY_SEED_SET");
    assert_eq!(set.sig.args, vec![AbiType::I64]);
}

#[test]
fn class_overload_resolves_by_arity() {
    let e = build();
    let arr = e.registry().class("Arr").unwrap();

    // 1 arg -> indexOf(x) ; 2 args -> indexOf(x, from)
    let one = arr.resolve_instance_method("indexOf", 1).unwrap();
    assert_eq!(one.sig.args.len(), 2); // receiver + x
    let two = arr.resolve_instance_method("indexOf", 2).unwrap();
    assert_eq!(two.sig.args.len(), 3); // receiver + x + from
    assert_eq!(two.fn_ptr.addr(), toy_index_of_from as *const u8 as usize);

    // aridade que não casa -> first-by-name
    assert!(arr.resolve_instance_method("indexOf", 9).is_some());
}

#[test]
fn global_scope_member() {
    let e = build();
    let g = e.registry().global("isNaN").unwrap();
    assert!(matches!(g.kind, MemberKind::Function));
    assert_eq!(g.sig.returns, AbiType::Bool);
}

#[test]
fn counts() {
    let e = build();
    assert_eq!(e.registry().module_count(), 1);
    assert_eq!(e.registry().class_count(), 1);
}
