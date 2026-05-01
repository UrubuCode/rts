import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. throw em async fn — try/catch pega.
async function fail(msg: string): i64 { throw msg; }

try {
  await fail("oh no");
} catch (e) {
  print("c1: " + e);
}

// 2. Promise.new_rejected com handle string.
async function reject_with(msg: string): i64 {
  throw msg;
}

try {
  await reject_with("explicit");
} catch (e) {
  print("c2: " + e);
}

// 3. Async fn aninhada — rejection propaga.
async function inner(): i64 { throw "from inner"; }
async function outer(): i64 {
  await inner();
  return 1;
}

try {
  await outer();
} catch (e) {
  print("c3: " + e);
}

// 4. Caminho normal — sem throw, sem catch disparado.
async function ok(x: i64): i64 { return x * 2; }

let result: i64 = -1;
try {
  result = await ok(21);
} catch (e) {
  print("nao deveria: " + e);
}
print("result=" + result);

// 5. Throw em async fn com argumentos.
async function throw_arg(msg: string): i64 {
  throw msg;
}

try {
  await throw_arg("error message");
} catch (e) {
  print("c5: " + e);
}

describe("promise rejection propagation (F5, #416)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "c1: oh no\nc2: explicit\nc3: from inner\nresult=42\nc5: error message\n"
    );
  });
});
