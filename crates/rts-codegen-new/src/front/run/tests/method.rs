//! Method-dispatch bail cases: callbacks, dynamic receiver, unsupported arity —
//! the explicit `Unsupported` bails that keep dispatch from guessing.

use super::{assert_bails, run_source};

#[test]
fn unknown_method_name_bails() {
    // A method NOT in the Registry mirror on a string receiver bails explicitly
    // (P4 covers a fixed surface; an unknown name is never guessed).
    assert_bails(r#"console.log("a".notARealMethod());"#);
}

#[test]
fn method_on_dynamic_receiver_bails() {
    // A string method on a VARIABLE (kind Unknown — class not statically proven)
    // bails: dynamic receiver-kind dispatch is a later increment.
    let res = run_source(r#"let s = "hi"; console.log(s.toUpperCase());"#);
    assert!(res.is_err(), "dynamic-receiver method must bail, got {res:?}");
}

#[test]
fn one_arg_slice_bails() {
    // `slice(n)` (1-arg form) relies on a runtime default this table does not
    // inject — BAIL rather than guess the end index.
    assert_bails(r#"console.log("hello".slice(1));"#);
}
