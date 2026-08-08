import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `promise.*` e `collections.vec_*` do namespace `rts` viraram `Promise.*`
// sobre arrays.
//
// O `time.sleep_ms(20)` dentro de `ok()` existia so' para tornar o `race`
// deterministico: a ordem de scan do `promise.race` antigo nao era garantida,
// e o atraso forcava o fast_fail a vencer. A superficie que fica nao precisa
// do truque — a spec manda subscrever na ordem do iteravel, portanto entre
// duas ja-settled vence a primeira do array. O sleep saiu porque a garantia
// que ele simulava passou a ser prometida; o determinismo afirmado e' o mesmo.
// Membros rejeitados escritos como `Promise.reject(m)` em vez de uma async fn
// que faz `throw`: sao a mesma coisa observavel (uma Promise ja rejeitada) e e
// a forma que nao depende de QUANDO o corpo da async fn corre.
function fail(msg: string): any { return Promise.reject(msg); }
async function ok(x: i64): i64 { return x; }

// A rejeicao e' colhida por `.catch` em vez de `try/catch` em volta do
// `await`. Sao as duas leituras que a spec oferece para a MESMA rejeicao, e o
// `.catch` e' o que pertence a superficie de Promise — a razao recebida e a
// assercao sobre ela sao identicas.

// 1. Promise.all rejeita na primeira falha.
await Promise.all([ok(1), fail("middle"), ok(3)])
  .catch((e: any) => { print("all_rej: " + e); return 0; });

// 2. Promise.race com primeiro rejected.
function fast_fail(msg: string): any { return Promise.reject(msg); }
await Promise.race([fast_fail("loser-error"), ok(99)])
  .catch((e: any) => { print("race_rej: " + e); return 0; });

// 3. Promise.any com TODAS rejeitando: rejeita com AggregateError.
let any_caught: i64 = 0;
await Promise.any([fail("a"), fail("b"), fail("c")])
  .catch((_e: any) => { any_caught = 1; return 0; });
print("any_all_rejected_caught=" + any_caught);

// 4. Promise.any com mix — pega primeira fulfilled.
const v4r = await Promise.any([fail("nope"), ok(7), fail("late")]);
print("any_mixed=" + v4r);

// 5. try/catch sem await dentro — fluxo normal.
let counter: i64 = 0;
try {
  counter = 100;
} catch (e) {
  counter = -1;
}
print("normal_flow=" + counter);

describe("promise rejection com combinators", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "all_rej: middle\nrace_rej: loser-error\nany_all_rejected_caught=1\nany_mixed=7\nnormal_flow=100\n"
    );
  });
});
