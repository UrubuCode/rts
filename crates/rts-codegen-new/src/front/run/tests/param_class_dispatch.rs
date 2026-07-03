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

/// A `type` alias param no longer bails: the method dispatches DYNAMICALLY by
/// the receiver's runtime shape (the shape-keyed virtual dispatch), so the
/// aliased type name never has to resolve statically — and a wrong-slot read
/// cannot happen (the shape check keys the arm).
#[test]
fn alias_to_class_dispatches() {
    assert_stdout(
        "class RealClass { m(): number { return 42; } } \
         type Alias = RealClass; \
         function f(x: Alias): number { return x.m(); } \
         const r = new RealClass(); console.log(f(r));",
        "42\n",
    );
}

/// An `interface`-typed param dispatches the same way: the RUNTIME shape of the
/// argument (a class implementing the interface) keys the virtual dispatch.
#[test]
fn interface_param_dispatches() {
    assert_stdout(
        "interface Shape { area(): number; } \
         function f(s: Shape): number { return s.area(); } \
         class Circle implements Shape { area(): number { return 3; } } \
         console.log(f(new Circle()));",
        "3\n",
    );
}
