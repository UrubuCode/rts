import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `promise.then(p, f)` virou `p.then(f)`; `promise.create(f, args)` — que
// corria `f(...args)` dentro de uma Promise — virou a forma padrao de erguer
// uma chamada sincrona para uma Promise: uma async fn que a executa. Os valores
// afirmados sao os mesmos; o que se testa continua a ser que o callback pode
// ser uma fn de utilizador, um `bind` ou um `new Function`.

function double(x: i64): i64 { return x * 2; }
function inc(x: i64): i64 { return x + 1; }

// 1. then com user fn ident
print("then_userfn=" + (await Promise.resolve(21).then(double)));

// 2. then com handle Function via bind — era SIGSEGV, fix #359-followup
const incBound = inc.bind(0);
print("then_bound=" + (await Promise.resolve(10).then(incBound)));

// 3. then com new Function dinamica
const triple = new Function("x", "return x * 3;");
print("then_dyn=" + (await Promise.resolve(7).then(triple)));

// 4. erguer chamada de user fn para Promise
async function callDouble(x: i64): i64 { return double(x); }
print("create_userfn=" + (await callDouble(50)));

// 5. o mesmo com new Function
const sq = new Function("x", "return x * x;");
async function callSq(x: i64): i64 { return sq(x); }
print("create_dyn=" + (await callSq(9)));

// 6. rejection via throw dentro da chamada erguida
function fails(x: i64): i64 {
    if (x > 0) throw 999;
    return x;
}
async function callFails(x: i64): i64 { return fails(x); }
let caught: i64 = -1;
try {
    await callFails(5);
} catch (e) {
    caught = 1;
}
print("create_throw=" + caught);

// 7. fn de aridade 0
function noargs(): i64 { return 777; }
async function callNoargs(): i64 { return noargs(); }
print("create_noargs=" + (await callNoargs()));

describe("promise + function (#359 followup)", () => {
  test("then aceita user fn, bind e new Function; chamada erguida a Promise", () => {
    expect(__rtsCapturedOutput).toBe(
      "then_userfn=42\nthen_bound=11\nthen_dyn=21\ncreate_userfn=100\ncreate_dyn=81\ncreate_throw=1\ncreate_noargs=777\n"
    );
  });
});
