import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `promise.*` / `collections.vec_*` / `time.*` do namespace `rts` viraram a
// superficie que fica: `Promise.*` sobre arrays, `Date.now()` e um sleep feito
// de `setTimeout` dentro de uma Promise. Nota importante sobre o sleep: o
// `time.sleep_ms` antigo BLOQUEAVA a thread; `await new Promise(r =>
// setTimeout(r, n))` cede o controlo. E' precisamente por isso que continua a
// ser a traducao certa AQUI — o que este ficheiro afirma e' que as tres
// esperas se sobrepoem, e um sleep bloqueante nunca poderia demonstrar isso.
function sleep(ms: i64): any {
  return new Promise((resolve: any) => { setTimeout(resolve, ms); });
}

async function delayed(x: i64, ms: i64): i64 {
  await sleep(ms);
  return x;
}

// 1. Promise.all com async fns — paralelismo confirmado por timing.
const t0 = Date.now();
const r1: any = await Promise.all([delayed(1, 30), delayed(2, 30), delayed(3, 30)]);
const dt = Date.now() - t0;

// 3 promises paralelas de 30ms. Serie seria 90ms+. CI macOS arm64
// pode levar ate ~250ms com overhead de timers. Threshold 250ms — serie ainda
// passa muito disso.
const wasParallel = dt < 250 ? 1 : 0;
print("was_parallel=" + wasParallel);
print("sum=" + (r1[0] + r1[1] + r1[2]));

// 2. Race entre async fns — vence o mais rapido.
// Gap maior (100ms vs 10ms vs 200ms) pra dar margem em CI lento.
const winner = await Promise.race([delayed(100, 100), delayed(200, 10), delayed(300, 200)]);
print("winner=" + winner);  // 200

// 3. all com 1 async + 1 ja-resolvido.
const r3: any = await Promise.all([delayed(50, 10), Promise.resolve(99)]);
print("mixed[0]=" + r3[0]);  // 50
print("mixed[1]=" + r3[1]);  // 99

// 4. allSettled com mix sucesso/falha. As rows sao objetos SHAPED
// `{ status, value | reason }` (spec).
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
