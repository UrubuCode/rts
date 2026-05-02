import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function double(x: i64): i64 { return x * 2; }
function inc(x: i64): i64 { return x + 1; }

// 1. promise.then com user fn ident — sempre funcionou
const p1 = promise.new_resolved(21);
print("then_userfn=" + (await promise.then(p1, double)));

// 2. promise.then com handle Function via bind — era SIGSEGV, fix #359-followup
const incBound = inc.bind(0);
const p2 = promise.new_resolved(10);
print("then_bound=" + (await promise.then(p2, incBound)));

// 3. promise.then com new Function dinamica
const triple = new Function("x", "return x * 3;");
const p3 = promise.new_resolved(7);
print("then_dyn=" + (await promise.then(p3, triple)));

// 4. promise.create — entrypoint Promise-centric (drysius design)
const pc1 = promise.create(double, [50]);
print("create_userfn=" + (await pc1));

// 5. promise.create com new Function
const sq = new Function("x", "return x * x;");
const pc2 = promise.create(sq, [9]);
print("create_dyn=" + (await pc2));

// 6. promise.create + rejection via throw
function fails(x: i64): i64 {
    if (x > 0) throw 999;
    return x;
}
let caught: i64 = -1;
try {
    const pf = promise.create(fails, [5]);
    await pf;
} catch (e) {
    caught = 1;
}
print("create_throw=" + caught);

// 7. promise.create sem args (fn de aridade 0)
function noargs(): i64 { return 777; }
const pc3 = promise.create(noargs, 0);
print("create_noargs=" + (await pc3));

describe("promise + function (#359 followup)", () => {
  test("then aceita handle Function + promise.create entrypoint", () => {
    expect(__rtsCapturedOutput).toBe(
      "then_userfn=42\nthen_bound=11\nthen_dyn=21\ncreate_userfn=100\ncreate_dyn=81\ncreate_throw=1\ncreate_noargs=777\n"
    );
  });
});
