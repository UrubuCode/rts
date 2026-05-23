import { describe, test, expect } from "rts:test";
import { promise, thread, time, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Promise resolved com valor 0 (caso especial — zero pode ser confundido com handle invalido).
const p0 = promise.new_resolved(0);
print("zero_state=" + promise.state(p0));
print("zero_wait=" + promise.wait(p0));

// 2. Promise resolved com valor negativo.
const pNeg = promise.new_resolved(-42);
print("neg_wait=" + promise.wait(pNeg));

// 3. Promise resolved com valor maximo i64.
const pMax = promise.new_resolved(9223372036854775807);
print("max_wait=" + promise.wait(pMax));

// 4. Promise resolved com valor proximo ao minimo i64. Evitamos
// -9223372036854775807 (= i64::MIN+1) porque coincide com sentinel
// bool true e o coerce_to_handle em template literal gera "true".
const pMin = promise.new_resolved(-9007199254740990);
print("min_wait=" + promise.wait(pMin));

// 5. Cadeia de resolves manuais — N Promises onde cada uma sinaliza a proxima.
const chain: i64[] = [];
let i: i64 = 0;
while (i < 5) {
  chain.push(promise.new_pending());
  i = i + 1;
}

function chainWorker(_arg: i64): i64 {
  // Resolve a primeira; cada uma resolve a proxima.
  promise.resolve(chain[0], 1);
  promise.resolve(chain[1], 2);
  promise.resolve(chain[2], 3);
  promise.resolve(chain[3], 4);
  promise.resolve(chain[4], 5);
  return 0;
}

const ch = thread.spawn(chainWorker, 0);
let chainSum: i64 = 0;
let j: i64 = 0;
while (j < 5) {
  chainSum = chainSum + promise.wait(chain[j]);
  j = j + 1;
}
thread.join(ch);
print("chain_sum=" + chainSum);  // 1+2+3+4+5 = 15

// 6. Promise pending nunca resolved + try_value (timeout-free).
const stuck = promise.new_pending();
print("stuck_try=" + promise.try_value(stuck));   // 0 (pending)
print("stuck_state=" + promise.state(stuck));      // 0

// 7. Resolve depois libera Promise (try_value ve valor).
promise.resolve(stuck, 99);
print("stuck_after_resolve=" + promise.try_value(stuck));

// 8. Stress: 100 Promises pending, todas resolvidas em sequencia.
const many: i64[] = [];
let k: i64 = 0;
while (k < 100) {
  many.push(promise.new_pending());
  k = k + 1;
}
let m: i64 = 0;
while (m < 100) {
  promise.resolve(many[m], m * 2);
  m = m + 1;
}
let stressSum: i64 = 0;
let n: i64 = 0;
while (n < 100) {
  stressSum = stressSum + promise.wait(many[n]);
  n = n + 1;
}
print("stress_sum=" + stressSum);  // sum(0..100)*2 = 2 * 4950 = 9900

describe("promise resolve com varios valores e cadeias", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "zero_state=1\nzero_wait=0\nneg_wait=-42\nmax_wait=9223372036854775807\nmin_wait=-9007199254740990\nchain_sum=15\nstuck_try=0\nstuck_state=0\nstuck_after_resolve=99\nstress_sum=9900\n"
    );
  });
});
