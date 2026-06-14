//! Function-as-constructor tests (Phase 2/3): `new F()` on a FUNCTION value runs
//! F with a fresh `this`, returns F's object-return-or-the-instance; `x
//! instanceof F` (F a function) is a runtime check via the ctor side-table. This
//! is the JS dual-callable pattern (`if (this instanceof F) … else …`).
//!
//! NOTE 1 (the `this` param): a TS `this:` parameter is TYPE-ONLY — the engine
//! only synthesizes a real leading `this` slot (and marks the fn `has_this`, the
//! gate for the fn-ctor path) when the body actually REFERENCES `this`. A real
//! dual-callable always does (`this instanceof F`), so every test here references
//! `this`.
//!
//! NOTE 2 (`new`'s result, the JS spec): a constructor that `return`s a PRIMITIVE
//! has that primitive DISCARDED — `new F()` yields the fresh instance. Only an
//! OBJECT return overrides the instance. So a `this instanceof F` distinction is
//! observed through a SIDE EFFECT (console.log) or an OBJECT return, not by the
//! primitive value of `new F()`.

use super::assert_stdout;

#[test]
fn new_on_function_basic() {
    // new F() runs F with a fresh `this` (referenced so the slot is synthesized);
    // F returns the instance implicitly. `typeof (new F())` is "object".
    assert_stdout(
        r#"function F(this: any): string { return typeof this; }
           let x = new F();
           console.log(typeof x);"#,
        "object\n",
    );
}

#[test]
fn this_instanceof_distinguishes_new_vs_call() {
    // `this instanceof F` is true under `new`, false under a plain call. Observed
    // via a side-effecting console.log (the primitive return is discarded by `new`).
    assert_stdout(
        r#"const F = function(this: any): void {
             if (this instanceof F) { console.log("ctor"); }
             else { console.log("call"); }
           };
           new F();
           F();"#,
        "ctor\ncall\n",
    );
}

#[test]
fn new_returns_explicit_object() {
    // An OBJECT return from a `new`-called constructor OVERRIDES the instance.
    // Observed via `instanceof W`: when F (called with `new`) returns a fresh `W`,
    // `new F()` IS the W (override), so `new F() instanceof W` is true. The plain
    // call returns the primitive string.
    assert_stdout(
        r#"class W { tag(): string { return "W"; } }
           const F = function(this: any): any {
             if (this instanceof F) { return new W(); }
             return "primitive";
           };
           let a: any = new F();
           console.log(a instanceof W);
           console.log(F());"#,
        "true\nprimitive\n",
    );
}

#[test]
fn instanceof_f_on_outside_value() {
    // A value NOT constructed by F is not `instanceof F`. Both F and G reference
    // `this` so both gain the synthesized receiver slot.
    assert_stdout(
        r#"function F(this: any): string { return typeof this; }
           function G(this: any): string { return typeof this; }
           let a: any = new F();
           let b: any = new G();
           console.log(a instanceof F);
           console.log(b instanceof F);"#,
        "true\nfalse\n",
    );
}

#[test]
fn fn_decl_form_distinguishes_new_vs_call() {
    // Same distinction with a `function F(){}` DECLARATION form.
    assert_stdout(
        r#"function F(this: any): void {
             if (this instanceof F) { console.log("ctor"); }
             else { console.log("call"); }
           }
           new F();
           F();"#,
        "ctor\ncall\n",
    );
}
