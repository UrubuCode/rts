//! A USER class used as a first-class VALUE: a bare class name in value position
//! (`const C = Box`, `typeof Box`, `globalThis.X = Box`) reifies to a callable
//! class-value (a `TAG_FUNCTION` new-thunk), and `new C(args)` on the const-bound
//! reference constructs the class. Proven against the reference runtime.
//!
//! SCOPE: `const C = Box; new C(..)` resolves the class statically through the
//! reference. `new <runtime-value>()` (a class read back from `globalThis` at
//! runtime) CONSTRUCTS the instance soundly (slice 2) — but the result has no
//! static class, so calling a METHOD on it (`new G().get()`) needs static
//! `globalThis`-key→class tracking or shape-keyed dispatch (a deferred follow-up;
//! field access on the result works). Class EXPRESSIONS (`class {…}`) are also
//! deferred (HIR does not model them).

use super::assert_stdout;

/// `const C = Box; new C(7)` — the class name reifies to a value, the const binds
/// the reference, and `new C(..)` constructs `Box` on the static path.
#[test]
fn const_ref_dynamic_new() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } \
         get(): number { return this.v; } } \
         const C = Box; console.log(new C(7).get());",
        "7\n",
    );
}

/// Multiple constructions through the same class-reference local.
#[test]
fn const_ref_multiple_new() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } \
         get(): number { return this.v; } } \
         const C = Box; const x = new C(3); const y = new C(9); \
         console.log(x.get() + y.get());",
        "12\n",
    );
}

/// `typeof Box` is "function" (a class value is callable, like JS).
#[test]
fn typeof_class_is_function() {
    assert_stdout(
        "class Box { v: number = 0; } console.log(typeof Box);",
        "function\n",
    );
}

/// A class VALUE survives a round-trip through `globalThis`: stored as a property
/// and read back, it is still a callable function value.
#[test]
fn class_value_through_globalthis() {
    assert_stdout(
        "class Widget { } globalThis.Widget = Widget; \
         console.log(typeof globalThis.Widget);",
        "function\n",
    );
}

/// (slice 2) `new <runtime-value>()`: a class read back from `globalThis` at
/// runtime CONSTRUCTS a real instance — field access on the result works (the
/// instance carries the class's fields). The result has no static class, so a
/// METHOD call on it is a deferred follow-up.
#[test]
fn new_on_runtime_class_value_constructs() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } } \
         globalThis.Box = Box; const G = globalThis.Box; \
         const b = new G(5); console.log(b.v);",
        "5\n",
    );
}

/// Two constructions through a runtime class-value read from `globalThis`.
#[test]
fn new_on_runtime_class_value_multiple() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } } \
         globalThis.Box = Box; const G = globalThis.Box; \
         const x = new G(2); const y = new G(8); console.log(x.v + y.v);",
        "10\n",
    );
}

/// SOUNDNESS: `new <value>()` where the value is NOT a constructor (a number held
/// in a local) throws a TypeError at runtime — it never mis-constructs. The throw
/// is catchable, exactly like the reference runtime.
#[test]
fn new_on_non_constructor_throws() {
    assert_stdout(
        "class Box { } globalThis.x = 5; const G = globalThis.x; \
         try { const b = new G(); console.log(\"no-throw\"); } \
         catch (e) { console.log(\"caught\"); }",
        "caught\n",
    );
}

/// (caminho A) `globalThis.X = Box` (the only write, a known class) is tracked
/// statically, so `const G = globalThis.X; new G(5)` constructs on the STATIC path
/// and a METHOD on the result dispatches — the end-to-end `globalThis`-class
/// pattern.
#[test]
fn globalthis_tracked_class_dispatches_method() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } \
         get(): number { return this.v; } } \
         globalThis.Box = Box; const G = globalThis.Box; \
         console.log(new G(5).get());",
        "5\n",
    );
}

/// Two constructions + method dispatch through a statically-tracked globalThis key.
#[test]
fn globalthis_tracked_class_multiple() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } \
         get(): number { return this.v; } } \
         globalThis.Box = Box; const G = globalThis.Box; \
         const a = new G(2); const b = new G(8); console.log(a.get() + b.get());",
        "10\n",
    );
}

/// SOUNDNESS: a key with a DISAGREEING write (`globalThis.X = Box` then
/// `globalThis.X = 5`) is POISONED — it is NOT tracked as `Box`, so `new G()`
/// falls to the dynamic path where the runtime value (5) is not a constructor and
/// THROWS, never mis-dispatching as `Box`. (If the poison rule failed, `new G(7)`
/// would wrongly construct a `Box` and print "constructed".)
#[test]
fn globalthis_poisoned_key_not_dispatched_as_class() {
    assert_stdout(
        "class Box { v: number = 0; constructor(n: number) { this.v = n; } } \
         globalThis.X = Box; globalThis.X = 5; const G = globalThis.X; \
         try { const b = new G(7); console.log(\"constructed\"); } \
         catch (e) { console.log(\"caught\"); }",
        "caught\n",
    );
}
