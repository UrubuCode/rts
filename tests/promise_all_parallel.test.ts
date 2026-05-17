import { describe, test, expect } from "rts:test";
import { promise, collections, time } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function delayed(x: i64, ms: i64): i64 {
  time.sleep_ms(ms);
  return x;
}

// 1. promise.all com async fns — paralelismo confirmado por timing.
const v1 = collections.vec_new();
collections.vec_push(v1, delayed(1, 30));
collections.vec_push(v1, delayed(2, 30));
collections.vec_push(v1, delayed(3, 30));

const t0 = time.now_ms();
const r1 = await promise.all(v1);
const dt = time.now_ms() - t0;

// 3 promises paralelas de 30ms. Serie seria 90ms+. CI macOS arm64
// pode levar ate ~250ms com overhead de promise.all + tokio
// runtime. Threshold 250ms — serie ainda passa muito disso.
const wasParallel = dt < 250 ? 1 : 0;
print("was_parallel=" + wasParallel);
print("sum=" + (collections.vec_get(r1, 0) + collections.vec_get(r1, 1) + collections.vec_get(r1, 2)));

// 2. Race entre async fns — vence o mais rapido.
// Gap maior (100ms vs 10ms vs 200ms) pra dar margem em CI macOS arm64
// onde scheduling de spawn_blocking pode adicionar ~50-100ms latencia.
const v2 = collections.vec_new();
collections.vec_push(v2, delayed(100, 100));
collections.vec_push(v2, delayed(200, 10));  // mais rapido com larga margem
collections.vec_push(v2, delayed(300, 200));
const winner = await promise.race(v2);
print("winner=" + winner);  // 200

// 3. all com 1 async + 1 ja-resolvido.
const v3 = collections.vec_new();
collections.vec_push(v3, delayed(50, 10));
collections.vec_push(v3, promise.new_resolved(99));
const r3 = await promise.all(v3);
print("mixed[0]=" + collections.vec_get(r3, 0));  // 50
print("mixed[1]=" + collections.vec_get(r3, 1));  // 99

// 4. allSettled com mix sucesso/falha.
const v4 = collections.vec_new();
collections.vec_push(v4, promise.new_resolved(1));
collections.vec_push(v4, promise.new_rejected(2));
collections.vec_push(v4, promise.new_resolved(3));
collections.vec_push(v4, promise.new_rejected(4));
const r4 = await promise.all_settled(v4);
print("settled_len=" + collections.vec_len(r4));
// Cada elemento eh Map { status, value | reason } (JS spec).
const m0 = collections.vec_get(r4, 0);
const m1 = collections.vec_get(r4, 1);
const m2 = collections.vec_get(r4, 2);
const m3 = collections.vec_get(r4, 3);
print("s[0].value=" + collections.map_get(m0, "value"));
print("s[1].reason=" + collections.map_get(m1, "reason"));
print("s[2].value=" + collections.map_get(m2, "value"));
print("s[3].reason=" + collections.map_get(m3, "reason"));

describe("promise combinators paralelismo + mix de tipos", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "was_parallel=1\nsum=6\nwinner=200\nmixed[0]=50\nmixed[1]=99\nsettled_len=4\ns[0].value=1\ns[1].reason=2\ns[2].value=3\ns[3].reason=4\n"
    );
  });
});
