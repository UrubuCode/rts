import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Issue #425: async fn com 2+ params era silently sync.
// Apos fix, args sao empacotados em Vec e desempacotados no watcher.

// 1. Dois argumentos.
async function add(a: i64, b: i64): i64 {
  return a + b;
}
print("add(3,4)=" + (await add(3, 4)));
print("add(100,200)=" + (await add(100, 200)));

// 2. Tres argumentos.
async function sum3(a: i64, b: i64, c: i64): i64 {
  return a + b + c;
}
print("sum3=" + (await sum3(10, 20, 30)));

// 3. Cinco argumentos.
async function sum5(a: i64, b: i64, c: i64, d: i64, e: i64): i64 {
  return a + b + c + d + e;
}
print("sum5=" + (await sum5(1, 2, 3, 4, 5)));

// 4. Multi-arg com early return em loop (combina com #425).
async function range(start: i64, max: i64): i64 {
  for (let i: i64 = start; i < max; i = i + 1) {
    if (i * i > 50) return i;
  }
  return -1;
}
print("range(0,20)=" + (await range(0, 20)));   // 8 (8*8=64>50)
print("range(10,15)=" + (await range(10, 15))); // 10 (10*10=100>50)

// 5. Args com valores grandes.
async function mul(a: i64, b: i64): i64 {
  return a * b;
}
print("mul(1000,1000)=" + (await mul(1000, 1000)));

// 6. Args negativos.
async function diff(a: i64, b: i64): i64 {
  return a - b;
}
print("diff(5,10)=" + (await diff(5, 10)));    // -5
print("diff(-3,-7)=" + (await diff(-3, -7)));  // 4

describe("async fn com multiplos argumentos (#425)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "add(3,4)=7\nadd(100,200)=300\nsum3=60\nsum5=15\nrange(0,20)=8\nrange(10,15)=10\nmul(1000,1000)=1000000\ndiff(5,10)=-5\ndiff(-3,-7)=4\n"
    );
  });
});
