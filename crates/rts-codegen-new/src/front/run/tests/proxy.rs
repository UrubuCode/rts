//! PROXY (#218) — `new Proxy(target, handler)` + the `get`/`set` traps.
//!
//! A proxy is an `Entry::Proxy { target, handler }` (built by the Registry ctor
//! `__RTS_FN_GL_PROXY_NEW`). Property access on a proxy routes through the dynamic
//! property trampolines (`__rtsadp_obj_get`/`_set`), which detect the proxy
//! receiver via `resolve_proxy` and invoke `handler.get`/`.set` through the
//! `__rtsadp_fn_invoke` callback bridge — falling through to the target when the
//! handler defines no trap. The other traps (has/deleteProperty/apply/construct)
//! and `Reflect.*` are later increments.

use super::assert_stdout;

#[test]
fn get_trap_intercepts_every_key() {
    // `handler.get` fires for ANY property — present or absent on the target.
    assert_stdout(
        "const t: any = { x: 10, y: 20 }; \
         const h: any = { get: (_t: any, _k: any) => 42 }; \
         const p: any = new Proxy(t, h); \
         console.log(p.x, p.anything, p.y);",
        "42 42 42\n",
    );
}

#[test]
fn no_get_trap_reads_through_to_target() {
    // A handler WITHOUT a `get` trap reads the property straight off the target
    // (present → its value, absent → `undefined`).
    assert_stdout(
        "const t: any = { foo: 100 }; const p: any = new Proxy(t, {}); \
         console.log(p.foo, p.missing);",
        "100 undefined\n",
    );
}

#[test]
fn set_trap_intercepts_writes() {
    // `handler.set` fires on every write; the underlying target is NOT mutated by
    // the trap (it only counts), so the side effect is observable via the counter.
    assert_stdout(
        "let count = 0; const t: any = { x: 0 }; \
         const h: any = { set: (_t: any, _k: any, _v: any) => { count = count + 1; return true; } }; \
         const p: any = new Proxy(t, h); \
         p.a = 1; p.b = 2; p.c = 3; console.log(count);",
        "3\n",
    );
}

#[test]
fn inline_handler_arrow_in_new_ctor_arg() {
    // The handler object literal — with its `get` ARROW — is passed INLINE as a
    // ctor arg to `new Proxy(...)` (not a named `const`). The arrow extraction must
    // descend into `new`'s args to lift it, else "expression arrow".
    assert_stdout(
        "const p: any = new Proxy({ y: 2 }, { get: (_t: any, _k: any) => 99 }); \
         console.log(p.y);",
        "99\n",
    );
}

#[test]
fn get_trap_can_read_the_target() {
    // A `get` trap that forwards to the target (`target[key]`) — the common
    // logging-proxy shape — reads the real value through the proxy.
    assert_stdout(
        "const t: any = { n: 7 }; \
         const h: any = { get: (target: any, key: any) => target[key] }; \
         const p: any = new Proxy(t, h); console.log(p.n);",
        "7\n",
    );
}
