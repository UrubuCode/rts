//! `Symbol` — the PRIMORDIAL symbol primitive (#216). `Symbol(desc)` call form,
//! `Symbol()` no-arg, `Symbol.for`/`keyFor` statics, and the `.description` getter
//! (in-scope). Each runs end to end with EXACT stdout.

use super::assert_stdout;

#[test]
fn symbol_call_form_distinct_handles() {
    // `Symbol(desc)` returns a unique handle each call; `===` is identity.
    assert_stdout(
        "const a = Symbol(\"x\"); const b = Symbol(\"x\"); console.log(a !== b, a === a);",
        "true true\n",
    );
}

#[test]
fn symbol_no_arg() {
    // `Symbol()` (0 args, optional description) is valid and unique.
    assert_stdout(
        "const a = Symbol(); const b = Symbol(); console.log(a !== b);",
        "true\n",
    );
}

#[test]
fn symbol_description_getter() {
    // `Symbol(desc).description` reads the description back (an InstanceGetter).
    assert_stdout(
        "const s = Symbol(\"hello\"); console.log(s.description);",
        "hello\n",
    );
}

#[test]
fn symbol_for_returns_same_handle() {
    // `Symbol.for(key)` — the global registry: same key → same handle.
    assert_stdout(
        "const r1 = Symbol.for(\"k\"); const r2 = Symbol.for(\"k\"); console.log(r1 === r2);",
        "true\n",
    );
}
