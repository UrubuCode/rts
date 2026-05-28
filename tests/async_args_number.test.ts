import { describe, test, expect } from "rts:test";
import { promise } from "rts";

// Regression (cross-runtime): async fn COM ARGUMENTOS number dava arg=0. A inner
// __async_inner_f mantinha params tipados (f64) mas era invocada por
// PROMISE_CREATE como (i64...) com args i64-truncados -> arg chegava 0/lixo.
// Fix: inner recebe params i64-only (__araw_<name>) + prelogo num.f64_from_bits;
// wrapper empacota number via num.f64_to_bits. (Elo f64/invoke em async.)

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function f(n: number): Promise<number> { return n * 10; }
async function add(a: number, b: number): Promise<number> { return a + b; }
async function compute(x: number): Promise<number> {
  const y = x * x;
  return y + 1;
}

// pre-computa no top-level via promise.wait (sincrono).
print("f5=" + promise.wait(f(5)));            // 50
print("add=" + promise.wait(add(3, 4)));      // 7
print("compute=" + promise.wait(compute(4))); // 17
print("float=" + promise.wait(f(2.5)));       // 25

describe("async fn com args number", () => {
  test("args number chegam corretamente na inner", () => {
    expect(__rtsCapturedOutput).toBe("f5=50\nadd=7\ncompute=17\nfloat=25\n");
  });
});
