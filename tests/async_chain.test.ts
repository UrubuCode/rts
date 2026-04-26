import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Cadeia de awaits — cada um avalia em sequência.

async function step1(): Promise<number> { return 10; }
async function step2(x: number): Promise<number> { return x * 2; }
async function step3(x: number): Promise<number> { return x + 5; }

async function pipeline(): Promise<number> {
    const a = await step1();      // 10
    const b = await step2(a);     // 20
    const c = await step3(b);     // 25
    return c;
}

const r = pipeline();
const h = gc.string_from_i64(r as number);
print(h); gc.string_free(h); // 25

describe("fixture:async_chain", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("25\n");
  });
});
