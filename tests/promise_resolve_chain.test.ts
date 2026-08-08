import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Traducao do namespace `rts` para a superficie que fica:
//   promise.new_resolved(v)  -> Promise.resolve(v)
//   promise.new_pending()    -> new Promise, guardando o `resolve` do executor
//   promise.wait(p)          -> await p
//   promise.state(p)         -> ver `peek` abaixo
//   promise.try_value(p)     -> ver `peek` abaixo
//
// `state`/`try_value` liam o estado SINCRONAMENTE, o que a superficie padrao
// nao oferece de todo. O que ela oferece, e que da' exatamente a mesma leitura,
// e' `Promise.race([p, Promise.resolve(0)])`: a spec manda subscrever pela
// ordem do iteravel, logo se `p` ja' esta' settled ele ganha e devolve o seu
// valor; se esta' pendente ganha a sentinela e devolve 0 — que era precisamente
// o que `try_value` devolvia para uma pendente.
function peek(p: any): any {
  return Promise.race([p, Promise.resolve(0)]);
}

// Um par (promise, resolve) — o substituto de `new_pending()` + `resolve(h, v)`.
let __cap: any = null;
function pending(): any {
  __cap = null;
  const p = new Promise((resolve: any) => { __cap = resolve; });
  (p as any).settle = __cap;
  return p;
}

// `state` (0=pending, 1=fulfilled, 2=rejected) lido pela unica via que a
// superficie padrao promete: qual dos dois callbacks de `.then` corre.
let __st: i64 = 0;
async function stateOf(p: any): i64 {
  __st = 0;
  await p.then((v: any) => { __st = 1; return v; })
         .catch((e: any) => { __st = 2; return e; });
  return __st;
}

// 1. Promise resolved com valor 0 (caso especial — zero era confundivel com
// handle invalido no desenho antigo; aqui e' so' um valor).
const p0 = Promise.resolve(0);
print("zero_state=" + (await stateOf(p0)));
print("zero_wait=" + (await p0));

// 2. Promise resolved com valor negativo.
print("neg_wait=" + (await Promise.resolve(-42)));

// 3. Promise resolved com valor maximo i64.
print("max_wait=" + (await Promise.resolve(9223372036854775807)));

// 4. Promise resolved com valor proximo ao minimo i64.
print("min_wait=" + (await Promise.resolve(-9007199254740990)));

// 5. Cadeia de resolves manuais — N Promises, cada uma sinalizada por sua vez.
// A versao antiga fazia os resolves numa OUTRA thread (`thread.spawn`) e a
// principal bloqueava em `promise.wait`. Isso nao tem equivalente: uma Promise
// e' de um so' realm e um worker nao lhe toca. Ficou a parte que a superficie
// que fica garante — resolve tardio, valor entregue a quem espera, na ordem —
// e a dimensao multi-thread saiu, o que se declara aqui em vez de se fingir.
const chain: any[] = [];
let i: i64 = 0;
while (i < 5) {
  chain.push(pending());
  i = i + 1;
}
chain[0].settle(1);
chain[1].settle(2);
chain[2].settle(3);
chain[3].settle(4);
chain[4].settle(5);
let chainSum: i64 = 0;
let j: i64 = 0;
while (j < 5) {
  chainSum = chainSum + (await chain[j]);
  j = j + 1;
}
print("chain_sum=" + chainSum);  // 1+2+3+4+5 = 15

// 6. Promise pendente que nunca e' resolvida — peek nao bloqueia e da' 0.
const stuck = pending();
print("stuck_peek=" + (await peek(stuck)));   // 0 (pendente)

// 7. Depois de resolvida, o peek ve' o valor.
stuck.settle(99);
print("stuck_after_resolve=" + (await peek(stuck)));

// 8. Stress: 100 Promises pendentes, todas resolvidas em sequencia.
const many: any[] = [];
let k: i64 = 0;
while (k < 100) {
  many.push(pending());
  k = k + 1;
}
let m: i64 = 0;
while (m < 100) {
  many[m].settle(m * 2);
  m = m + 1;
}
let stressSum: i64 = 0;
let n: i64 = 0;
while (n < 100) {
  stressSum = stressSum + (await many[n]);
  n = n + 1;
}
print("stress_sum=" + stressSum);  // sum(0..100)*2 = 2 * 4950 = 9900

describe("promise resolve com varios valores e cadeias", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      // max_wait: 9223372036854775807 (i64::MAX) > 2^53 não cabe na mantissa
      // f64; vira 9223372036854776000, exatamente como JS/Node imprime o
      // literal numérico.
      "zero_state=1\nzero_wait=0\nneg_wait=-42\nmax_wait=9223372036854776000\nmin_wait=-9007199254740990\nchain_sum=15\nstuck_peek=0\nstuck_after_resolve=99\nstress_sum=9900\n"
    );
  });
});
