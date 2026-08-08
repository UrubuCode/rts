import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Promise.resolve() sem args — antes dava "too few arguments" warning.
// Equivalente a Promise.resolve(undefined). `p_handle=(p !== 0)` era a leitura
// do handle inteiro do namespace `rts`; sem handles, o que a superficie que
// fica garante e' que se obtem uma Promise cumprida com `undefined`.
const p = Promise.resolve();
print("p_value=" + (await p));

// Comparar com Promise.resolve(42) — `promise.wait` virou `await`.
const q = Promise.resolve(42);
print("q_wait=" + (await q));

describe("Promise.resolve() no args (#285)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "p_value=undefined\n" +
      "q_wait=42\n"
    )
  );
});
