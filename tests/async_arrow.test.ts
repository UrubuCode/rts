import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Arrow async + await dentro.

async function fetch(): Promise<number> { return 7; }

const handler = async (n: number): Promise<number> => {
    const v = await fetch();
    return v * n;
};

const r = handler(6);
const h = gc.string_from_i64(r as number);
print(h); gc.string_free(h); // 42

describe("fixture:async_arrow", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n");
  });
});
