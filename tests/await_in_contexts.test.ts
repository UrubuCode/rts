import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function num(x: i64): i64 { return x; }

// 1. await em const declaration.
const a = await num(10);
print("const=" + a);

// 2. await em let + reatribuicao.
let b = await num(20);
b = b + 5;
print("let=" + b);

// 3. await em ambos os lados de operador.
const c = (await num(3)) + (await num(4));
print("bin=" + c);

// 4. await aninhado em chamada de fn normal.
function addOne(x: i64): i64 { return x + 1; }
const d = addOne(await num(99));
print("call_arg=" + d);

// 5. await em condicional ternario.
const cond: i64 = 1;
const e = cond ? (await num(100)) : (await num(200));
print("ternary=" + e);

// 6. await em if-condition.
async function isReady(): i64 { return 1; }
let f: i64 = 0;
if (await isReady()) {
  f = 42;
}
print("if_cond=" + f);

// 7. await em while-condition (limita iteracoes).
async function countdown(x: i64): i64 { return x - 1; }
let n: i64 = 3;
let iters: i64 = 0;
while (n > 0) {
  n = await countdown(n);
  iters = iters + 1;
}
print("loop_iters=" + iters);

// 8. await de Promise pre-resolvida.
const p = promise.new_resolved(777);
print("pre_resolved=" + (await p));

// 10. await em array literal.
const arr: i64[] = [await num(1), await num(2), await num(3)];
print("arr_sum=" + (arr[0] + arr[1] + arr[2]));

describe("await em diferentes contextos", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "const=10\nlet=25\nbin=7\ncall_arg=100\nternary=100\nif_cond=42\nloop_iters=3\npre_resolved=777\narr_sum=6\n"
    );
  });
});
