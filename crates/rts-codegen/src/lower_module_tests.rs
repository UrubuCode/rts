//! What lowering a whole module answers, pinned apart from it.

use super::*;
use crate::names::resolve::resolve_module;
use crate::parse::parse_module;
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

/// A method call and a call to a GLOBAL both lower now, and this test has followed
/// the work twice: it first asserted both were refused, then that the member call
/// lowered and the global did not. Both do.
///
/// What it pins is that they lower to DIFFERENT shapes — the member call carries a
/// receiver, the global call reads the global object and carries none.
#[test]
fn a_method_call_carries_a_receiver_and_a_global_call_does_not() {
    let (lowered, _) = module(
        "function viaMember(o) { return o.m(); }
         function viaGlobal() { return parseInt('2'); }",
    );
    let receiver_of = |named: &str| {
        let entry = lowered
            .functions
            .iter()
            .find(|held| held.named == named)
            .expect("in the module");
        let func = entry.result.as_ref().expect("it lowers");
        func.insts
            .iter()
            .find_map(|held| match &held.op {
                rts_mir::Op::Call { receiver, .. } => Some(*receiver),
                _ => None,
            })
            .expect("a call")
    };
    assert!(receiver_of("viaMember").is_some());
    assert!(receiver_of("viaGlobal").is_none());
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

/// A class is three things: a constructor, an object to hold the methods, and the link
/// between them. Every one was already expressible.
#[test]
fn a_class_is_a_constructor_a_prototype_and_a_link() {
    let mut names = Names::new();
    let program = parse_module(
        "function make() {
           class P { constructor(x) { this.x = x; } twice() { return this.x * 2; } }
           return P;
         }",
        &mut names,
    )
    .expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    let make = lowered
        .functions
        .iter()
        .find(|held| held.named == "make")
        .expect("the outer function");
    let func = make.result.as_ref().expect("it lowers");
    assert_eq!(verify(func), Ok(()));

    let ops: Vec<_> = func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    // A closure for the constructor, an object for the prototype, a closure for the
    // method, and two writes: the method into the prototype and the prototype into the
    // constructor.
    assert_eq!(
        ops,
        vec![
            crate::domain::JsPrim::MakeClosure,
            crate::domain::JsPrim::NewObject,
            crate::domain::JsPrim::MakeClosure,
            crate::domain::JsPrim::FieldWrite,
            crate::domain::JsPrim::FieldWrite,
        ]
    );
}

/// The prototype link is written under a key the LANGUAGE fixes, not one from the
/// program's text — asking the interner for it would need a mutable interner in the
/// lowering for a string the program never wrote.
#[test]
fn the_prototype_link_uses_a_well_known_key() {
    let mut names = Names::new();
    let program = parse_module(
        "function make() { class P { constructor() {} } return P; }",
        &mut names,
    )
    .expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    let func = lowered.functions[0].result.as_ref().expect("lowers");
    let well_known = func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Const(rts_mir::Const::Declared(index)) => {
                lowered.domain.declared(*index).cloned()
            }
            _ => None,
        })
        .any(|held| {
            matches!(
                held,
                crate::domain::JsConst::WellKnown(crate::domain::WellKnown::Prototype)
            )
        });
    assert!(well_known, "the link is a well-known key");
}

/// `extends` brings three decisions with it — `super()` before `this` exists, a home
/// object for `super.m()`, and a second prototype link — so a class that inherited
/// without them would compile and get `super` wrong.
#[test]
fn a_class_with_extends_is_refused_with_its_three_reasons() {
    let mut names = Names::new();
    let program = parse_module(
        "function make(B) { class P extends B { constructor() {} } return P; }",
        &mut names,
    )
    .expect("parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, Tier::Generic);
    assert!(matches!(
        lowered
            .functions
            .iter()
            .find(|held| held.named == "make")
            .and_then(|held| held.result.as_ref().err()),
        Some(Unsupported::Expression(_))
    ));
}

/// `arguments` is declared by no scope and is not a global. Read through the global
/// object it answered `undefined` -- or whatever a program had put there -- where the
/// language answers the activation's argument list, so it is refused by name, in an
/// arrow as well: an arrow sees its enclosing function's.
#[test]
fn the_arguments_object_is_refused_rather_than_read_as_a_global() {
    let (lowered, _) = module(
        "function f() { return arguments[0]; }
         function g() { return () => arguments; }",
    );
    let refused = Unsupported::Expression("the arguments object, which this stage does not build");
    assert_eq!(lowered.functions[0].result.as_ref().err(), Some(&refused));
    assert_eq!(lowered.functions[2].result.as_ref().err(), Some(&refused));
}

/// An arrow's `this` is the enclosing function's. Read as the receiver the arrow was
/// called with, `obj.m = function () { return () => this; }` would answer `undefined`
/// from the arrow where the language answers `obj` -- so it is refused, and a method's
/// own `this` still lowers.
#[test]
fn this_in_an_arrow_is_refused_and_a_methods_own_this_is_not() {
    let (lowered, _) = module("function m() { const own = this; return () => this; }");
    assert_eq!(
        lowered.functions[1].result.as_ref().err(),
        Some(&Unsupported::Expression(
            "`this` in an arrow is the enclosing function's, which this stage does not carry"
        ))
    );
    let (plain, _) = module("function m() { return this; }");
    assert!(plain.functions[0].result.is_ok());
}

/// A template substitution is converted with the STRING hint before it is joined -- a
/// `StringOf` call per substitution, then `+` over two strings. `"" + x` would ask
/// `valueOf` first, so `` `${{ toString: () => "T", valueOf: () => 42 }}` `` would read
/// `"42"` where the language reads `"T"`: the order is what this pins.
#[test]
fn a_template_converts_each_substitution_to_a_string_before_joining() {
    let (lowered, _) = module("function f(a, b) { return `x${a}y${b}`; }");
    let func = lowered.functions[0]
        .result
        .as_ref()
        .expect("a template lowers");
    assert_eq!(verify(func), Ok(()));
    let conversions = func
        .insts
        .iter()
        .filter(|held| {
            matches!(&held.op, rts_mir::Op::Call { callee: rts_mir::cfg::Callee::Entry(entry), .. }
                if lowered.domain.entry_meaning(*entry) == Some(crate::runtime::RuntimeOp::StringOf))
        })
        .count();
    assert_eq!(conversions, 2, "one ToString per substitution");
    let joins = func
        .insts
        .iter()
        .filter(|held| {
            matches!(&held.op, rts_mir::Op::Prim { prim, .. }
            if lowered.domain.meaning(*prim) == Some(crate::domain::JsPrim::Add))
        })
        .count();
    // `x` + a, + `y`, + b -- and the empty piece after `b` joins nothing.
    assert_eq!(
        joins, 3,
        "each substitution and each non-empty text after the first"
    );
}
