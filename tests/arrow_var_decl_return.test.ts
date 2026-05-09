// (#450 follow-up) Bug pre-existente: \`const arr = (a,b) => a+b\` sem
// anotacao explicita virava fn void no codegen, retornando 0 sempre.
// Fix: heuristica i64 quando o body tem return value (mesmo padrao
// de hoist_fn).

import { describe, test, expect } from "rts:test";

const a0 = () => 42;
const a1 = (x: i64) => x * 2;
const a2 = (a: i64, b: i64) => a + b;

// Block body
const a3 = (a: i64, b: i64) => { return a * b; };

// Conditional return
const a4 = (x: i64) => {
    if (x > 0) return x;
    return -x;
};

// No return value (void)
let side = 0;
const a5 = (x: i64) => { side = x; };

const r0 = a0();
const r1 = a1(10);
const r2 = a2(7, 8);
const r3 = a3(3, 4);
const r4a = a4(5);
const r4b = a4(-5);
a5(99);

describe("arrow_var_decl_return", () => {
    test("a0 (no params)", () => expect(r0).toBe(42));
    test("a1 (single param)", () => expect(r1).toBe(20));
    test("a2 (two params)", () => expect(r2).toBe(15));
    test("a3 (block body)", () => expect(r3).toBe(12));
    test("a4 positive", () => expect(r4a).toBe(5));
    test("a4 negative", () => expect(r4b).toBe(5));
    test("a5 (void) side effect", () => expect(side).toBe(99));
});
