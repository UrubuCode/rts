//! Reflect (#218) — `Reflect.get`/`set`/`has`, a pure-`.ts` stdlib over dynamic
//! property access. `Reflect.get`/`set` on a Proxy fire its traps because
//! `target[key]` routes through the dynamic property trampolines (which detect the
//! proxy). The descriptor / prototype / apply reflectors are a later increment.

use super::assert_stdout;

#[test]
fn reflect_get_set_has_on_plain_object() {
    assert_stdout(
        "const o: any = { a: 1 }; Reflect.set(o, \"b\", 2); \
         console.log(Reflect.get(o, \"a\"), Reflect.get(o, \"b\"), \
         Reflect.has(o, \"a\"), Reflect.has(o, \"z\"));",
        "1 2 true false\n",
    );
}

#[test]
fn reflect_get_fires_proxy_get_trap() {
    // `Reflect.get(proxy, k)` → `proxy[k]` → the proxy's `get` trap.
    assert_stdout(
        "const t: any = { v: 5 }; \
         const p: any = new Proxy(t, { get: (_t: any, _k: any) => 77 }); \
         console.log(Reflect.get(p, \"v\"), Reflect.get(p, \"other\"));",
        "77 77\n",
    );
}

#[test]
fn reflect_delete_property_and_own_keys() {
    assert_stdout(
        "const o: any = { a: 1, b: 2, c: 3 }; \
         const ok = Reflect.deleteProperty(o, \"b\"); \
         console.log(ok, Reflect.ownKeys(o).join(\",\"), Reflect.has(o, \"b\"));",
        "true a,c false\n",
    );
}

#[test]
fn reflect_set_fires_proxy_set_trap() {
    // `Reflect.set(proxy, k, v)` → `proxy[k] = v` → the proxy's `set` trap.
    assert_stdout(
        "let hits = 0; const t: any = { x: 0 }; \
         const p: any = new Proxy(t, { set: (_t: any, _k: any, _v: any) => { hits = hits + 1; return true; } }); \
         Reflect.set(p, \"x\", 1); Reflect.set(p, \"y\", 2); console.log(hits);",
        "2\n",
    );
}
