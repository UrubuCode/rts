import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Array destructuring básico.

const [a, b, c] = [10, 20, 30];

const h1 = gc.string_from_i64(a);
print(h1); gc.string_free(h1); // 10
const h2 = gc.string_from_i64(b);
print(h2); gc.string_free(h2); // 20
const h3 = gc.string_from_i64(c);
print(h3); gc.string_free(h3); // 30

describe("fixture:destruct_array", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n30\n");
  });
});
