import { describe, test, expect } from "rts:test";
import { io, gc, mem } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// mem.drop_handle libera GC; forget_handle nao libera (vaza).

const h1 = gc.string_from_i64(42);
print(h1);
mem.drop_handle(h1);
print("dropped");

const h2 = gc.string_from_i64(99);
print(h2);
mem.forget_handle(h2);
print("forgotten");

describe("fixture:mem_handles", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\ndropped\n99\nforgotten\n");
  });
});
