import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Object destructuring básico.

const obj = { x: 5, y: 10 };
const { x, y } = obj;

const h1 = gc.string_from_i64(x);
print(h1); gc.string_free(h1); // 5
const h2 = gc.string_from_i64(y);
print(h2); gc.string_free(h2); // 10

describe("fixture:destruct_object", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n10\n");
  });
});
