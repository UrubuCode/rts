import { describe, test, expect } from "rts:test";
import { promise, collections, time } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function fail(msg: string): i64 { throw msg; }
// (#determinismo) ok() com delay artificial para que race seja
// deterministico: fast_fail sempre vence porque resolve imediato.
async function ok(x: i64): i64 { time.sleep_ms(20); return x; }

// 1. promise.all rejeita na primeira falha.
const v1 = collections.vec_new();
collections.vec_push(v1, ok(1));
collections.vec_push(v1, fail("middle"));
collections.vec_push(v1, ok(3));

try {
  await promise.all(v1);
} catch (e) {
  print("all_rej: " + e);
}

// 2. promise.race com primeiro rejected.
async function fast_fail(msg: string): i64 { throw msg; }
const v2 = collections.vec_new();
collections.vec_push(v2, fast_fail("loser-error"));
collections.vec_push(v2, ok(99));

try {
  await promise.race(v2);
} catch (e) {
  print("race_rej: " + e);
}

// 3. promise.any com TODAS rejeitando.
const v3 = collections.vec_new();
collections.vec_push(v3, fail("a"));
collections.vec_push(v3, fail("b"));
collections.vec_push(v3, fail("c"));

let any_caught: i64 = 0;
try {
  await promise.any(v3);
} catch (e) {
  any_caught = 1;
}
print("any_all_rejected_caught=" + any_caught);

// 4. promise.any com mix — pega primeira fulfilled.
const v4 = collections.vec_new();
collections.vec_push(v4, fail("nope"));
collections.vec_push(v4, ok(7));
collections.vec_push(v4, fail("late"));

const v4r = await promise.any(v4);
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
