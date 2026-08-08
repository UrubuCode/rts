import { describe, test, expect } from "rts:test";
import { atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Traducao do namespace `rts` para a superficie que fica. `atomic` NAO muda:
// e' machine-level e continua onde esta'.
//
//   promise.new_pending()  -> new Promise, guardando resolve/reject do executor
//   promise.resolve(h, v)  -> essa funcao guardada
//   promise.wait(h)        -> await
//   promise.state(h)       -> `stateOf` (qual callback de .then corre)
//   promise.try_value(h)   -> `peek`: Promise.race([p, Promise.resolve(0)]),
//                             que pela ordem de subscricao da spec devolve o
//                             valor se ja' settled e 0 se ainda pendente
//
// As transicoes eram DISPARADAS NOUTRA THREAD (`thread.spawn` + `time.sleep`).
// Isso nao tem equivalente na superficie que fica: uma Promise pertence a um
// realm e um worker nao lhe toca. O que ficou e' a transicao em si — pendente
// -> fulfilled/rejected, no-op depois de settled, varios esperadores a verem o
// mesmo valor — que era o que cada assercao media. A dimensao multi-thread saiu
// e esta' declarada aqui em vez de simulada.

function peek(p: any): any {
  return Promise.race([p, Promise.resolve(0)]);
}

let __st: i64 = 0;
async function stateOf(p: any): i64 {
  __st = 0;
  await p.then((v: any) => { __st = 1; return v; })
         .catch((e: any) => { __st = 2; return e; });
  return __st;
}

let __res: any = null;
let __rej: any = null;
function pending(): any {
  const p: any = new Promise((resolve: any, reject: any) => {
    __res = resolve;
    __rej = reject;
  });
  p.settle = __res;
  p.fail = __rej;
  return p;
}

// 1. Pending → fulfilled.
const counter = atomic.i64_new(0);
const p1 = pending();
print("p1_peek_pending=" + (await peek(p1)));   // 0 — ainda pendente
p1.settle(111);
atomic.i64_fetch_add(counter, 1);
print("p1_wait=" + (await p1));
print("p1_state_after=" + (await stateOf(p1)));
print("counter=" + atomic.i64_load(counter));

// 2. Pending → rejected.
const p2 = pending();
let p2reason: i64 = 0;
p2.catch((e: i64) => { p2reason = e; return e; });
p2.fail(666);
print("p2_reason=" + (await p2.catch((e: i64) => e)));
print("p2_state=" + (await stateOf(p2)));

// 3. resolve apos reject = no-op.
const p3 = pending();
p3.fail(50);
p3.settle(100);
print("p3_state=" + (await stateOf(p3)));
print("p3_value=" + (await p3.catch((e: i64) => e)));

// 4. peek em pending = 0; depois de settled = valor.
const p4 = pending();
print("p4_peek_pending=" + (await peek(p4)));
p4.settle(7);
print("p4_peek_settled=" + (await peek(p4)));

// 5. Multiplos esperadores da mesma Promise (todos veem o valor).
const p5 = pending();
const seen = atomic.i64_new(0);
p5.settle(13);
let w: i64 = 0;
while (w < 3) {
  atomic.i64_fetch_add(seen, await p5);
  w = w + 1;
}
print("p5_seen=" + atomic.i64_load(seen));  // 3 * 13 = 39

// 6. `promise.wait(0)`/`promise.state(0)` devolviam 0/-1 porque 0 nao era
// handle de Promise. Nao ha handles na superficie que fica, logo nao ha estado
// invalido; o que substitui a garantia e' `Promise.resolve` sobre nao-thenable.
print("non_thenable_wait=" + (await Promise.resolve(0)));
print("non_thenable_state=" + (await stateOf(Promise.resolve(0))));

describe("promise state transitions", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "p1_peek_pending=0\np1_wait=111\np1_state_after=1\ncounter=1\np2_reason=666\np2_state=2\np3_state=2\np3_value=50\np4_peek_pending=0\np4_peek_settled=7\np5_seen=39\nnon_thenable_wait=0\nnon_thenable_state=1\n"
    );
  });
});
