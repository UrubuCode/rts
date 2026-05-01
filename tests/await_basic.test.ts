import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function answer(): i64 {
  return 42;
}

async function double(x: i64): i64 {
  return x * 2;
}

// Cenario 1: await de async fn sem args.
const v1 = await answer();
print("v1=" + v1);

// Cenario 2: await com argumento.
const v2 = await double(21);
print("v2=" + v2);

// Cenario 3: await de Promise explicita.
const p = promise.new_resolved(99);
const v3 = await p;
print("v3=" + v3);

// Cenario 4: await dentro de expressao.
const v4 = (await double(10)) + (await double(5));
print("v4=" + v4);

describe("await codegen (issue #414, F3)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("v1=42\nv2=42\nv3=99\nv4=30\n");
  });
});
