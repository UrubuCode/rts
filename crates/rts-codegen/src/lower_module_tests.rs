//! What lowering a whole module answers, pinned apart from it.

use super::*;
use crate::parse::parse_module;
use crate::names::resolve::resolve_module;
use rts_mir::verify::verify;

/// A module, lowered, with the interner that read it.
fn module(source: &str) -> (Module, Names) {
    let mut names = Names::new();
    let program = parse_module(source, &mut names).expect("the fixture parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    (lowered, names)
}

/// The numbering is source order, and a call names a position in it.
#[test]
fn a_call_names_the_function_the_module_numbered() {
    let (lowered, _) = module(
        "function twice(n) { return n + n; }
         function start() { return twice(4); }",
    );
    assert_eq!(lowered.functions.len(), 2);
    assert_eq!(lowered.lowered(), 2);

    let start = lowered.functions[1]
        .result
        .as_ref()
        .expect("the caller lowers");
    assert_eq!(verify(start), Ok(()));
    let called = start
        .insts
        .iter()
        .find_map(|held| match &held.op {
            rts_mir::Op::Call { callee, .. } => Some(*callee),
            _ => None,
        })
        .expect("a call");
    // `twice` is function zero, which is what source order means.
    assert_eq!(called, rts_mir::cfg::Callee::Func(rts_mir::cfg::FuncId(0)));
}

/// A call may name a function written LATER, which is why the map is built before
/// anything is lowered.
#[test]
fn a_call_to_a_function_declared_later_still_resolves() {
    let (lowered, _) = module(
        "function first() { return second(); }
         function second() { return 1; }",
    );
    assert_eq!(lowered.lowered(), 2);
    let first = lowered.functions[0].result.as_ref().expect("lowers");
    let called = first
        .insts
        .iter()
        .find_map(|held| match &held.op {
            rts_mir::Op::Call { callee, .. } => Some(*callee),
            _ => None,
        })
        .expect("a call");
    assert_eq!(called, rts_mir::cfg::Callee::Func(rts_mir::cfg::FuncId(1)));
}

/// The reason the map is keyed by binding and not by spelling: a module may spell
/// two functions the same, and each is a different entry.
///
/// Asserted on the MAP rather than through a lowered call, and the difference is a
/// finding: the shape that shows this best — a `function step()` declared inside
/// two different function bodies — does not lower at all, because a nested
/// definition is its own graph and is refused. So a test written through the
/// lowering measured zero calls and proved nothing about the map.
#[test]
fn two_functions_of_one_spelling_are_two_entries_of_the_map() {
    let mut names = Names::new();
    let program = parse_module(
        "function outer() { function step() { return 1; } return step(); }
         function other() { function step() { return 2; } return step(); }",
        &mut names,
    )
    .expect("parses");
    let resolution = resolve_module(&program.body);
    let functions = every_function(&program.body);
    let callees = crate::lower::Callees::of(&program.body, &functions, &resolution);

    let step = names.intern("step");
    let entries: Vec<_> = (0..resolution.len())
        .map(|at| crate::names::resolve::BindingId::from_index(at))
        .filter(|held| resolution.binding(*held).name == step)
        .filter_map(|held| callees.of_binding(held))
        .collect();
    assert_eq!(entries.len(), 2, "two bindings, two entries");
    assert_ne!(
        entries[0], entries[1],
        "a spelling could not have told them apart"
    );
}

/// A `const f = () => …` is a binding holding a function expression, which the
/// numbering sees and the name-based map cannot.
#[test]
fn a_function_expression_bound_to_a_name_is_callable() {
    let (lowered, _) = module(
        "const double = (n) => n + n;
         function start() { return double(2); }",
    );
    let start = lowered
        .functions
        .iter()
        .find(|held| held.named == "start")
        .expect("the caller is in the module");
    let func = start.result.as_ref().expect("it lowers");
    assert!(
        func.insts
            .iter()
            .any(|held| matches!(&held.op, rts_mir::Op::Call { .. })),
        "the call should resolve through the declaration's binding"
    );
}

/// A method call lowers now, and a GLOBAL still does not — which is the pair worth
/// keeping, because the two were one refusal before the receiver existed.
///
/// The previous version of this test asserted that both were refused. The member
/// call now lowers, so the assertion was replaced rather than relaxed: what is still
/// missing is the global, and saying so is what the survey reads.
#[test]
fn a_method_call_lowers_and_a_global_call_still_names_its_global() {
    let (lowered, _) = module(
        "function viaMember(o) { return o.m(); }
         function viaGlobal() { return parseInt('2'); }",
    );
    let member = lowered
        .functions
        .iter()
        .find(|held| held.named == "viaMember")
        .expect("in the module");
    assert!(member.result.is_ok(), "{:?}", member.result);

    let global = lowered
        .functions
        .iter()
        .find(|held| held.named == "viaGlobal")
        .expect("in the module");
    assert!(matches!(
        global.result.as_ref().err(),
        Some(Unsupported::Global(_))
    ));
}

/// A call through a parameter is a DYNAMIC call, which is what the receiver field
/// made expressible: the callee is the parameter's value and no receiver travels.
#[test]
fn a_call_through_a_parameter_reaches_the_value_the_parameter_holds() {
    let (lowered, _) = module("function apply(f) { return f(1); }");
    let func = lowered.functions[0]
        .result
        .as_ref()
        .expect("a dynamic call lowers");
    let call = func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .expect("a call");
    match &call.op {
        rts_mir::Op::Call {
            callee, receiver, ..
        } => {
            assert_eq!(*callee, rts_mir::cfg::Callee::Dynamic(rts_mir::ValueId(0)));
            assert!(receiver.is_none());
        }
        other => panic!("expected a call, got {other:?}"),
    }

}

/// Every graph in a module shares one domain, which is what makes an index mean the
/// same thing in two of them.
#[test]
fn the_whole_module_shares_one_set_of_tables() {
    let (lowered, _) = module(
        "function a() { return 1 + 2; }
         function b() { return 3 - 4; }",
    );
    let add = lowered.domain.prim(crate::domain::JsPrim::Add);
    let first = lowered.functions[0].result.as_ref().expect("lowers");
    assert!(
        first
            .insts
            .iter()
            .any(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if *prim == add))
    );
    // And the second graph's subtract is the module's index for it, not a second
    // table's.
    let subtract = lowered.domain.prim(crate::domain::JsPrim::Subtract);
    let second = lowered.functions[1].result.as_ref().expect("lowers");
    assert!(
        second
            .insts
            .iter()
            .any(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if *prim == subtract))
    );
}

/// A method is numbered like any other function, because a class body is code.
#[test]
fn a_method_is_numbered_and_lowered_like_any_other_function() {
    let (lowered, _) = module("class C { plus(n) { return n + 1; } }");
    assert_eq!(lowered.functions.len(), 1);
    assert!(lowered.functions[0].result.is_ok());
}
