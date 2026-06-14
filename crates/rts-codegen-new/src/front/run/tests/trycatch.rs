//! P5.13: `try`/`catch`/`finally` + `throw` — the manual-unwind exception model
//! (a thread-local pending-error slot + sentinel-return unwinding, NO real stack
//! unwind). Each test runs a REAL `.ts` program end to end and asserts exact
//! captured stdout.

use super::{assert_bails, assert_stdout};

#[test]
fn basic_throw_error_message() {
    assert_stdout(
        r#"try { throw new Error("boom"); } catch (e) { console.log(e.message); }"#,
        "boom\n",
    );
}

#[test]
fn throw_string() {
    assert_stdout(
        r#"try { throw "oops"; } catch (e) { console.log(e); }"#,
        "oops\n",
    );
}

#[test]
fn finally_runs() {
    assert_stdout(
        r#"try { console.log("a"); } finally { console.log("b"); }"#,
        "a\nb\n",
    );
}

#[test]
fn catch_and_finally() {
    assert_stdout(
        r#"try { throw new Error("x"); } catch (e) { console.log("caught"); } finally { console.log("done"); }"#,
        "caught\ndone\n",
    );
}

#[test]
fn no_throw_skips_catch() {
    assert_stdout(
        r#"try { console.log("ok"); } catch (e) { console.log("no"); }"#,
        "ok\n",
    );
}

#[test]
fn propagation_through_a_call() {
    assert_stdout(
        r#"function boom() { throw new Error("deep"); }
try { boom(); } catch (e) { console.log(e.message); }"#,
        "deep\n",
    );
}

#[test]
fn throw_in_fn_value_after_not_used() {
    // The `return 1` after the throw must NOT run; the assignment `r = f()` must
    // not bind a value (the throw unwinds before the call's result is used).
    assert_stdout(
        r#"function f() { throw new Error("e"); return 1; }
let r = "before";
try { r = f(); } catch (e) { r = "caught"; }
console.log(r);"#,
        "caught\n",
    );
}

#[test]
fn typed_error_name_and_message() {
    assert_stdout(
        r#"try { throw new TypeError("bad"); } catch (e) { console.log(e.name, e.message); }"#,
        "TypeError bad\n",
    );
}

#[test]
fn catch_no_binding() {
    assert_stdout(
        r#"try { throw 1; } catch { console.log("caught"); }"#,
        "caught\n",
    );
}

#[test]
fn nested_try_rethrow() {
    assert_stdout(
        r#"try {
  try { throw new Error("inner"); } catch (e) { throw new Error("outer"); }
} catch (e) { console.log(e.message); }"#,
        "outer\n",
    );
}

#[test]
fn rethrow_in_catch_propagates() {
    // `throw e` in a catch re-sets the slot; the outer catch reads the SAME error.
    assert_stdout(
        r#"try {
  try { throw new Error("again"); } catch (e) { throw e; }
} catch (e) { console.log(e.message); }"#,
        "again\n",
    );
}

#[test]
fn finally_runs_after_caught_throw_in_catch() {
    // A throw in `catch` still runs `finally`, then propagates to the outer catch.
    assert_stdout(
        r#"try {
  try { throw new Error("a"); }
  catch (e) { throw new Error("b"); }
  finally { console.log("fin"); }
} catch (e) { console.log(e.message); }"#,
        "fin\nb\n",
    );
}

#[test]
fn try_finally_no_catch_propagates() {
    // Body throws; finally runs; the error keeps propagating to the outer catch.
    assert_stdout(
        r#"try {
  try { throw new Error("p"); } finally { console.log("cleanup"); }
} catch (e) { console.log(e.message); }"#,
        "cleanup\np\n",
    );
}

#[test]
fn no_throw_finally_only() {
    // try/finally with no catch and no throw: body + finally, normal flow after.
    assert_stdout(
        r#"try { console.log("body"); } finally { console.log("fin"); }
console.log("after");"#,
        "body\nfin\nafter\n",
    );
}

#[test]
fn throw_number_caught() {
    assert_stdout(
        r#"try { throw 42; } catch (e) { console.log(e); }"#,
        "42\n",
    );
}

#[test]
fn code_after_catch_runs() {
    assert_stdout(
        r#"try { throw new Error("z"); } catch (e) { console.log("c"); }
console.log("next");"#,
        "c\nnext\n",
    );
}

#[test]
fn deep_propagation_two_levels() {
    assert_stdout(
        r#"function inner() { throw new Error("boom"); }
function outer() { inner(); return 0; }
try { outer(); } catch (e) { console.log(e.message); }"#,
        "boom\n",
    );
}

// ---- bails (out of the implemented subset; explicit Unsupported, not a crash) ----

#[test]
fn bail_async_try() {
    // async functions are not in the run subset at all → bail.
    assert_bails(
        r#"async function f() { try { throw new Error("x"); } catch (e) { console.log(e.message); } }
f();"#,
    );
}
