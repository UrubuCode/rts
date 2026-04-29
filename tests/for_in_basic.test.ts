import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// for-in itera chaves de objeto.

const obj = { foo: 1, bar: 2, baz: 3 };

for (const key in obj) {
    print(key);
}

describe("fixture:for_in_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("foo\nbar\nbaz\n");
  });
});
