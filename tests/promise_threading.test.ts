import { describe, test, expect } from "rts:test";
import { promise, thread, time, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Cenario stress: muitas Promises pending sendo resolvidas em paralelo.

const total = atomic.i64_new(0);

function resolverWorker(p: i64): i64 {
  // Cada worker resolve sua Promise com 1.
  promise.resolve(p, 1);
  return 0;
}

// 50 Promises, 50 threads resolvendo cada uma, await sequencial.
const N: i64 = 50;
const promises: i64[] = [];
const handles: i64[] = [];

let i: i64 = 0;
while (i < N) {
  const p = promise.new_pending();
  promises.push(p);
  i = i + 1;
}

// Spawna threads resolvendo cada Promise.
let j: i64 = 0;
while (j < N) {
  const h = thread.spawn(resolverWorker, promises[j]);
  handles.push(h);
  j = j + 1;
}

// Wait sequencial — cada um resolve em outra thread.
let k: i64 = 0;
let sum: i64 = 0;
while (k < N) {
  sum = sum + promise.wait(promises[k]);
  k = k + 1;
}

// Join threads.
let m: i64 = 0;
while (m < N) {
  thread.join(handles[m]);
  m = m + 1;
}

print("sum=" + sum);  // N * 1 = 50

// Cenario: resolver com handle de string (handle como i64 cabe no slot).
const s = promise.new_pending();
function stringResolver(p: i64): i64 {
  // Resolve com um valor i64 grande (mas nao gen 0, pra simular handle).
  promise.resolve(p, 281474976710657);  // gen=1 slot=1 — formato handle
  return 0;
}

const sh = thread.spawn(stringResolver, s);
const sv = promise.wait(s);
thread.join(sh);
// (#92) sv vem de promise.wait que agora marca o retorno como ambiguo
// para TPL_COERCE_AUTO resolver em runtime. Renderiza handle como
// [object Object] em vez de raw int. Usa try_value que retorna i64 puro.
print("handle_value=" + promise.try_value(s));

// Cenario: race entre threads — primeiro a settle vence.
const race_p = promise.new_pending();

// Diferenca de sleep grande pra dar margem em CI lento (macOS arm64
// pode atrasar spawn de threads ~50ms). Antes 20ms vs 50ms (30ms gap)
// nao era suficiente — racer2 chegava primeiro em CI as vezes.
function racer1(p: i64): i64 {
  time.sleep_ms(20);
  promise.resolve(p, 100);
  return 0;
}

function racer2(p: i64): i64 {
  time.sleep_ms(200);
  promise.resolve(p, 200);
  return 0;
}

const r1 = thread.spawn(racer1, race_p);
const r2 = thread.spawn(racer2, race_p);
const winner = promise.wait(race_p);
thread.join(r1);
thread.join(r2);
print("winner=" + winner);  // 100 (resolveu primeiro)

describe("promise + threading sob carga", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "sum=50\nhandle_value=281474976710657\nwinner=100\n"
    );
  });
});
