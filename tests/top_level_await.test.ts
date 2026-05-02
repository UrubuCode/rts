import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function double(x: i64): i64 { return x * 2; }
async function compute(): i64 {
  const a = await double(5);
  const b = await double(10);
  return a + b;
}

// 1. await em top-level com promise resolved
const p1 = promise.new_resolved(42);
print("v1=" + (await p1));

// 2. await de async fn em top-level
print("v2=" + (await double(7)));

// 3. async fn que faz await dela mesma — chamada de top-level
print("v3=" + (await compute()));

// 4. await em sequencia
const a = await promise.new_resolved(1);
const b = await promise.new_resolved(2);
const c = await promise.new_resolved(3);
print("seq=" + (a + b + c));

// 5. await em condicional top-level
const cond = await promise.new_resolved(10);
if (cond > 5) { print("big"); } else { print("small"); }

// 6. await dentro de for loop top-level
let total: i64 = 0;
for (let i: i64 = 0; i < 5; i = i + 1) {
  total = total + (await promise.new_resolved(i));
}
print("loop=" + total);

// 7. try/catch top-level com await rejeitado
const pr = promise.new_rejected(99);
let caught: i64 = -1;
try {
  await pr;
} catch (e) {
  caught = 1;
}
print("caught=" + caught);

describe("top-level await (F7, #418)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "v1=42\nv2=14\nv3=30\nseq=6\nbig\nloop=10\ncaught=1\n"
    );
  });
});
