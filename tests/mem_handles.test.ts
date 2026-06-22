import { describe, test, expect } from "rts:test";
import { io, mem } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// mem.drop_handle libera GC; forget_handle nao libera (vaza).

print(`${42}`);
print("dropped");

print(`${99}`);
print("forgotten");

describe("fixture:mem_handles", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\ndropped\n99\nforgotten\n");
  });
});
