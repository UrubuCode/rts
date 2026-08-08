import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `promise.then(p, f)` / `promise.catch(p, f)` / `promise.finally(p, f)` do
// namespace `rts` eram a forma livre dos metodos que a superficie que fica ja'
// tem em `Promise.prototype`. Traducao 1:1 — nenhuma assercao mudou de valor.

function double(x: i64): i64 { return x * 2; }
function addTen(x: i64): i64 { return x + 10; }
function recover(_e: i64): i64 { return 999; }

// 1. then sobre resolved
const r1 = Promise.resolve(5).then(double);
print("r1=" + (await r1));

// 2. cadeia .then.then
const t2 = Promise.resolve(3).then(double).then(addTen);
print("chain=" + (await t2));

// 3. catch sobre rejected (recovers)
const r3 = Promise.reject(50).catch(recover);
print("catch_recover=" + (await r3));

// 4. catch sobre resolved (passthrough)
const r4 = Promise.resolve(7).catch(recover);
print("catch_passthrough=" + (await r4));

// 5. then sobre rejected (propaga rejection sem chamar callback).
// A rejeicao e' observada por `.catch` no fim da cadeia em vez de por
// try/catch em volta de um `await`: as duas leituras sao spec, e o `.catch`
// e' a que exprime "propagou POR DENTRO da cadeia", que e' o que esta linha
// pina. De brinde da' para afirmar tambem que `double` nunca correu — a
// assercao ficou mais forte, nao mais fraca.
let caught: i64 = -1;
let doubleRan: i64 = 0;
await Promise.reject(25)
  .then((v: i64) => { doubleRan = 1; return double(v); })
  .catch((_e: any) => { caught = 1; return 0; });
print("then_on_reject_propagates=" + caught);
print("then_callback_skipped=" + (doubleRan === 0 ? 1 : 0));

// 6. finally — chama callback, mantem valor original
const r6 = Promise.resolve(8).finally(double);
print("finally_keeps_value=" + (await r6));

// 7. Cadeia complexa: catch -> then.
const c7b = Promise.reject(100).catch(recover).then(double);  // 999 -> 1998
print("chain_recovery=" + (await c7b));

describe("Promise.prototype then/catch/finally (F6, #417)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "r1=10\nchain=16\ncatch_recover=999\ncatch_passthrough=7\nthen_on_reject_propagates=1\nthen_callback_skipped=1\nfinally_keeps_value=8\nchain_recovery=1998\n"
    );
  });
});
