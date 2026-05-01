import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. async fn com for-loop tradicional.
async function sumRange(n: i64): i64 {
  let s: i64 = 0;
  for (let i: i64 = 1; i <= n; i = i + 1) {
    s = s + i;
  }
  return s;
}

print("sumRange(10)=" + (await sumRange(10)));   // 55
print("sumRange(100)=" + (await sumRange(100))); // 5050

// 2. async fn com nested loop.
async function matrix(n: i64): i64 {
  let total: i64 = 0;
  for (let i: i64 = 0; i < n; i = i + 1) {
    for (let j: i64 = 0; j < n; j = j + 1) {
      total = total + 1;
    }
  }
  return total;
}

print("matrix(5)=" + (await matrix(5)));   // 25
print("matrix(10)=" + (await matrix(10))); // 100

// 3. async fn com if dentro de loop (sem early return ou flag —
// `let` reassignment funciona OK).
async function countEven(n: i64): i64 {
  let count: i64 = 0;
  let i: i64 = 0;
  while (i < n) {
    if (i % 2 == 0) {
      count = count + 1;
    }
    i = i + 1;
  }
  return count;
}

print("countEven(10)=" + (await countEven(10))); // 5 (0,2,4,6,8)
print("countEven(15)=" + (await countEven(15))); // 8 (0,2,4,6,8,10,12,14)

// 4. async fn com continue.
async function sumOdd(n: i64): i64 {
  let s: i64 = 0;
  for (let i: i64 = 1; i <= n; i = i + 1) {
    if (i % 2 == 0) continue;
    s = s + i;
  }
  return s;
}

print("sumOdd(10)=" + (await sumOdd(10)));  // 1+3+5+7+9 = 25

// 5. async fn com early return.
async function findIdx(target: i64): i64 {
  let i: i64 = 0;
  while (i < 1000) {
    if (i * i >= target) {
      return i;
    }
    i = i + 1;
  }
  return -1;
}

print("findIdx(100)=" + (await findIdx(100)));  // 10 (10*10=100)
print("findIdx(50)=" + (await findIdx(50)));    // 8 (8*8=64 >= 50)

// 6. async fn com do-while.
async function decrement(start: i64): i64 {
  let n: i64 = start;
  let count: i64 = 0;
  do {
    n = n - 1;
    count = count + 1;
  } while (n > 0);
  return count;
}

print("decrement(5)=" + (await decrement(5)));  // 5

describe("async functions com loops e control flow", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "sumRange(10)=55\nsumRange(100)=5050\nmatrix(5)=25\nmatrix(10)=100\ncountEven(10)=5\ncountEven(15)=8\nsumOdd(10)=25\nfindIdx(100)=10\nfindIdx(50)=8\ndecrement(5)=5\n"
    );
  });
});
