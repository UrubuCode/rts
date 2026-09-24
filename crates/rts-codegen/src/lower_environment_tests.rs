//! Where a captured binding lives, pinned as the language's semantics: a closure and
//! the function that declared what it reads must reach ONE place for it.

use crate::domain::{JsConst, JsPrim};
use crate::lower::Unsupported;
use crate::lower_module::{Module, lower_module};
use crate::names::Names;
use crate::names::resolve::resolve_module;
use crate::parse::parse_module;
use rts_mir::guard::Tier;
use rts_mir::verify::verify;

fn module(source: &str, tier: Tier) -> (Module, Names) {
    let mut names = Names::new();
    let program = parse_module(source, &mut names).expect("the fixture parses");
    let resolution = resolve_module(&program.body);
    let lowered = lower_module(&program.body, &resolution, &names, tier);
    (lowered, names)
}

/// The operations of one function, in order, with what each primitive is.
fn prims(lowered: &Module, at: usize) -> Vec<(rts_mir::ValueId, JsPrim, Vec<rts_mir::ValueId>)> {
    let func = lowered.functions[at].result.as_ref().expect("it lowers");
    assert_eq!(verify(func), Ok(()));
    func.insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, args } => lowered
                .domain
                .meaning(*prim)
                .map(|which| (held.result, which, args.clone())),
            _ => None,
        })
        .collect()
}

fn count(of: &[(rts_mir::ValueId, JsPrim, Vec<rts_mir::ValueId>)], which: JsPrim) -> usize {
    of.iter().filter(|(_, held, _)| *held == which).count()
}

/// `n = 2` after the closure is made has to be what the closure sees, which is only
/// true if the declarer writes the environment rather than a register of its own. It
/// kept `n` in SSA before this, while the closure asked for it somewhere else.
#[test]
fn the_declarer_of_a_captured_local_writes_and_reads_it_through_its_environment() {
    let (lowered, mut names) = module(
        "function f() { let n = 1; const g = () => n; n = 2; return n + g(); }",
        Tier::Generic,
    );
    let ops = prims(&lowered, 0);
    let built: Vec<_> = ops
        .iter()
        .filter(|(_, held, _)| *held == JsPrim::EnvNew)
        .collect();
    assert_eq!(built.len(), 1, "one environment per activation");
    let (environment, _, operands) = built[0];
    // The link first, then one key -- `n` alone, because `g` is not captured.
    assert_eq!(operands.len(), 2);
    let func = lowered.functions[0].result.as_ref().expect("lowers");
    let key = func
        .insts
        .iter()
        .find(|inst| inst.result == operands[1])
        .expect("a key");
    match &key.op {
        rts_mir::Op::Const(rts_mir::Const::Declared(index)) => assert_eq!(
            lowered.domain.declared(*index),
            Some(&JsConst::Key(names.intern("n")))
        ),
        other => panic!("expected a declared key, got {other:?}"),
    }
    assert_eq!(
        count(&ops, JsPrim::EnvWrite),
        2,
        "the declaration and the assignment"
    );
    assert_eq!(count(&ops, JsPrim::EnvRead), 1, "the read in the return");
    // THE CLOSURE CLOSES OVER THAT ENVIRONMENT, not over whatever came first.
    let made = ops
        .iter()
        .find(|(_, held, _)| *held == JsPrim::MakeClosure)
        .expect("a closure");
    assert_eq!(made.2[1], *environment);
}

/// A closure made where nothing reaches past the maker is handed `undefined`, which
/// is what the running engine hands one too -- and nothing builds or reads an
/// environment for it.
#[test]
fn a_closure_over_nothing_is_handed_undefined_and_nothing_is_built() {
    let (lowered, _) = module(
        "function f() { const g = () => 1; return g; }",
        Tier::Generic,
    );
    let ops = prims(&lowered, 0);
    assert_eq!(count(&ops, JsPrim::EnvNew), 0);
    assert_eq!(count(&ops, JsPrim::EnclosingEnvironment), 0);
    let made = ops
        .iter()
        .find(|(_, held, _)| *held == JsPrim::MakeClosure)
        .expect("a closure");
    let func = lowered.functions[0].result.as_ref().expect("lowers");
    let handed = func
        .insts
        .iter()
        .find(|inst| inst.result == made.2[1])
        .expect("defined");
    match &handed.op {
        rts_mir::Op::Const(rts_mir::Const::Declared(index)) => assert_eq!(
            lowered.domain.declared(*index),
            Some(&JsConst::Singleton(crate::values::Singleton::Undefined))
        ),
        other => panic!("expected undefined, got {other:?}"),
    }
}

/// One link per activation that BUILT an environment between the use and the owner,
/// and none for one that did not: `b` owns nothing, so it hands `a`'s environment on
/// unchanged, and counting it would read one link past `a`.
#[test]
fn a_read_walks_one_link_per_environment_built_in_between() {
    let (lowered, _) = module(
        "function a() {
           let far = 1;
           function b() {
             function c() { let near = 2; return () => far + near; }
             return c;
           }
           return b;
         }",
        Tier::Generic,
    );
    // Source order: a, b, c, the arrow.
    let arrow = prims(&lowered, 3);
    assert_eq!(count(&arrow, JsPrim::EnvRead), 2);
    assert_eq!(count(&arrow, JsPrim::EnvOuter), 1, "past `c`, not past `b`");
    let middle = prims(&lowered, 1);
    assert_eq!(
        count(&middle, JsPrim::EnvNew),
        0,
        "`b` owns nothing captured"
    );
    assert_eq!(
        count(&middle, JsPrim::EnclosingEnvironment),
        1,
        "but it hands the one it was made in to `c`"
    );
}

/// A `let` in a loop head is a new binding per pass, and one slot per activation
/// would make every closure see the last pass. Refused on both sides, by name.
#[test]
fn a_captured_binding_of_a_loop_pass_is_refused_rather_than_shared() {
    let (lowered, _) = module(
        "function f() { const fs = []; for (let i = 0; i < 2; i++) { fs.push(() => i); } return fs; }",
        Tier::Generic,
    );
    let per_pass = Unsupported::Shape(
        "a captured binding declared inside a loop is one per pass, and this environment holds one per activation",
    );
    assert_eq!(lowered.functions[0].result.as_ref().err(), Some(&per_pass));
    assert_eq!(lowered.functions[1].result.as_ref().err(), Some(&per_pass));
}

/// A captured parameter is held in a register while the claim's guard runs and moved
/// afterwards. Building the environment first would put an allocation ahead of the
/// guard, and a guard behind an allocation is not one the other tier can be entered
/// from.
#[test]
fn a_captured_parameter_moves_into_the_environment_after_its_guard() {
    let (lowered, _) = module(
        "function f(x: number) { return () => x; }",
        Tier::Specialised,
    );
    let func = lowered.functions[0].result.as_ref().expect("lowers");
    let guard = func
        .insts
        .iter()
        .position(|held| matches!(held.op, rts_mir::Op::Guard { .. }))
        .expect("the claim is guarded");
    let built = func
        .insts
        .iter()
        .position(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::EnvNew)))
        .expect("an environment");
    assert!(guard < built);
    assert_eq!(count(&prims(&lowered, 0), JsPrim::EnvWrite), 1);
}
