import { describe, test, expect } from "rts:test";
import { promise } from "rts";

// Regression (cross-runtime #207): ordem de microtask de Promise.then/catch/
// finally sobre promise PENDING era NAO-DETERMINISTICA (spawn_blocking em
// threads tokio). Fix: enfileira PendingThen/PendingFinally na microtask queue
// (polling determinista no drain) em vez de spawn_blocking. Chains resolvem
// FIFO na ordem JS spec, deterministicamente.

let out = "";
function print(v: string): void { out += v + "\n"; }

// chain de .then sobre pending (cada .then cria pending p/ o proximo)
let log = "";
const p = promise.create(() => 1, []);
promise.wait(
  promise.then(
    promise.then(p, (v: number) => { log += "a" + v; return v + 1; }),
    (v: number) => { log += "b" + v; return v; }
  )
);
print(log);   // a1b2

// .finally preserva valor e roda na ordem
let f = "";
const p2 = promise.create(() => 42, []);
const r = promise.wait(
  promise.finally(p2, () => { f += "fin"; })
);
print("fin=" + f + " val=" + r);   // fin=fin val=42

describe("promise microtask order (#207)", () => {
  test("then/finally sobre pending sao deterministas", () =>
    expect(out).toBe("a1b2\nfin=fin val=42\n"));
});
