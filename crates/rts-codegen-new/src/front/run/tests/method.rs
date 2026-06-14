//! Method-dispatch bail cases: callbacks, dynamic receiver, unsupported arity —
//! the explicit `Unsupported` bails that keep dispatch from guessing.

use super::assert_bails;
use super::assert_stdout;

#[test]
fn unknown_method_name_bails() {
    // A method NOT in the Registry mirror on a string receiver bails explicitly
    // (P4 covers a fixed surface; an unknown name is never guessed).
    assert_bails(r#"console.log("a".notARealMethod());"#);
}

#[test]
fn method_on_dynamic_receiver_now_dispatches() {
    // P5.9: a string method on a VARIABLE (kind Unknown — class not statically
    // proven) now dispatches on the receiver's PolyValue tag AT RUNTIME, instead
    // of bailing. `s.toUpperCase()` with `s = "hi"` → "HI".
    assert_stdout(r#"let s = "hi"; console.log(s.toUpperCase());"#, "HI\n");
}

#[test]
fn one_arg_slice_now_works() {
    // P5.2: `slice(n)` (1-arg form) now injects the "to end" default bound at the
    // lowering (previously a bail). `"hello".slice(1)` → `"ello"`.
    assert_stdout(r#"console.log("hello".slice(1));"#, "ello\n");
}
