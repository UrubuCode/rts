import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// async/await fase 1: síncrono — async é flag aceita, await é no-op.

async function getValue(): Promise<number> {
    return 42;
}

async function compute(): Promise<number> {
    const x = await getValue();
    return x + 8;
}

const r = compute();
const h = gc.string_from_i64(r as number);
print(h); gc.string_free(h); // 50

describe("fixture:async_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("50\n");
  });
});
