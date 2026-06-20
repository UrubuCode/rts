//! The static CLASS of a value survives a CALL / METHOD-CALL RESULT, so a
//! chained call on it dispatches statically — not only on a bare `new C()` or a
//! `let`-bound local. Covers the general fluent/factory pattern (a function that
//! returns a class instance, then a method on that result), proven against the
//! reference runtime. The class never comes from a guess: it is the declared
//! `new C`, a `return this`, a `const`-bound instance, a function's proven return
//! class, or a chain of those.

use super::assert_stdout;

/// `f(): C { return new C().method(); }` then `f().other()` — the function's
/// return class is inferred from a `new C().chain()` (a MethodCall, not a bare
/// `new`), so the call result dispatches.
#[test]
fn method_on_function_result() {
    assert_stdout(
        "class Box { v: number = 0; add(n: number): this { this.v += n; return this; } \
         get(): number { return this.v; } } \
         function mk(): Box { return new Box().add(10); } \
         console.log(mk().get());",
        "10\n",
    );
}

/// `f(): C { const b = new C(); …; return b; }` — the return class flows through
/// a `const`-bound local instance.
#[test]
fn method_on_function_result_via_local() {
    assert_stdout(
        "class Box { v: number = 0; set(n: number): this { this.v = n; return this; } \
         get(): number { return this.v; } } \
         function mk(): Box { const b = new Box(); b.set(5); return b; } \
         console.log(mk().get());",
        "5\n",
    );
}

/// A deep fluent chain on a bare `new C()`: every `add` returns `this`, so the
/// final `.get()` dispatches on the chain result.
#[test]
fn deep_fluent_chain() {
    assert_stdout(
        "class Box { v: number = 0; add(n: number): this { this.v += n; return this; } \
         get(): number { return this.v; } } \
         console.log(new Box().add(1).add(2).add(3).get());",
        "6\n",
    );
}

/// A factory returning a factory result (`return otherFn()`): the fixpoint pass
/// resolves `mk2`'s class from `mk1`'s already-inferred return class.
#[test]
fn function_result_forwarded() {
    assert_stdout(
        "class Box { v: number = 0; add(n: number): this { this.v += n; return this; } \
         get(): number { return this.v; } } \
         function mk1(): Box { return new Box().add(7); } \
         function mk2(): Box { return mk1(); } \
         console.log(mk2().get());",
        "7\n",
    );
}
