import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __out: string = "";
function p(s: string): void { __out += s + "\n"; }

// 1. Return arrow simples — chama imediatamente.
function makeHello(): i64 {
  return () => { p("hello"); };
}

// 2. Return arrow que chama outra user fn.
function sayWorld(): void { p("world"); }
function makeWorld(): i64 {
  return () => { sayWorld(); };
}

// 3. Return arrow condicional (dois ramos, ambos lift dentro de if).
function makeBranchy(flag: i32): i64 {
  if (flag > 0) {
    return () => { p("positive"); };
  }
  return () => { p("non-positive"); };
}

const f1 = makeHello(); f1();
const f2 = makeWorld(); f2();
const f3 = makeBranchy(1); f3();
const f4 = makeBranchy(-1); f4();

const __expected: string = "hello\nworld\npositive\nnon-positive\n";

describe("fixture:arrow_lift_return", () => {
  test("matches expected stdout", () => {
    expect(__out).toBe(__expected);
  });
});
