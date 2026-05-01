import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// async fn sem args — retorna Promise pendente que resolve com 42.
async function getAnswer(): i64 {
  return 42;
}

const p1 = getAnswer();
print("getAnswer.wait=" + promise.wait(p1));

// async fn com 1 arg.
async function double(x: i64): i64 {
  return x * 2;
}

const p2 = double(21);
print("double(21).wait=" + promise.wait(p2));

// async fn que devolve handle (string) — funciona porque internamente
// e' i64. Caller interpreta.
async function makeNumber(): i64 {
  return 99;
}

const p3 = makeNumber();
print("makeNumber.wait=" + promise.wait(p3));

describe("async function codegen (issue #413, F2)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "getAnswer.wait=42\ndouble(21).wait=42\nmakeNumber.wait=99\n"
    );
  });
});
