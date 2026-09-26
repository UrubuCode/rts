import { describe, test, expect } from "rts:test";

// A `try` body and its `finally` are blocks: a `let` written in one is a new binding,
// and the binding of the same name outside keeps its value. The MIR stage read both
// bodies in the enclosing scope, so the inner declaration was lowered as a write to
// the outer binding -- a wrong answer with nothing refused.

function shadowedInTry(): number {
  let x = 1;
  try {
    let x = 2;
    x = x + 10;
  } finally {
    let x = 3;
    x = x + 20;
  }
  return x;
}

function shadowedInFinallyOnly(): number {
  let y = 5;
  try {
    y = y + 1;
  } finally {
    const y = 100;
    void y;
  }
  return y;
}

function declaredInTryAndRead(): number {
  try {
    const z = 7;
    return z * 2;
  } catch (e) {
    return -1;
  }
}

describe("a try body and its finally are blocks", () => {
  test("a let in the try and in the finally shadows, and the outer binding keeps its value", () => {
    expect(shadowedInTry()).toBe(1);
  });
  test("a const in the finally shadows an outer binding the try body assigned", () => {
    expect(shadowedInFinallyOnly()).toBe(6);
  });
  test("a const declared in the try body is read there", () => {
    expect(declaredInTryAndRead()).toBe(14);
  });
});
