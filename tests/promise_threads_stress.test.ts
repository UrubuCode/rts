import { describe, test, expect } from "rts:test";
import { promise, thread, time, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Stress 1: 100 Promises, 100 threads resolvendo em paralelo, contagem.
const counter = atomic.i64_new(0);

function worker(p: i64): i64 {
  promise.resolve(p, 1);
  atomic.i64_fetch_add(counter, 1);
  return 0;
}

const promises: i64[] = [];
const handles: i64[] = [];
const N: i64 = 100;
let i: i64 = 0;
while (i < N) {
  promises.push(promise.new_pending());
  i = i + 1;
}
let j: i64 = 0;
while (j < N) {
  handles.push(thread.spawn(worker, promises[j]));
  j = j + 1;
}
let waitSum: i64 = 0;
let k: i64 = 0;
while (k < N) {
  waitSum = waitSum + promise.wait(promises[k]);
  k = k + 1;
}
let m: i64 = 0;
while (m < N) {
  thread.join(handles[m]);
  m = m + 1;
}
print("wait_sum=" + waitSum);
print("counter_atomic=" + atomic.i64_load(counter));

// Stress 2: Promise compartilhada por N waiters.
const shared = promise.new_pending();
const sharedSum = atomic.i64_new(0);

function sharedWaiter(p: i64): i64 {
  const v = promise.wait(p);
  atomic.i64_fetch_add(sharedSum, v);
  return 0;
}

// 10 threads esperando a mesma Promise
const wHandles: i64[] = [];
let w: i64 = 0;
while (w < 10) {
  wHandles.push(thread.spawn(sharedWaiter, shared));
  w = w + 1;
}
time.sleep_ms(20);
promise.resolve(shared, 7);
let q: i64 = 0;
while (q < 10) {
  thread.join(wHandles[q]);
  q = q + 1;
}
print("shared_sum=" + atomic.i64_load(sharedSum));  // 10 * 7 = 70

// Stress 3: producer-consumer pattern com Promises.
const slot = promise.new_pending();

function producer(p: i64): i64 {
  time.sleep_ms(15);
  promise.resolve(p, 999);
  return 0;
}

const tStart = time.now_ms();
const ph = thread.spawn(producer, slot);
const value = promise.wait(slot);
const elapsed = time.now_ms() - tStart;
thread.join(ph);
print("prod_value=" + value);
const wasBlocking = elapsed >= 10 ? 1 : 0;
print("prod_blocking=" + wasBlocking);  // 1 (esperou ~15ms)

describe("promise + threading sob stress real", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "wait_sum=100\ncounter_atomic=100\nshared_sum=70\nprod_value=999\nprod_blocking=1\n"
    );
  });
});
