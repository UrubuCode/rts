import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #207): ordem de microtask de Promise.then/catch/
// finally sobre promise PENDING era NAO-DETERMINISTICA (spawn_blocking em
// threads tokio). Fix: enfileira PendingThen/PendingFinally na microtask queue
// (polling determinista no drain) em vez de spawn_blocking. Chains resolvem
// FIFO na ordem JS spec, deterministicamente.
//
// `promise.create(fn, [])` + `promise.then(p, f)` + `promise.wait(p)` do
// namespace `rts` viraram a superficie que fica: `Promise.resolve().then(fn)`
// produz a mesma promise pendente-ate-a-microtask, `.then` encadeia e `await`
// substitui a espera. A ordem afirmada e' a da spec, entao e' a mesma.

let out = "";
function print(v: string): void { out += v + "\n"; }

// chain de .then sobre pending (cada .then cria pending p/ o proximo)
let log = "";
await Promise.resolve()
  .then(() => 1)
  .then((v: number) => { log += "a" + v; return v + 1; })
  .then((v: number) => { log += "b" + v; return v; });
print(log);   // a1b2

// .finally preserva valor e roda na ordem
let f = "";
const r = await Promise.resolve()
  .then(() => 42)
  .finally(() => { f += "fin"; });
print("fin=" + f + " val=" + r);   // fin=fin val=42

describe("promise microtask order (#207)", () => {
  test("then/finally sobre pending sao deterministas", () =>
    expect(out).toBe("a1b2\nfin=fin val=42\n"));
});
