//! DYNAMIC (runtime) method-dispatch tests — P5.9.
//!
//! `recv.method(args)` where the receiver's CLASS is NOT statically proven (a
//! param, a call return, a re-`let` local — a Tagged value of `Unknown` kind) is
//! lowered to a runtime tag dispatch (`__rtsadp_dyn_*`). These assert EXACT stdout
//! against what Node/Bun print, plus the soundness bails (no-impl receivers).

use super::{assert_bails, assert_stdout};

// ---------------------------------------------------------------------------
// String methods on a Tagged receiver (param / let-local).
// ---------------------------------------------------------------------------

#[test]
fn string_method_on_param() {
    // `s` is a Tagged param (class not statically proven) — dispatches at runtime.
    assert_stdout(
        r#"function up(s){ return s.toUpperCase(); } console.log(up("hi"));"#,
        "HI\n",
    );
}

#[test]
fn string_to_lower_and_trim_on_param() {
    assert_stdout(
        r#"function f(s){ return s.toLowerCase(); } console.log(f("AbC"));"#,
        "abc\n",
    );
    assert_stdout(
        r#"function f(s){ return s.trim(); } console.log(f("  hi  "));"#,
        "hi\n",
    );
}

#[test]
fn string_index_of_on_tagged() {
    assert_stdout(
        r#"function f(s){ return s.indexOf("b"); } console.log(f("abc"));"#,
        "1\n",
    );
}

#[test]
fn string_includes_starts_ends_on_param() {
    assert_stdout(
        r#"function f(s){ return s.includes("b"); } console.log(f("abc"));"#,
        "true\n",
    );
    assert_stdout(
        r#"function f(s){ return s.startsWith("ab"); } console.log(f("abc"));"#,
        "true\n",
    );
    assert_stdout(
        r#"function f(s){ return s.endsWith("z"); } console.log(f("abc"));"#,
        "false\n",
    );
}

#[test]
fn string_char_at_and_char_code_at_on_param() {
    assert_stdout(
        r#"function f(s){ return s.charAt(1); } console.log(f("abc"));"#,
        "b\n",
    );
    assert_stdout(
        r#"function f(s){ return s.charCodeAt(0); } console.log(f("A"));"#,
        "65\n",
    );
}

#[test]
fn string_repeat_on_param() {
    assert_stdout(
        r#"function f(s){ return s.repeat(3); } console.log(f("ab"));"#,
        "ababab\n",
    );
}

// ---------------------------------------------------------------------------
// Array methods on a Tagged receiver.
// ---------------------------------------------------------------------------

#[test]
fn array_length_on_param() {
    // `.length` is a PROPERTY, dispatched dynamically too (member, not call).
    assert_stdout(
        r#"function len(a){ return a.length; } console.log(len([1, 2, 3]));"#,
        "3\n",
    );
}

#[test]
fn string_length_on_param() {
    assert_stdout(
        r#"function len(s){ return s.length; } console.log(len("hello"));"#,
        "5\n",
    );
}

#[test]
fn array_join_on_param() {
    assert_stdout(
        r#"function joinAny(a){ return a.join("-"); } console.log(joinAny([1, 2, 3]));"#,
        "1-2-3\n",
    );
}

#[test]
fn array_index_of_includes_on_param() {
    assert_stdout(
        r#"function f(a){ return a.indexOf(2); } console.log(f([1, 2, 3]));"#,
        "1\n",
    );
    assert_stdout(
        r#"function f(a){ return a.includes(9); } console.log(f([1, 2, 3]));"#,
        "false\n",
    );
}

#[test]
fn array_push_pop_on_param() {
    assert_stdout(
        r#"function f(a){ return a.push(4); } console.log(f([1, 2, 3]));"#,
        "4\n",
    );
    assert_stdout(
        r#"function f(a){ return a.pop(); } console.log(f([1, 2, 3]));"#,
        "3\n",
    );
}

#[test]
fn array_at_on_param() {
    assert_stdout(
        r#"function f(a){ return a.at(-1); } console.log(f([10, 20, 30]));"#,
        "30\n",
    );
}

// ---------------------------------------------------------------------------
// `.toString()` on a value of unknown kind — defined on EVERY value.
// ---------------------------------------------------------------------------

#[test]
fn to_string_on_unknown_number_and_string() {
    assert_stdout(
        r#"function show(x){ return x.toString(); } console.log(show(42), show("hi"));"#,
        "42 hi\n",
    );
}

// ---------------------------------------------------------------------------
// `.slice` (string or array, 1- and 2-arg) on a Tagged receiver.
// ---------------------------------------------------------------------------

#[test]
fn slice_on_tagged_string_and_array() {
    assert_stdout(
        r#"function f(s){ return s.slice(1); } console.log(f("hello"));"#,
        "ello\n",
    );
    assert_stdout(
        r#"function f(a){ return a.join(",") + "|" + a.slice(1, 3).join(","); } console.log(f([1, 2, 3, 4]));"#,
        "1,2,3,4|2,3\n",
    );
}

// ---------------------------------------------------------------------------
// `.split` on a Tagged string receiver yields an array (chainable).
// ---------------------------------------------------------------------------

#[test]
fn split_on_tagged_string() {
    assert_stdout(
        r#"function f(s){ return s.split(",").length; } console.log(f("a,b,c"));"#,
        "3\n",
    );
}

// ---------------------------------------------------------------------------
// Method on a call return (member-on-call, when bound first).
// ---------------------------------------------------------------------------

#[test]
fn method_on_call_return_bound() {
    assert_stdout(
        r#"function mk(){ return [1, 2, 3]; } let a = mk(); console.log(a.length);"#,
        "3\n",
    );
}

// ---------------------------------------------------------------------------
// Soundness bails — a method NO class implements, or a callback on a Tagged recv.
// ---------------------------------------------------------------------------

#[test]
fn unknown_method_on_tagged_bails() {
    // A method not in the dynamic table on a Tagged receiver bails (never guessed).
    assert_bails(r#"function f(s){ return s.notARealMethod(); } console.log(f("x"));"#);
}

#[test]
fn callback_on_tagged_receiver_dispatches() {
    // An array callback method with a NON-CAPTURING callback on a Tagged receiver
    // now dispatches through the `__rtsadp_arr_*` trampolines (P5.9 extension —
    // previously this bailed). Observed via `.join` (element indexing on a function
    // return is a separate dynamic-index limitation).
    super::assert_stdout(
        r#"function f(a){ return a.map(x => x * 2).join(","); } console.log(f([1, 2]));"#,
        "2,4\n",
    );
}
