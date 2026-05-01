import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function double(x: i64): i64 { return x * 2; }
function addTen(x: i64): i64 { return x + 10; }
function recover(_e: i64): i64 { return 999; }

// 1. then sobre resolved
const p1 = promise.new_resolved(5);
const r1 = promise.then(p1, double);
print("r1=" + (await r1));

// 2. cadeia .then.then
const p2 = promise.new_resolved(3);
const t1 = promise.then(p2, double);
const t2 = promise.then(t1, addTen);
print("chain=" + (await t2));

// 3. catch sobre rejected (recovers)
const p3 = promise.new_rejected(50);
const r3 = promise.catch(p3, recover);
print("catch_recover=" + (await r3));

// 4. catch sobre resolved (passthrough)
const p4 = promise.new_resolved(7);
const r4 = promise.catch(p4, recover);
print("catch_passthrough=" + (await r4));

// 5. then sobre rejected (propaga rejection sem chamar callback)
const p5 = promise.new_rejected(25);
const r5 = promise.then(p5, double);
let caught: i64 = -1;
try {
  await r5;
} catch (e) {
  caught = 1;
}
print("then_on_reject_propagates=" + caught);

// 6. finally — chama callback, mantem valor original
const p6 = promise.new_resolved(8);
const r6 = promise.finally(p6, double);
print("finally_keeps_value=" + (await r6));

// 7. Cadeia complexa: then -> catch -> then.
const p7 = promise.new_rejected(100);
const c7a = promise.catch(p7, recover);    // 999
const c7b = promise.then(c7a, double);     // 1998
print("chain_recovery=" + (await c7b));

describe("promise.then/catch/finally namespace (F6, #417)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "r1=10\nchain=16\ncatch_recover=999\ncatch_passthrough=7\nthen_on_reject_propagates=1\nfinally_keeps_value=8\nchain_recovery=1998\n"
    );
  });
});
