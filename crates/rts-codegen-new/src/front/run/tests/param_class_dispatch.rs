//! Method dispatch on a parameter typed with a USER CLASS (`function f(x: Box)
//! { x.m() }` and the arrow form `(x: Box) => x.m()`). The engine reads the
//! param's class annotation (`lower_param` / the arrow arm in `rts-hir`), records
//! `name → class` in `local_classes`, and dispatches `x.m()` statically — the same
//! fact a `const x = new Box()` local carries.
//!
//! SOUNDNESS GATE (the `alias_*` tests): the engine resolves ONLY a real `class`
//! name (never an `interface` or `type` alias — those aren't in `scope.classes`),
//! so a param typed with a non-class name BAILS honestly instead of dispatching on
//! a wrong shape. `type Alias = RealClass` is the one residual hole this fixture
//! pins: it must bail, NEVER emit a method call over an unproven shape.

use super::{assert_bails, assert_stdout};

/// Method on a class-typed function parameter dispatches statically.
#[test]
fn method_on_class_param() {
    assert_stdout(
        "class Box { v: number = 7; g(): number { return this.v; } } \
         function use(x: Box): number { return x.g(); } \
         const b = new Box(); console.log(use(b));",
        "7\n",
    );
}

/// Same, through an ARROW parameter (the callback form `run((x: Box) => ...)`).
#[test]
fn method_on_class_arrow_param() {
    assert_stdout(
        "class Box { v: number = 5; g(): number { return this.v; } } \
         function run(f: (x: Box) => void): void { const b = new Box(); f(b); } \
         run((x: Box) => { console.log(x.g()); });",
        "5\n",
    );
}

/// `arr.map((x: Box) => x.g())` — falls out of the same fix.
#[test]
fn method_on_class_param_in_map() {
    assert_stdout(
        "class Box { v: number = 3; g(): number { return this.v; } } \
         const arr = [new Box(), new Box()]; \
         const sums = arr.map((x: Box) => x.g()); \
         console.log(sums[0]);",
        "3\n",
    );
}

/// SOUNDNESS: a `type` alias to a real class must NOT dispatch on the aliased
/// shape — it bails honestly (the alias name is not a `class` in scope). A wrong
/// dispatch here would read a wrong slot / ACCESS_VIOLATION; bailing is correct.
#[test]
fn alias_to_class_bails_not_dispatches() {
    assert_bails(
        "class RealClass { m(): number { return 42; } } \
         type Alias = RealClass; \
         function f(x: Alias): number { return x.m(); } \
         const r = new RealClass(); f(r);",
    );
}

/// SOUNDNESS: an `interface` name that is NOT a class also bails (no class in
/// scope to prove the receiver's shape).
#[test]
fn interface_param_bails() {
    assert_bails(
        "interface Shape { area(): number; } \
         function f(s: Shape): number { return s.area(); } ",
    );
}
