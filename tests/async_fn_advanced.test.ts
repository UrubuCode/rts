import { describe, test, expect } from "rts:test";
import { promise, time, atomic } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. async fn com varias instrucoes no body.
async function compute(x: i64): i64 {
  let r: i64 = x;
  r = r * 2;
  r = r + 10;
  return r;
}

const v1 = await compute(5);
print("compute(5)=" + v1);  // 5*2+10 = 20

// 2. async fn que faz multiplos calculos (sem chamar outra async — issue
// com `await chamada_async()` dentro de async tem bug de F3-fase-2).
async function complex(x: i64): i64 {
  const a: i64 = x * 2;
  const b: i64 = a + 50;
  return b;
}

const v2 = await complex(50);
print("complex(50)=" + v2);  // 50*2+50 = 150

// 3. async fn com if/else.
async function categorize(x: i64): i64 {
  if (x < 10) return 1;
  if (x < 100) return 2;
  return 3;
}

print("cat(5)=" + (await categorize(5)));
print("cat(50)=" + (await categorize(50)));
print("cat(500)=" + (await categorize(500)));

// 4. async fn com loop.
async function sumTo(n: i64): i64 {
  let s: i64 = 0;
  let i: i64 = 1;
  while (i <= n) {
    s = s + i;
    i = i + 1;
  }
  return s;
}

print("sumTo(10)=" + (await sumTo(10)));   // 55
print("sumTo(100)=" + (await sumTo(100))); // 5050

// 5. Multiplas async chamadas paralelas — confirma execucao concorrente.
async function delayed(ms: i64): i64 {
  time.sleep_ms(ms);
  return ms;
}

const t0 = time.now_ms();
const pa = delayed(50);
const pb = delayed(50);
const pc = delayed(50);
// Espera pelos 3. Se executassem em serie seria 150ms+.
// Em paralelo deveria ficar perto de 50ms.
const va = await pa;
const vb = await pb;
const vc = await pc;
const elapsed = time.now_ms() - t0;
print("parallel_sum=" + (va + vb + vc));   // 150
// Tolerancia: paralelo real fica em ~50-100ms; serie seria 150+.
// Threshold 140ms cobre runners de CI lentos (macOS arm64 GH Actions)
// sem perder o teste — serie ainda pula bem alem disso.
const wasParallel = elapsed < 140 ? 1 : 0;
print("was_parallel=" + wasParallel);

// 6. await em valor de retorno usado em expressao.
async function getN(): i64 { return 7; }

const v6 = (await getN()) * 3 + 1;
print("expr=" + v6);  // 22

describe("async function — casos avancados", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "compute(5)=20\ncomplex(50)=150\ncat(5)=1\ncat(50)=2\ncat(500)=3\nsumTo(10)=55\nsumTo(100)=5050\nparallel_sum=150\nwas_parallel=1\nexpr=22\n"
    );
  });
});
