import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Promise.resolve() sem args — antes dava "too few arguments" warning.
// Equivalente a Promise.resolve(undefined).
const p = Promise.resolve();
print("p_handle=" + (p !== 0));

// Comparar com Promise.resolve(42)
const q = Promise.resolve(42);
const qv = promise.wait(q as unknown as number);
print("q_wait=" + qv);

describe("Promise.resolve() no args (#285)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "p_handle=true\n" +
      "q_wait=42\n"
    )
  );
});
