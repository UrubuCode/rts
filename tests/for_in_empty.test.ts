import { describe, test, expect } from "rts:test";
import { io, collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// for-in em objeto vazio: nenhum corpo executado.

const obj = collections.map_new(); // map vazio sem inits
print("before");
for (const key in obj) {
    print("UNREACHABLE");
}
print("after");

describe("fixture:for_in_empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("before\nafter\n");
  });
});
