// node:vm — the one call shape that KILLS THE WHOLE PROCESS, isolated so the
// rest of `claude-node-vm.test.ts` stays measurable.
//
// Root cause (see `claude-node-vm.test.ts`'s longer comment): every call to a
// `vm.compileFunction(...)`-produced callable answers `undefined` instead of
// running its body with the bound arguments, because `param_names` in
// `vm.rs` reads the `params` array with `entry::get_member(context, params,
// &index.to_string())` — a STRING key built at runtime — which does not
// resolve an array element (unlike `entry::get_indexed` with a numeric key,
// which is what every OTHER module in this crate reading an array-like uses
// for the same job). So `__params__` is stored as `""`, and the wrapper
// source `invoke_compiled` builds becomes `(function() { <body> })()` — a
// ZERO-parameter call whose body still references the free names the caller
// meant to bind as parameters.
//
// Normally this is silent: `invoke_compiled` reads `this.__params__`, and a
// PLAIN call `fn(1, 2)` binds `this` to `undefined` (strict-mode call, no
// receiver) rather than to `fn` itself, so `get_member(undefined,
// "__params__")` itself answers `undefined`, `text_of` fails, and
// `invoke_compiled` returns early — silently wrong (see the main file), but
// not fatal.
//
// It stops being silent the moment `this` IS the callable — `fn.call(fn,
// ...)`, `fn.apply(fn, ...)`, or a bound/rebound copy of it — which is an
// ordinary thing to write if a caller passes the function around by
// reference and later invokes it through `.call`. Then `__params__`/
// `__body__` ARE found (both real strings), the wrapper source really is
// `(function() { return a + b; })()`, and it runs as its own brand-new
// `entry::evaluate` program with no `a`/`b` bound anywhere in it — a
// `ReferenceError: a is not defined` that reaches nothing this crate can
// catch and takes the whole process down. Reproduced identically via `rts
// run` on a two-line script outside the test harness, so this is not an
// artifact of how `rts:test` calls into a file.
import { describe, test, expect } from "rts:test";
import * as vm from "node:vm";

const addFn: any = vm.compileFunction("return a + b;", ["a", "b"]);

describe("node:vm — compileFunction callable invoked via .call(fn, ...)", () => {
    test("does not crash the process (currently it does — see comment)", () => {
        // THE KILLING LINE. Commenting this out is what makes the rest of the
        // suite runnable again; it is here, uncommented, because a red
        // assertion is not what this file is for — dying IS the finding.
        const result = addFn.call(addFn, 3, 4);
        expect(result).toBe(7);
    });
});
