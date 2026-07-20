//! `Error(message, { cause })` — the ES2022 options bag; `cause` becomes
//! `error.cause`. The 1-arg form leaves `cause` undefined.

use super::assert_stdout;

#[test]
fn error_with_cause_option() {
    assert_stdout(
        "const e=new Error(\"outer\",{cause:new Error(\"inner\")}); console.log(e.message,e.cause.message);",
        "outer inner\n",
    );
}

#[test]
fn error_one_arg_leaves_cause_undefined() {
    assert_stdout(
        "const e=new Error(\"x\"); console.log(e.cause===undefined);",
        "true\n",
    );
}
