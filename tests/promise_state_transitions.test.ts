import { describe, test, expect } from "rts:test";
import { promise, thread, time, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Pending → fulfilled em outra thread.
const counter = atomic.i64_new(0);

function resolverThread(p: i64): i64 {
  time.sleep_ms(20);
  promise.resolve(p, 111);
  atomic.i64_fetch_add(counter, 1);
  return 0;
}

const p1 = promise.new_pending();
print("p1_state_pending=" + promise.state(p1));
const h1 = thread.spawn(resolverThread, p1);
const v1 = promise.wait(p1);
thread.join(h1);
print("p1_wait=" + v1);
print("p1_state_after=" + promise.state(p1));
print("counter=" + atomic.i64_load(counter));

// 2. Pending → rejected em outra thread.
function rejecterThread(p: i64): i64 {
  time.sleep_ms(10);
  promise.reject(p, 666);
  return 0;
}

const p2 = promise.new_pending();
const h2 = thread.spawn(rejecterThread, p2);
const v2 = promise.wait(p2);
thread.join(h2);
print("p2_wait=" + v2);
print("p2_state=" + promise.state(p2));

// 3. resolve apos reject = no-op.
const p3 = promise.new_pending();
print("p3_reject=" + promise.reject(p3, 50));
print("p3_resolve_apos_reject=" + promise.resolve(p3, 100));
print("p3_state=" + promise.state(p3));
print("p3_value=" + promise.try_value(p3));

// 4. try_value em pending = 0.
const p4 = promise.new_pending();
print("p4_try_value_pending=" + promise.try_value(p4));
promise.resolve(p4, 7);
print("p4_try_value_settled=" + promise.try_value(p4));

// 5. Multiplos waiters da mesma Promise (todos veem o valor).
const p5 = promise.new_pending();
const seen = atomic.i64_new(0);

function waiter(p: i64): i64 {
  const v = promise.wait(p);
  atomic.i64_fetch_add(seen, v);
  return 0;
}

const w1 = thread.spawn(waiter, p5);
const w2 = thread.spawn(waiter, p5);
const w3 = thread.spawn(waiter, p5);
time.sleep_ms(20);
promise.resolve(p5, 13);
thread.join(w1);
thread.join(w2);
thread.join(w3);
print("p5_seen=" + atomic.i64_load(seen));  // 3 waiters * 13 = 39

// 6. wait em handle invalido = 0.
print("invalid_wait=" + promise.wait(0));
print("invalid_state=" + promise.state(0));

describe("promise state transitions", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "p1_state_pending=0\np1_wait=111\np1_state_after=1\ncounter=1\np2_wait=666\np2_state=2\np3_reject=1\np3_resolve_apos_reject=0\np3_state=2\np3_value=50\np4_try_value_pending=0\np4_try_value_settled=7\np5_seen=39\ninvalid_wait=null\ninvalid_state=-1\n"
    );
  });
});
