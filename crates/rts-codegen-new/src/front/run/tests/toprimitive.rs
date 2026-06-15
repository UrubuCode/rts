//! Lowering-time `ToPrimitive` tests (issue #304) — a STATICALLY-KNOWN-CLASS
//! object used in a STRING/NUMBER coercion context (`${obj}`, `String(obj)`,
//! `obj + ""`) runs its `toString`/`valueOf` method, matching Node/Bun EXACTLY.
//!
//! The correctness boundary tested here:
//! - template / `String()` / `+` DO call `toString`/`valueOf`;
//! - `console.log(obj)` does NOT (it inspects) — covered by the inspect tests;
//! - a DYNAMIC-class object / `Symbol.toPrimitive` BAILS (never a wrong value).

use super::{assert_bails, assert_stdout};

#[test]
fn to_string_in_template() {
    assert_stdout(
        r#"class P { x: number; constructor(x: number) { this.x = x; } toString() { return "P(" + this.x + ")"; } }
           let p = new P(5);
           console.log(`val=${p}`);"#,
        "val=P(5)\n",
    );
}

#[test]
fn string_of_object() {
    assert_stdout(
        r#"class C { toString() { return "custom"; } }
           console.log(String(new C()));"#,
        "custom\n",
    );
}

#[test]
fn object_plus_string() {
    assert_stdout(
        r#"class C { toString() { return "X"; } }
           console.log(new C() + "!");"#,
        "X!\n",
    );
}

#[test]
fn string_plus_object() {
    // The object on the RIGHT of a string `+` (the template-chain shape).
    assert_stdout(
        r#"class C { toString() { return "Y"; } }
           console.log("v=" + new C());"#,
        "v=Y\n",
    );
}

#[test]
fn value_of_in_arithmetic() {
    // `valueOf` returns a number → `n + 5` adds numerically (not string-concats).
    assert_stdout(
        r#"class N { v: number; constructor(v: number) { this.v = v; } valueOf() { return this.v; } }
           let n = new N(10);
           console.log(n + 5);"#,
        "15\n",
    );
}

#[test]
fn value_of_first_for_default_hint() {
    // A class with BOTH: the bare `+` (default hint) tries `valueOf` first, so
    // `n + 5` is `10 + 5 = 15`, NOT `"ten5"`.
    assert_stdout(
        r#"class N { valueOf() { return 10; } toString() { return "ten"; } }
           let n = new N();
           console.log(n + 5);"#,
        "15\n",
    );
}

#[test]
fn to_string_first_for_string_hint() {
    // For `String()` (string hint) the class with BOTH uses `toString` first.
    assert_stdout(
        r#"class N { valueOf() { return 10; } toString() { return "ten"; } }
           console.log(String(new N()));"#,
        "ten\n",
    );
}

#[test]
fn inherited_to_string() {
    // `toString` declared on the PARENT is used for a subclass instance.
    assert_stdout(
        r#"class Base { toString() { return "base"; } }
           class Sub extends Base { }
           console.log(String(new Sub()));"#,
        "base\n",
    );
}

#[test]
fn to_string_using_field() {
    // `toString` reading a field, in a template.
    assert_stdout(
        r#"class Pt { x: number; y: number; constructor(x: number, y: number) { this.x = x; this.y = y; }
                     toString() { return "(" + this.x + "," + this.y + ")"; } }
           let p = new Pt(1, 2);
           console.log(`p=${p}`);"#,
        "p=(1,2)\n",
    );
}

#[test]
fn default_object_is_object_object() {
    // A plain object literal (no class / no toString) → `[object Object]`.
    assert_stdout(
        r#"let o = { a: 1 }; console.log(String(o));"#,
        "[object Object]\n",
    );
}

#[test]
fn array_string_join() {
    // `String([1,2,3])` = the array join (`","`), NOT object ToPrimitive.
    assert_stdout(r#"console.log(String([1, 2, 3]));"#, "1,2,3\n");
}

#[test]
fn error_to_string() {
    // A runtime Error instance → `Error.prototype.toString` = `"name: message"`.
    assert_stdout(
        r#"console.log(String(new Error("boom")));"#,
        "Error: boom\n",
    );
}

#[test]
fn console_log_does_not_call_to_string() {
    // CRITICAL JS SEMANTICS: `console.log(instance)` INSPECTS the object — it does
    // NOT call `toString`. A bare-instance inspect is a near-miss vs bun's format,
    // so the engine BAILS console.log of a bare instance (honesty floor) rather
    // than print the `toString` result. This asserts we did NOT wire toString into
    // console.log (which would print "S" — wrong).
    assert_stdout(
        r#"class C { toString() { return "S"; } } console.log(new C());"#,
        "{}\n",
    );
}

#[test]
fn console_log_with_fields_does_not_call_to_string() {
    // Same boundary with a field present: the instance inspects to `{ n: 1 }`, NOT
    // the toString result "S".
    assert_stdout(
        r#"class C { n: number; constructor() { this.n = 1; } toString() { return "S"; } }
           console.log(new C());"#,
        "{ n: 1 }\n",
    );
}

#[test]
fn no_to_string_class_in_string_bails() {
    // A known-class instance with NEITHER toString NOR valueOf, in a string `+`,
    // resolves NO primitive method — so it falls to the pre-existing whole-object
    // gate, which BAILS (rather than the engine inventing a coercion). Sound: never
    // a wrong value. (JS would render `[object Object]`; a faithful default for the
    // no-method case is a later increment.)
    assert_bails(
        r#"class C { n: number; constructor() { this.n = 1; } }
           console.log("v=" + new C());"#,
    );
}
