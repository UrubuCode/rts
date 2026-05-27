import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #348 parcial): spread em chamada a user fn
// (`f(...xs)`) dava `error: spread not supported` — o call direto eh
// posicional (aridade fixa). Fix: rotear via INVOKE_AUTO sobre handle
// Function reificado com param_kinds (invoke_typed reinterpreta f64-bits).
// Cobre args number (f64) e int, spread puro e misto com posicionais.

let out = "";
function print(v: string): void { out += v + "\n"; }

function adder(a: number, b: number, c: number): number { return a + b + c; }
function sum2(a: number, b: number): number { return a + b; }

// spread puro
const xs = [1, 2, 3];
print("" + adder(...xs));        // 6

// spread misto: posicional + spread
const tail = [20, 30];
print("" + adder(10, ...tail));  // 60

// spread no meio
const mid = [2];
print("" + adder(1, ...mid));    // adder(1,2,undefined) -> NaN-ish; usa 2 args validos
print("" + sum2(...[7, 8]));     // 15

// floats
function favg(a: number, b: number): number { return (a + b) / 2; }
print("" + favg(...[3.0, 4.0])); // 3.5

describe("spread em user fn call", () => {
  test("number args via spread", () =>
    expect(out.indexOf("6\n") === 0).toBe(true));
  test("spread misto", () =>
    expect(out.indexOf("60\n") >= 0).toBe(true));
  test("float spread", () =>
    expect(out.indexOf("3.5\n") >= 0).toBe(true));
  test("sum2 spread", () =>
    expect(out.indexOf("15\n") >= 0).toBe(true));
});
