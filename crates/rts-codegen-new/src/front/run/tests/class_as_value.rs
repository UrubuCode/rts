//! A USER class used as a first-class VALUE: a bare class name in value position
//! (`const C = Box`, `typeof Box`, `globalThis.X = Box`) reifies to a callable
//! class-value (a `TAG_FUNCTION` new-thunk), and `new C(args)` on the const-bound
//! reference constructs the class. Proven against the reference runtime.
//!
//! SCOPE: the dynamic `new` resolves the class statically through the `const C =
//! Box` reference. `new <runtime-value>()` (a class read back from globalThis at
//! runtime) and class EXPRESSIONS (`class {…}`) are a deferred follow-up.

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
