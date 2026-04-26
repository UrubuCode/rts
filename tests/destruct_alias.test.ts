import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Object destructuring com alias: { x: a }.

const obj = { width: 100, height: 50 };
const { width: w, height: h } = obj;

const h1 = gc.string_from_i64(w);
print(h1); gc.string_free(h1); // 100
const h2 = gc.string_from_i64(h);
print(h2); gc.string_free(h2); // 50

describe("fixture:destruct_alias", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n50\n");
  });
});
