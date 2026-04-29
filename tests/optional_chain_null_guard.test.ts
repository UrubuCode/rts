import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// obj?.prop — deve retornar undefined se obj é null, NÃO crash
const obj: { a: { b: { c: number } } } | null = null;
print(`${obj?.a}`);          // undefined
print(`${obj?.a?.b?.c}`);    // undefined

// Cadeia parcial — obj existe mas prop intermediária não
const obj2 = { a: null as { b: number } | null };
print(`${obj2?.a?.b}`);      // undefined

// Array optional access
const arr: number[] | null = null;
print(`${arr?.[0]}`);        // undefined

// Optional call
const fn: (() => number) | null = null;
print(`${fn?.()}`);          // undefined

const fn2: (() => number) | null = () => 42;
print(`${fn2?.()}`);         // 42

// Deep chain — must short-circuit, not evaluate rest
let sideEffect = false;
const deep: { getValue: () => number } | null = null;
const res = deep?.getValue();
print(`${res}`);             // undefined
print(`${sideEffect}`);      // false — side effect not triggered

// Chaining after optional
const safe = { items: [10, 20, 30] };
print(`${safe?.items?.[1]}`);  // 20

describe("optional_chain_null_guard", () => {
  test("null_guard", () => expect(__rtsCapturedOutput).toBe(
    "undefined\nundefined\nundefined\nundefined\nundefined\n42\nundefined\nfalse\n20\n"
  ));
});
