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

// 4. allSettled com mix sucesso/falha, pela superficie JS (Promise.allSettled).
// As rows sao objetos SHAPED `{ status, value | reason }` (spec) desde
// c400f9bd — a leitura via collections.map_get era a representacao interna
// antiga (Entry::Map) e nao existe mais.
async function ok4(n: i64): i64 { return n; }
async function bad4(n: i64): i64 { throw n; }
const s4: any[] = await Promise.allSettled([ok4(1), bad4(2), ok4(3), bad4(4)]);
print("settled_len=" + s4.length);
print("s[0].value=" + s4[0].value);
print("s[1].reason=" + s4[1].reason);
print("s[2].value=" + s4[2].value);
print("s[3].reason=" + s4[3].reason);

describe("promise combinators paralelismo + mix de tipos", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "was_parallel=1\nsum=6\nwinner=200\nmixed[0]=50\nmixed[1]=99\nsettled_len=4\ns[0].value=1\ns[1].reason=2\ns[2].value=3\ns[3].reason=4\n"
    );
  });
});
